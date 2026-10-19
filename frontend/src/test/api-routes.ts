/**
 * Route table for stubFetch (fetch-mock.ts): `"GET /api/ideas"` → JSON body, Response or
 * function. Matching ignores the query string; a request without a route fails like a network
 * error, so a test never reaches a real server and unexpected calls stay visible.
 */

export type RouteHandler = (request: Request, url: URL) => unknown;

export function apiRoutes(routes: Record<string, unknown>) {
  return async (request: Request): Promise<Response> => {
    const url = new URL(request.url);
    const route = routes[`${request.method} ${url.pathname}`];
    if (route === undefined) {
      throw new TypeError(`Unexpected request in test: ${request.method} ${url.pathname}`);
    }
    const answer: unknown =
      typeof route === "function" ? await (route as RouteHandler)(request, url) : route;
    return answer instanceof Response ? answer : Response.json(answer);
  };
}

/** An error answer in the backend format (AGENTS.md §4.1). */
export function apiError(status: number, code: string, message: string): Response {
  return Response.json({ error: { code, message } }, { status });
}
