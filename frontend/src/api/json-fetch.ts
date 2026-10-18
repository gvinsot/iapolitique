import { apiErrorFromResponse, isAbortError, networkError } from "./errors";

/**
 * Same-origin JSON call for the endpoints not yet in the generated OpenAPI types (AGENTS.md
 * §13.2 lists them; replace by `apiClient` once their types are generated). Failures become
 * ApiError like every other call; the answer is validated by `parse` at runtime.
 */
export async function requestJson<T>(
  method: "GET" | "POST",
  path: `/api/${string}`,
  parse: (value: unknown) => T | null,
  options: { body?: unknown; signal?: AbortSignal } = {},
): Promise<T> {
  let response: Response;
  try {
    response = await globalThis.fetch(new URL(path, window.location.origin), {
      method,
      headers:
        options.body === undefined
          ? { Accept: "application/json" }
          : { Accept: "application/json", "Content-Type": "application/json" },
      body: options.body === undefined ? null : JSON.stringify(options.body),
      // No cookie exists anyway (AGENTS.md §7.1): never send credentials.
      credentials: "omit",
      // Tracking pages must never leak their address (and the token in its fragment).
      referrerPolicy: "no-referrer",
      signal: options.signal ?? null,
    });
  } catch (error) {
    if (isAbortError(error)) throw error;
    throw networkError(error);
  }
  if (!response.ok) throw await apiErrorFromResponse(response);
  let value: unknown = null;
  if (response.status !== 204) {
    const text = await response.text();
    value = text ? (JSON.parse(text) as unknown) : null;
  }
  const parsed = parse(value);
  if (parsed === null) throw new Error(`Unexpected answer from ${method} ${path}`);
  return parsed;
}
