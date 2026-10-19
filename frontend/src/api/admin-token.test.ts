import { describe, expect, it, vi } from "vitest";

import { ADMIN_TOKEN_KEY, clearAdminToken, readAdminToken, storeAdminToken } from "./admin-token";

/** AGENTS.md §14.4: the admin token lives in memory or sessionStorage, never in localStorage. */
describe("admin token storage", () => {
  it("keeps the token in sessionStorage only", () => {
    expect(storeAdminToken("  jeton-admin-de-test  ")).toBe(true);
    expect(window.sessionStorage.getItem(ADMIN_TOKEN_KEY)).toBe("jeton-admin-de-test");
    expect(readAdminToken()).toBe("jeton-admin-de-test");
    expect(window.localStorage.length).toBe(0);
    expect(document.cookie).toBe("");
  });

  it("never touches localStorage, even to read", () => {
    const local = vi.spyOn(Storage.prototype, "getItem");
    window.localStorage.setItem(ADMIN_TOKEN_KEY, "jeton-planté-dans-localStorage");
    expect(readAdminToken()).toBeNull();
    for (const call of local.mock.contexts) expect(call).not.toBe(window.localStorage);
  });

  it("forgets the token on sign-out", () => {
    storeAdminToken("jeton-admin-de-test");
    clearAdminToken();
    expect(readAdminToken()).toBeNull();
    expect(window.sessionStorage.length).toBe(0);
  });

  it("ignores a blank token", () => {
    window.sessionStorage.setItem(ADMIN_TOKEN_KEY, "   ");
    expect(readAdminToken()).toBeNull();
  });

  it("degrades without storage instead of falling back to localStorage", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    expect(storeAdminToken("jeton-admin-de-test")).toBe(false);
    expect(window.localStorage.length).toBe(0);
  });
});
