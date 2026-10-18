import { describe, expect, it, vi } from "vitest";

import {
  forgetAllLinks,
  forgetLink,
  isLinkRemembered,
  readRememberedLinks,
  rememberLink,
} from "./tracking-storage";
import { TRACKING_LINKS_STORAGE_KEY } from "./tracking-token";

const TOKEN_A = "A".repeat(43);
const TOKEN_B = "B".repeat(43);
const TODAY = new Date("2026-10-18T10:00:00Z");

describe("remembered tracking links", () => {
  it("stores nothing until asked", () => {
    expect(readRememberedLinks()).toEqual([]);
    expect(window.localStorage.getItem(TRACKING_LINKS_STORAGE_KEY)).toBeNull();
  });

  it("remembers, deduplicates (most recent first) and forgets links", () => {
    expect(rememberLink(TOKEN_A, "Idée", TODAY)).toBe(true);
    expect(rememberLink(TOKEN_B, "Signalement", TODAY)).toBe(true);
    expect(rememberLink(TOKEN_A, "Idée (bis)", TODAY)).toBe(true);
    expect(readRememberedLinks()).toEqual([
      { token: TOKEN_A, label: "Idée (bis)", savedOn: "2026-10-18" },
      { token: TOKEN_B, label: "Signalement", savedOn: "2026-10-18" },
    ]);
    expect(isLinkRemembered(TOKEN_B)).toBe(true);

    forgetLink(TOKEN_B);
    expect(isLinkRemembered(TOKEN_B)).toBe(false);
    forgetAllLinks();
    expect(window.localStorage.getItem(TRACKING_LINKS_STORAGE_KEY)).toBeNull();
  });

  it("refuses malformed tokens and ignores tampered entries", () => {
    expect(rememberLink("trop-court", "Idée", TODAY)).toBe(false);
    window.localStorage.setItem(
      TRACKING_LINKS_STORAGE_KEY,
      JSON.stringify([{ token: "x" }, { token: TOKEN_A, label: "Idée", savedOn: "2026-10-18" }, 3]),
    );
    expect(readRememberedLinks()).toHaveLength(1);
    window.localStorage.setItem(TRACKING_LINKS_STORAGE_KEY, "{pas du json");
    expect(readRememberedLinks()).toEqual([]);
  });

  it("survives a blocked storage", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    expect(readRememberedLinks()).toEqual([]);
    expect(rememberLink(TOKEN_A, "Idée", TODAY)).toBe(false);
    expect(isLinkRemembered(TOKEN_A)).toBe(false);
  });
});
