import { toast } from "sonner";
import { describe, expect, it, vi } from "vitest";

import { apiErrorFromBody, networkError } from "./errors";
import { createQueryClient, queryRetryDelay, shouldRetryQuery } from "./query-client";

describe("shouldRetryQuery", () => {
  it("retries transient failures at most twice", () => {
    const offline = networkError(new TypeError("Failed to fetch"));
    expect(shouldRetryQuery(0, offline)).toBe(true);
    expect(shouldRetryQuery(1, offline)).toBe(true);
    expect(shouldRetryQuery(2, offline)).toBe(false);
    expect(shouldRetryQuery(0, apiErrorFromBody(503, null))).toBe(true);
  });

  it("never retries client errors, server bugs or unknown errors", () => {
    expect(shouldRetryQuery(0, apiErrorFromBody(404, null))).toBe(false);
    expect(shouldRetryQuery(0, apiErrorFromBody(429, null))).toBe(false);
    expect(shouldRetryQuery(0, apiErrorFromBody(500, null))).toBe(false);
    expect(shouldRetryQuery(0, new Error("bug"))).toBe(false);
  });
});

describe("queryRetryDelay", () => {
  it("backs off exponentially, honouring Retry-After", () => {
    expect(queryRetryDelay(0, null)).toBe(1000);
    expect(queryRetryDelay(1, null)).toBe(2000);
    expect(queryRetryDelay(10, null)).toBe(10_000);
    expect(queryRetryDelay(0, apiErrorFromBody(503, null, 5))).toBe(5000);
    expect(queryRetryDelay(0, apiErrorFromBody(503, null, 3600))).toBe(30_000);
  });
});

describe("mutation errors", () => {
  it("are shown in a toast unless the caller opts out", async () => {
    const toastError = vi.spyOn(toast, "error");
    const client = createQueryClient();
    const failure = apiErrorFromBody(429, null);

    await expect(
      client
        .getMutationCache()
        .build(client, { mutationFn: () => Promise.reject(failure) })
        .execute(undefined),
    ).rejects.toBe(failure);
    expect(toastError).toHaveBeenCalledWith(failure.message);

    toastError.mockClear();
    await expect(
      client
        .getMutationCache()
        .build(client, {
          mutationFn: () => Promise.reject(failure),
          meta: { errorToast: false },
        })
        .execute(undefined),
    ).rejects.toBe(failure);
    expect(toastError).not.toHaveBeenCalled();
  });
});
