import { vi } from "vitest";

type Handler = (request: Request) => Response | Promise<Response>;

/**
 * Replaces the global fetch for one test (restored by `unstubGlobals`). Tests never reach a real
 * server: any request the handler does not expect should make it throw.
 */
export function stubFetch(handler: Handler) {
  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) =>
    handler(input instanceof Request ? input : new Request(input, init)),
  );
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

/** Answers the admin session check as the backend does. */
export function adminApi(validToken: string): Handler {
  return (request) => {
    const url = new URL(request.url);
    if (url.pathname !== "/api/admin/session") {
      throw new Error(`Unexpected request in test: ${request.method} ${url.pathname}`);
    }
    if (request.headers.get("Authorization") === `Bearer ${validToken}`) {
      return Response.json({ authenticated: true });
    }
    return Response.json(
      { error: { code: "unauthorized", message: "Jeton d’administration invalide." } },
      { status: 401 },
    );
  };
}
