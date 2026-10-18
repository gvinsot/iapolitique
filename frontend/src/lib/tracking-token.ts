/**
 * Tracking tokens are 256 random bits encoded in unpadded base64url (AGENTS.md §7.1), i.e.
 * exactly 43 characters. Checking the shape locally avoids sending obviously broken links
 * (truncated copy-paste) to the API.
 */
/**
 * localStorage key of the tracking links the visitor explicitly asked to remember on this
 * device (AGENTS.md §7.5, described in /confidentialite).
 */
export const TRACKING_LINKS_STORAGE_KEY = "iapolitique-tracking-links";

const TRACKING_TOKEN_PATTERN = /^[A-Za-z0-9_-]{43}$/;

export function isWellFormedTrackingToken(token: string | undefined): token is string {
  return token !== undefined && TRACKING_TOKEN_PATTERN.test(token);
}
