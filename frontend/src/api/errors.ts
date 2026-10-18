/**
 * Error normalisation. Every failed API call surfaces as an ApiError, whatever went wrong:
 * the backend JSON error `{ "error": { "code", "message" } }` (AGENTS.md §4.1), an unexpected
 * body (proxy error page, WAF block), or no response at all (network down).
 */

export interface ApiErrorBody {
  error: { code: string; message: string };
}

/** Status used for failures without an HTTP response. */
export const NETWORK_ERROR_STATUS = 0;

export class ApiError extends Error {
  override readonly name = "ApiError";
  /** HTTP status, or NETWORK_ERROR_STATUS when the server could not be reached. */
  readonly status: number;
  /** Stable snake_case code from the backend, or a fallback derived from the status. */
  readonly code: string;
  /** Seconds to wait before retrying, from the Retry-After header (429, 503). */
  readonly retryAfterSeconds: number | null;

  constructor(options: {
    status: number;
    code: string;
    message: string;
    retryAfterSeconds?: number | null;
    cause?: unknown;
  }) {
    super(options.message, { cause: options.cause });
    this.status = options.status;
    this.code = options.code;
    this.retryAfterSeconds = options.retryAfterSeconds ?? null;
  }

  get isNetworkError(): boolean {
    return this.status === NETWORK_ERROR_STATUS;
  }

  /** Worth retrying automatically: the request itself was fine. */
  get isTransient(): boolean {
    return this.isNetworkError || this.status === 502 || this.status === 503 || this.status === 504;
  }
}

const fallbacks: Record<number, { code: string; message: string }> = {
  400: { code: "bad_request", message: "La requête est invalide." },
  401: { code: "unauthorized", message: "Une authentification est nécessaire." },
  403: { code: "forbidden", message: "Accès refusé." },
  404: { code: "not_found", message: "Ressource introuvable." },
  409: { code: "conflict", message: "Cette action entre en conflit avec l’état actuel." },
  413: { code: "payload_too_large", message: "Le contenu envoyé est trop volumineux." },
  422: { code: "validation_error", message: "Certaines informations sont invalides." },
  429: {
    code: "rate_limited",
    message: "Trop de demandes en peu de temps. Merci de patienter avant de réessayer.",
  },
  503: {
    code: "service_unavailable",
    message: "Le service est momentanément indisponible. Réessayez dans quelques instants.",
  },
};

const SERVER_ERROR = {
  code: "server_error",
  message: "Le service rencontre un problème. Réessayez dans quelques instants.",
};
const CLIENT_ERROR = { code: "request_error", message: "La requête n’a pas pu aboutir." };
const UNEXPECTED_ERROR = {
  code: "unexpected_error",
  message: "Une erreur inattendue est survenue.",
};
const NETWORK_ERROR = {
  code: "network_error",
  message: "Impossible de joindre le serveur. Vérifiez votre connexion puis réessayez.",
};

export function isApiErrorBody(value: unknown): value is ApiErrorBody {
  if (typeof value !== "object" || value === null || !("error" in value)) return false;
  const { error } = value;
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.code === "string" &&
    typeof error.message === "string" &&
    error.code.length > 0 &&
    error.message.length > 0
  );
}

function fallbackFor(status: number) {
  return fallbacks[status] ?? (status >= 500 ? SERVER_ERROR : CLIENT_ERROR);
}

/** Parses Retry-After (delta-seconds or HTTP date). */
export function parseRetryAfter(value: string | null, now: number = Date.now()): number | null {
  if (!value) return null;
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return Number(trimmed);
  const date = Date.parse(trimmed);
  if (Number.isNaN(date)) return null;
  return Math.max(0, Math.ceil((date - now) / 1000));
}

/** Builds an ApiError from an HTTP status and an already-parsed body. */
export function apiErrorFromBody(
  status: number,
  body: unknown,
  retryAfterSeconds: number | null = null,
): ApiError {
  const { code, message } = isApiErrorBody(body) ? body.error : fallbackFor(status);
  return new ApiError({ status, code, message, retryAfterSeconds });
}

/** Reads a failed response (any content type) into an ApiError. Never throws. */
export async function apiErrorFromResponse(response: Response): Promise<ApiError> {
  let body: unknown = null;
  try {
    const text = await response.text();
    body = text ? (JSON.parse(text) as unknown) : null;
  } catch {
    body = null;
  }
  return apiErrorFromBody(
    response.status,
    body,
    parseRetryAfter(response.headers.get("Retry-After")),
  );
}

export function networkError(cause: unknown): ApiError {
  return new ApiError({ status: NETWORK_ERROR_STATUS, ...NETWORK_ERROR, cause });
}

export function isAbortError(error: unknown): boolean {
  return (
    (error instanceof DOMException || error instanceof Error) &&
    (error.name === "AbortError" || error.name === "TimeoutError")
  );
}

const EMPTY_ANSWER = {
  code: "unexpected_answer",
  message: "Le serveur a envoyé une réponse inattendue. Réessayez dans quelques instants.",
};

/**
 * The body of a successful call. Errors already threw (client middleware); an empty body where
 * one is documented (proxy page, truncated answer) becomes an ApiError too.
 */
export function requireData<T>(data: T | undefined): T {
  if (data === undefined) throw new ApiError({ status: -1, ...EMPTY_ANSWER });
  return data;
}

/** Any thrown value as an ApiError (unknown errors keep their cause for debugging). */
export function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  return new ApiError({ status: -1, ...UNEXPECTED_ERROR, cause: error });
}

/** French message to show to the visitor for any thrown value. */
export function errorMessage(error: unknown): string {
  return toApiError(error).message;
}
