/**
 * Admin API token (AGENTS.md §8: `Authorization: Bearer $ADMIN_API_TOKEN`, defence in depth on
 * top of the LAN allow-list). Kept in sessionStorage only: it disappears with the tab, is never
 * a cookie and never reaches the public API. Every access is guarded: storage may be blocked.
 */

export const ADMIN_TOKEN_KEY = "iapolitique-admin-token";

export function readAdminToken(): string | null {
  try {
    const token = window.sessionStorage.getItem(ADMIN_TOKEN_KEY);
    return token && token.trim() !== "" ? token : null;
  } catch {
    return null;
  }
}

export function storeAdminToken(token: string): boolean {
  try {
    window.sessionStorage.setItem(ADMIN_TOKEN_KEY, token.trim());
    return true;
  } catch {
    return false;
  }
}

export function clearAdminToken(): void {
  try {
    window.sessionStorage.removeItem(ADMIN_TOKEN_KEY);
  } catch {
    // Storage unavailable: nothing was stored.
  }
}
