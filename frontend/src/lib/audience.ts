/**
 * Opt-out of the audience measurement (self-hosted Umami, AGENTS.md §1, §7.5). The Umami script
 * of index.html sends nothing while this localStorage key is set; every storage access is
 * guarded: private browsing or blocked storage must never break the page.
 */

/** Key read by the Umami script itself: do not rename. */
export const AUDIENCE_OPT_OUT_KEY = "umami.disabled";

export function isAudienceOptedOut(): boolean {
  try {
    return window.localStorage.getItem(AUDIENCE_OPT_OUT_KEY) !== null;
  } catch {
    return false;
  }
}

/** Records the visitor's choice; returns false when the browser refuses local storage. */
export function setAudienceOptOut(optOut: boolean): boolean {
  try {
    if (optOut) {
      window.localStorage.setItem(AUDIENCE_OPT_OUT_KEY, "1");
    } else {
      window.localStorage.removeItem(AUDIENCE_OPT_OUT_KEY);
    }
    return true;
  } catch {
    return false;
  }
}
