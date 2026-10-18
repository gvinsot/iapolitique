import { describe, expect, it } from "vitest";

import { canDelete, canRequestReview, type TrackedItem, trackingPhase } from "./tracking-model";

const base: TrackedItem = {
  kind: "idea",
  status: "rejected",
  submitted_on: "2026-10-18",
  idea: null,
  decision: {
    measure: "rejected",
    reason_code: "off_topic",
    reason_message: "Votre texte ne propose pas de mesure…",
    rule: { article: "4.5", title: "Une mesure pour les finances publiques", url: "/regles" },
    automated: true,
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

  it("refuses a human review of a masking decided by the administrator", () => {
    const decision = base.decision ?? null;
    expect(
      canRequestReview({
        ...base,
        status: "hidden",
        decision: decision && { ...decision, measure: "hidden", automated: false },
      }),
    ).toBe(false);
  });
});
