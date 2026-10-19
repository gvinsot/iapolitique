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

type RejectionListener = () => void;
const rejectionListeners = new Set<RejectionListener>();

/**
 * The admin API answered 401 to a request carrying the stored token (finding AUTH-3): the token
 * is wrong or was rotated. It is forgotten at once, and the admin session is told to ask for a
 * new one, instead of being sent again with every request of the tab.
 */
export function rejectAdminToken(): void {
  clearAdminToken();
  for (const listener of rejectionListeners) listener();
}

/** Subscribes to rejectAdminToken; returns the unsubscribe function. */
export function onAdminTokenRejected(listener: RejectionListener): () => void {
  rejectionListeners.add(listener);
  return () => {
    rejectionListeners.delete(listener);
  };
}
