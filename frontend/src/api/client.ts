import createFetchClient, { type Client, type Middleware } from "openapi-fetch";
import createQueryHooks from "openapi-react-query";

import { readAdminToken, rejectAdminToken } from "./admin-token";
import { apiErrorFromResponse, isAbortError, networkError } from "./errors";
import type { paths } from "./schema";

/**
 * Typed API client, generated types from the backend OpenAPI spec (src/api/schema.d.ts).
 *
 * Paths in the spec are the served paths, `/api` prefix included (AGENTS.md §13.3), and are
 * resolved against the current origin: the browser always calls same-origin relative `/api/...`
 * URLs (Traefik routes them in production, the Vite proxy in development).
 */

export const ADMIN_API_PREFIX = "/api/admin";

function currentOrigin(): string {
  return typeof window === "undefined" ? "" : window.location.origin;
}

/** Turns every failure into an ApiError: HTTP errors and network errors alike. */
export const errorMiddleware: Middleware = {
  async onResponse({ response }) {
    if (!response.ok) throw await apiErrorFromResponse(response);
    return undefined;
  },
  onError({ error }) {
    // Cancellations (query unmounted, AbortSignal.timeout) stay as they are.
    if (isAbortError(error)) return undefined;
    return networkError(error);
  },
};

function isAdminPath(pathname: string): boolean {
  return pathname === ADMIN_API_PREFIX || pathname.startsWith(`${ADMIN_API_PREFIX}/`);
}

/** Adds the admin bearer token to admin API calls only, never to the public API. */
export function adminAuthMiddleware(getToken: () => string | null): Middleware {
  return {
    onRequest({ request }) {
      if (!isAdminPath(new URL(request.url).pathname)) return undefined;
      const token = getToken();
      if (token) request.headers.set("Authorization", `Bearer ${token}`);
      return undefined;
    },
  };
}

/**
 * Forgets an admin token that the admin API refused (401 to a request that carried it, finding
 * AUTH-3). Registered after errorMiddleware: openapi-fetch runs `onResponse` in reverse order,
 * so this one sees the response before errorMiddleware turns it into an ApiError.
 */
export function adminRejectionMiddleware(onRejected: () => void): Middleware {
  return {
    onResponse({ request, response }) {
      if (
        response.status === 401 &&
        isAdminPath(new URL(request.url).pathname) &&
        request.headers.has("Authorization")
      ) {
        onRejected();
      }
      return undefined;
    },
  };
}

export interface ApiClientOptions {
  baseUrl?: string;
  fetch?: (request: Request) => Promise<Response>;
  getAdminToken?: () => string | null;
  /** Called when the admin API refuses the token sent (defaults to rejectAdminToken). */
  onAdminTokenRejected?: () => void;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type -- mirrors openapi-fetch's own constraint
export function createApiClient<Paths extends {} = paths>(
  options: ApiClientOptions = {},
): Client<Paths> {
  const client = createFetchClient<Paths>({
    baseUrl: options.baseUrl ?? currentOrigin(),
    // Resolved at call time (not captured at creation) so tests can stub the global fetch.
    fetch: options.fetch ?? ((request: Request) => globalThis.fetch(request)),
    headers: { Accept: "application/json" },
    // Same-origin API, no cookie anyway (AGENTS.md §7.1): never send credentials.
    credentials: "omit",
  });
  client.use(
    adminAuthMiddleware(options.getAdminToken ?? readAdminToken),
    errorMiddleware,
    adminRejectionMiddleware(options.onAdminTokenRejected ?? rejectAdminToken),
  );
  return client;
}

/** Shared client for imperative calls (`apiClient.POST("/api/ideas", …)`). */
export const apiClient = createApiClient();

/**
 * TanStack Query hooks bound to the typed client:
 * `$api.useQuery("get", "/api/ideas", { params: { query: { sort: "score" } } })`.
 * Failed calls reject with an ApiError (see errors.ts).
 */
export const $api = createQueryHooks(apiClient);
