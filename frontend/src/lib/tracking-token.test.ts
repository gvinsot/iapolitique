import { describe, expect, it } from "vitest";

import { isWellFormedTrackingToken } from "./tracking-token";

describe("isWellFormedTrackingToken", () => {
  it("accepts 43 base64url characters", () => {
    expect(isWellFormedTrackingToken(`${"A".repeat(40)}-_9`)).toBe(true);
  });

  it.each([
    undefined,
    "",
    "A".repeat(42),
    "A".repeat(44),
    `${"A".repeat(42)}=`,
    `${"A".repeat(42)}+`,
  ])("rejects %s", (token) => {
    expect(isWellFormedTrackingToken(token)).toBe(false);
  });
});
