import { describe, expect, it } from "vitest";

import {
  canDelete,
  canRequestReview,
  parseTrackedItem,
  type TrackedItem,
  trackingPhase,
} from "./tracking-model";

const base: TrackedItem = {
  kind: "idea",
  status: "rejected",
  submitted_on: "2026-10-18",
  idea: null,
  decision: {
    reason_code: "off_topic",
    reason_message: "Votre texte ne propose pas de mesure…",
    decided_on: "2026-10-18",
    purge_on: "2026-11-17",
    second_opinion: false,
  },
  report_outcome: null,
  review_request: null,
};

describe("trackingPhase", () => {
  it.each([
    ["idea", "submitted", "pending"],
    ["idea", "screening", "pending"],
    ["idea", "second_opinion", "second_opinion"],
    ["idea", "queued_analysis", "accepted"],
    ["idea", "analysis_failed", "accepted"],
    ["idea", "published", "published"],
    ["idea", "merged", "merged"],
    ["idea", "hidden", "hidden"],
    ["idea", "purged", "rejected"],
    ["contribution", "integrated", "integrated"],
    ["contribution", "accepted", "accepted"],
    ["report", "accepted", "received"],
    ["report", "reviewed", "reviewed"],
    ["report", "deleted", "deleted"],
    ["idea", "integrated", "unknown"],
  ] as const)("%s %s → %s", (kind, status, phase) => {
    expect(trackingPhase({ kind, status })).toBe(phase);
  });
});

describe("available actions", () => {
  it("allows deletion while the text exists", () => {
    expect(canDelete(base)).toBe(true);
    expect(canDelete({ ...base, status: "published" })).toBe(true);
    expect(canDelete({ ...base, status: "purged" })).toBe(false);
    expect(canDelete({ ...base, status: "deleted" })).toBe(false);
  });

  it("allows one human review of a refusal or a masking", () => {
    expect(canRequestReview(base)).toBe(true);
    expect(canRequestReview({ ...base, status: "hidden" })).toBe(true);
    expect(canRequestReview({ ...base, status: "published" })).toBe(false);
    expect(canRequestReview({ ...base, status: "purged" })).toBe(false);
    expect(
      canRequestReview({
        ...base,
        review_request: { requested_on: "2026-10-19", status: "pending" },
      }),
    ).toBe(false);
  });
});

describe("parseTrackedItem", () => {
  it("accepts a complete answer", () => {
    expect(parseTrackedItem(base)).toEqual(base);
  });

  it("drops unusable optional parts and rejects unusable answers", () => {
    expect(
      parseTrackedItem({
        ...base,
        idea: { slug: "" },
        decision: { reason_code: "x" },
        report_outcome: "delete",
        review_request: { status: "done" },
      }),
    ).toEqual({ ...base, decision: null, review_request: null });
    expect(parseTrackedItem({ ...base, kind: "vote" })).toBeNull();
    expect(parseTrackedItem({ ...base, status: "" })).toBeNull();
    expect(parseTrackedItem(null)).toBeNull();
  });
});
