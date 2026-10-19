import { renderHook, waitFor } from "@testing-library/react";
import createQueryHooks from "openapi-react-query";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { AppProviders } from "@/app/providers";

import { ADMIN_TOKEN_KEY, readAdminToken, storeAdminToken } from "./admin-token";
import { createApiClient } from "./client";
import { ApiError } from "./errors";
import { createQueryClient } from "./query-client";

/** Minimal spec in the shape openapi-typescript generates, independent of the real API. */
interface TestPaths {
  "/api/health": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: {
      parameters: { query?: never; header?: never; path?: never; cookie?: never };
      requestBody?: never;
      responses: {
        200: { headers: Record<string, unknown>; content: { "application/json": { ok: boolean } } };
      };
    };
  };
  "/api/admin/stats": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: {
      parameters: { query?: never; header?: never; path?: never; cookie?: never };
      requestBody?: never;
      responses: {
        200: {
          headers: Record<string, unknown>;
          content: { "application/json": { queued: number } };
        };
      };
    };
  };
}

function fakeFetch(respond: (request: Request) => Response | Promise<Response>) {
  return vi.fn(async (request: Request) => respond(request));
}

describe("createApiClient", () => {
  it("calls same-origin /api paths and returns typed data", async () => {
    const fetch = fakeFetch(() => Response.json({ ok: true }));
    const client = createApiClient<TestPaths>({ fetch });

    const { data } = await client.GET("/api/health");

    expect(data).toEqual({ ok: true });
    const request = fetch.mock.calls[0]![0];
    expect(request.url).toBe(`${window.location.origin}/api/health`);
    expect(request.headers.get("Accept")).toBe("application/json");
    expect(request.credentials).toBe("omit");
  });

  it("rejects with a normalised ApiError on HTTP errors", async () => {
    const fetch = fakeFetch(() =>
      Response.json(
        { error: { code: "not_found", message: "Idée introuvable." } },
        { status: 404 },
      ),
    );
    const client = createApiClient<TestPaths>({ fetch });

    const failure = client.GET("/api/health");
    await expect(failure).rejects.toBeInstanceOf(ApiError);
    await expect(failure).rejects.toMatchObject({
      status: 404,
      code: "not_found",
      message: "Idée introuvable.",
    });
  });

  it("rejects with a network ApiError when the server is unreachable", async () => {
    const fetch = vi.fn(() => Promise.reject(new TypeError("Failed to fetch")));
    const client = createApiClient<TestPaths>({ fetch });

    await expect(client.GET("/api/health")).rejects.toMatchObject({
      status: 0,
      code: "network_error",
    });
  });

  it("lets cancellations through untouched", async () => {
    const abort = new DOMException("The operation was aborted.", "AbortError");
    const client = createApiClient<TestPaths>({ fetch: vi.fn(() => Promise.reject(abort)) });

    await expect(client.GET("/api/health")).rejects.toBe(abort);
  });

  it("sends the admin token to the admin API only", async () => {
    const fetch = fakeFetch((request) =>
      request.url.endsWith("/api/health")
        ? Response.json({ ok: true })
        : Response.json({ queued: 3 }),
    );
    const client = createApiClient<TestPaths>({ fetch, getAdminToken: () => "s3cret" });

    await client.GET("/api/health");
    await client.GET("/api/admin/stats");

    expect(fetch.mock.calls[0]![0].headers.get("Authorization")).toBeNull();
    expect(fetch.mock.calls[1]![0].headers.get("Authorization")).toBe("Bearer s3cret");
  });

  it("omits the Authorization header when no admin token is stored", async () => {
    const fetch = fakeFetch(() => Response.json({ queued: 0 }));
    const client = createApiClient<TestPaths>({ fetch, getAdminToken: () => null });

    await client.GET("/api/admin/stats");

    expect(fetch.mock.calls[0]![0].headers.has("Authorization")).toBe(false);
  });

  it("forgets the admin token the admin API refuses, and only that one (AUTH-3)", async () => {
    const unauthorized = () =>
      Response.json(
        { error: { code: "unauthorized", message: "Jeton d’administration invalide." } },
        { status: 401 },
      );
    const onAdminTokenRejected = vi.fn();
    const client = createApiClient<TestPaths>({
      fetch: fakeFetch(unauthorized),
      getAdminToken: () => "ancien-jeton",
      onAdminTokenRejected,
    });
    await expect(client.GET("/api/admin/stats")).rejects.toMatchObject({ status: 401 });
    expect(onAdminTokenRejected).toHaveBeenCalledOnce();

    // A token-less probe (401 by design) and a public 401 never drop anything.
    const probe = createApiClient<TestPaths>({
      fetch: fakeFetch(unauthorized),
      getAdminToken: () => null,
      onAdminTokenRejected,
    });
    await expect(probe.GET("/api/admin/stats")).rejects.toMatchObject({ status: 401 });
    const publicClient = createApiClient<TestPaths>({
      fetch: fakeFetch(unauthorized),
      getAdminToken: () => "ancien-jeton",
      onAdminTokenRejected,
    });
    await expect(publicClient.GET("/api/health")).rejects.toMatchObject({ status: 401 });
    expect(onAdminTokenRejected).toHaveBeenCalledOnce();
  });

  it("clears the stored admin token on a 401 by default", async () => {
    storeAdminToken("ancien-jeton");
    const client = createApiClient<TestPaths>({
      fetch: fakeFetch(() =>
        Response.json(
          { error: { code: "unauthorized", message: "Jeton d’administration invalide." } },
          { status: 401 },
        ),
      ),
    });
    await expect(client.GET("/api/admin/stats")).rejects.toBeInstanceOf(ApiError);
    expect(readAdminToken()).toBeNull();
    expect(window.sessionStorage.getItem(ADMIN_TOKEN_KEY)).toBeNull();
  });
});

describe("query hooks", () => {
  function wrapper({ children }: { children: ReactNode }) {
    return <AppProviders queryClient={createQueryClient()}>{children}</AppProviders>;
  }

  it("loads data through TanStack Query", async () => {
    const fetch = fakeFetch(() => Response.json({ ok: true }));
    const $test = createQueryHooks(createApiClient<TestPaths>({ fetch }));

    const { result } = renderHook(() => $test.useQuery("get", "/api/health"), { wrapper });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });
    expect(result.current.data).toEqual({ ok: true });
  });

  it("exposes an ApiError and does not retry client errors", async () => {
    const fetch = fakeFetch(() =>
      Response.json({ error: { code: "forbidden", message: "Accès refusé." } }, { status: 403 }),
    );
    const $test = createQueryHooks(createApiClient<TestPaths>({ fetch }));

    const { result } = renderHook(() => $test.useQuery("get", "/api/admin/stats"), { wrapper });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(result.current.error).toBeInstanceOf(ApiError);
    expect(result.current.error).toMatchObject({ code: "forbidden" });
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});
