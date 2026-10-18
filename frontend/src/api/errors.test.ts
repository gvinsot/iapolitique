import { describe, expect, it } from "vitest";

import {
  ApiError,
  apiErrorFromBody,
  apiErrorFromResponse,
  errorMessage,
  isApiErrorBody,
  networkError,
  parseRetryAfter,
  toApiError,
} from "./errors";

describe("isApiErrorBody", () => {
  it("recognises the backend error format only", () => {
    expect(isApiErrorBody({ error: { code: "not_found", message: "Idée introuvable." } })).toBe(
      true,
    );
    expect(isApiErrorBody({ error: { code: "", message: "x" } })).toBe(false);
    expect(isApiErrorBody({ error: "boom" })).toBe(false);
    expect(isApiErrorBody({ message: "boom" })).toBe(false);
    expect(isApiErrorBody(null)).toBe(false);
    expect(isApiErrorBody("<html>")).toBe(false);
  });
});

describe("apiErrorFromResponse", () => {
  it("keeps the backend code and French message", async () => {
    const response = Response.json(
      { error: { code: "idea_too_short", message: "Votre idée est trop courte." } },
      { status: 422 },
    );
    const error = await apiErrorFromResponse(response);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 422,
      code: "idea_too_short",
      message: "Votre idée est trop courte.",
      retryAfterSeconds: null,
    });
  });

  it("falls back to a status-based error for non-JSON bodies", async () => {
    const response = new Response("<html>Bad Gateway</html>", {
      status: 502,
      headers: { "Content-Type": "text/html" },
    });
    const error = await apiErrorFromResponse(response);
    expect(error).toMatchObject({ status: 502, code: "server_error" });
    expect(error.isTransient).toBe(true);
  });

  it("handles empty bodies and Retry-After on rate limiting", async () => {
    const response = new Response(null, { status: 429, headers: { "Retry-After": "120" } });
    const error = await apiErrorFromResponse(response);
    expect(error).toMatchObject({ status: 429, code: "rate_limited", retryAfterSeconds: 120 });
    expect(error.message).toMatch(/Trop de demandes/);
    expect(error.isTransient).toBe(false);
  });

  it("maps unknown statuses to generic client and server errors", () => {
    expect(apiErrorFromBody(418, null).code).toBe("request_error");
    expect(apiErrorFromBody(507, null).code).toBe("server_error");
    expect(apiErrorFromBody(404, { unexpected: true }).message).toBe("Ressource introuvable.");
  });
});

describe("parseRetryAfter", () => {
  it("reads seconds and HTTP dates", () => {
    expect(parseRetryAfter(null)).toBeNull();
    expect(parseRetryAfter("30")).toBe(30);
    const now = Date.parse("2026-10-18T12:00:00Z");
    expect(parseRetryAfter("Sun, 18 Oct 2026 12:01:00 GMT", now)).toBe(60);
    expect(parseRetryAfter("Sun, 18 Oct 2026 11:00:00 GMT", now)).toBe(0);
    expect(parseRetryAfter("bientôt")).toBeNull();
  });
});

describe("toApiError and errorMessage", () => {
  it("wraps network failures", () => {
    const error = networkError(new TypeError("Failed to fetch"));
    expect(error).toMatchObject({ status: 0, code: "network_error" });
    expect(error.isNetworkError).toBe(true);
    expect(error.isTransient).toBe(true);
    expect(error.cause).toBeInstanceOf(TypeError);
  });

  it("never exposes raw exception messages to visitors", () => {
    const bug = new Error("Cannot read properties of undefined");
    expect(toApiError(bug)).toMatchObject({ code: "unexpected_error", cause: bug });
    expect(errorMessage(bug)).toBe("Une erreur inattendue est survenue.");
    const apiError = apiErrorFromBody(404, null);
    expect(toApiError(apiError)).toBe(apiError);
  });
});
