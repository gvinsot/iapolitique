/**
 * Tracking tokens are 256 random bits encoded in unpadded base64url (AGENTS.md §7.1), i.e.
 * exactly 43 characters. Checking the shape locally avoids sending obviously broken links
 * (truncated copy-paste) to the API.
 *
 * The token travels in the URL fragment (`/suivi#<token>`), which browsers never send to a
 * server, and reaches the API only in the body of a POST: it appears in no request URL, hence
 * in no access log.
 */

/**
 * localStorage key of the tracking links the visitor explicitly asked to remember on this
 * device (AGENTS.md §7.5, described in /confidentialite).
 */
export const TRACKING_LINKS_STORAGE_KEY = "iapolitique-tracking-links";

export const TRACKING_PATH = "/suivi";

const TRACKING_TOKEN_PATTERN = /^[A-Za-z0-9_-]{43}$/;

export function isWellFormedTrackingToken(token: string | null | undefined): token is string {
  return typeof token === "string" && TRACKING_TOKEN_PATTERN.test(token);
}

/** Relative tracking link: the token is in the fragment, never in the path or the query. */
export function trackingHref(token: string): string {
  return `${TRACKING_PATH}#${token}`;
}

/** Absolute tracking link given once to the author, e.g. https://iapolitique.fr/suivi#… */
export function trackingUrl(token: string, origin: string = window.location.origin): string {
  return `${origin}${trackingHref(token)}`;
}

/** Token carried by a location fragment ("#abc…"), or null. */
export function tokenFromHash(hash: string): string | null {
  const value = hash.startsWith("#") ? hash.slice(1) : hash;
  let decoded: string;
  try {
    decoded = decodeURIComponent(value).trim();
  } catch {
    return null;
  }
  return isWellFormedTrackingToken(decoded) ? decoded : null;
}

/**
 * Token typed or pasted by the visitor: the bare token, a tracking link (`…/suivi#token`) or a
 * link in the former format (`…/suivi/token`).
 */
export function parseTrackingInput(input: string): string | null {
  const value = input.trim();
  if (TRACKING_TOKEN_PATTERN.test(value)) return value;
  const hashIndex = value.indexOf("#");
  if (hashIndex >= 0) return tokenFromHash(value.slice(hashIndex));
  const legacy = /\/suivi\/([A-Za-z0-9_%-]+)\/?$/.exec(value);
  return legacy?.[1] ? tokenFromHash(legacy[1]) : null;
}
