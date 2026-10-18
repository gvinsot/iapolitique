import { describe, expect, it } from "vitest";

import {
  isWellFormedTrackingToken,
  parseTrackingInput,
  tokenFromHash,
  trackingHref,
  trackingUrl,
} from "./tracking-token";

const TOKEN = `${"A".repeat(40)}-_9`;

describe("isWellFormedTrackingToken", () => {
  it("accepts 43 base64url characters", () => {
    expect(isWellFormedTrackingToken(TOKEN)).toBe(true);
  });

  it.each([
    undefined,
    null,
    "",
    "A".repeat(42),
    "A".repeat(44),
    `${"A".repeat(42)}=`,
    `${"A".repeat(42)}+`,
  ])("rejects %s", (token) => {
    expect(isWellFormedTrackingToken(token)).toBe(false);
  });
});

describe("tracking links", () => {
  it("keeps the token in the fragment", () => {
    expect(trackingHref(TOKEN)).toBe(`/suivi#${TOKEN}`);
    expect(trackingUrl(TOKEN, "https://iapolitique.fr")).toBe(
      `https://iapolitique.fr/suivi#${TOKEN}`,
    );
  });

  it("reads a token from a fragment", () => {
    expect(tokenFromHash(`#${TOKEN}`)).toBe(TOKEN);
    expect(tokenFromHash(TOKEN)).toBe(TOKEN);
    expect(tokenFromHash("#")).toBeNull();
    expect(tokenFromHash("#%E0%A4%A")).toBeNull();
    expect(tokenFromHash("#trop-court")).toBeNull();
  });

  it.each([
    [TOKEN, TOKEN],
    [`  ${TOKEN}  `, TOKEN],
    [`https://iapolitique.fr/suivi#${TOKEN}`, TOKEN],
    [`https://iapolitique.fr/suivi/${TOKEN}`, TOKEN],
    [`/suivi/${TOKEN}/`, TOKEN],
    ["https://iapolitique.fr/suivi#tronque", null],
    ["texte quelconque", null],
  ])("parses the pasted value %s", (input, expected) => {
    expect(parseTrackingInput(input)).toBe(expected);
  });
});
