import { isWellFormedTrackingToken, TRACKING_LINKS_STORAGE_KEY } from "./tracking-token";

/**
 * Tracking links remembered on this device, only when the visitor asks for it (AGENTS.md
 * §7.5, described in /confidentialite). Stored in localStorage, never sent anywhere. Every
 * access is guarded: storage may be blocked, full or tampered with.
 */

export interface RememberedLink {
  token: string;
  /** Short reminder, e.g. "Idée envoyée le 18 octobre 2026". */
  label: string;
  /** Day the link was remembered, YYYY-MM-DD. */
  savedOn: string;
}

const MAX_REMEMBERED_LINKS = 50;
const MAX_LABEL_LENGTH = 200;

function isRememberedLink(value: unknown): value is RememberedLink {
  if (typeof value !== "object" || value === null) return false;
  const { token, label, savedOn } = value as Record<string, unknown>;
  return (
    typeof token === "string" &&
    isWellFormedTrackingToken(token) &&
    typeof label === "string" &&
    typeof savedOn === "string"
  );
}

export function readRememberedLinks(): RememberedLink[] {
  try {
    const raw = window.localStorage.getItem(TRACKING_LINKS_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isRememberedLink) : [];
  } catch {
    return [];
  }
}

function writeRememberedLinks(links: RememberedLink[]): boolean {
  try {
    if (links.length === 0) {
      window.localStorage.removeItem(TRACKING_LINKS_STORAGE_KEY);
    } else {
      window.localStorage.setItem(TRACKING_LINKS_STORAGE_KEY, JSON.stringify(links));
    }
    return true;
  } catch {
    return false;
  }
}

export function isLinkRemembered(token: string): boolean {
  return readRememberedLinks().some((link) => link.token === token);
}

/** Remembers a link (most recent first). Returns false when storage is unavailable. */
export function rememberLink(token: string, label: string, today: Date = new Date()): boolean {
  if (!isWellFormedTrackingToken(token)) return false;
  const others = readRememberedLinks().filter((link) => link.token !== token);
  const link: RememberedLink = {
    token,
    label: label.slice(0, MAX_LABEL_LENGTH),
    savedOn: today.toISOString().slice(0, 10),
  };
  return writeRememberedLinks([link, ...others].slice(0, MAX_REMEMBERED_LINKS));
}

export function forgetLink(token: string): boolean {
  return writeRememberedLinks(readRememberedLinks().filter((link) => link.token !== token));
}

export function forgetAllLinks(): boolean {
  return writeRememberedLinks([]);
}
