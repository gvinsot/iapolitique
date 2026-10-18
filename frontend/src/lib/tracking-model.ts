import type {
  ReviewOutcome,
  StatementOfReasons,
  TargetKind,
  TrackedIdea,
  TrackedItem,
} from "@/api/types";

/**
 * What the tracking page shows (POST /api/track, AGENTS.md §7.1 and §13.2). Statuses are the
 * raw backend statuses (backend/src/domain/status.rs) plus `deleted` (erased by the author);
 * `trackingPhase` turns them into the phase displayed to the author.
 */

export type TrackedKind = TargetKind;
export type { ReviewOutcome, TrackedItem };
/** Published idea, parent idea (merged duplicate) or idea concerned (contribution, report). */
export type TrackedIdeaRef = TrackedIdea;
/** Automated decision against the author (refusal or masking): DSA art. 17 statement. */
export type TrackedDecision = StatementOfReasons;

export type TrackingPhase =
  | "pending"
  | "second_opinion"
  | "accepted"
  | "published"
  | "merged"
  | "integrated"
  | "received"
  | "reviewed"
  | "rejected"
  | "hidden"
  | "deleted"
  | "unknown";

const ideaPhases: Record<string, TrackingPhase> = {
  submitted: "pending",
  screening: "pending",
  second_opinion: "second_opinion",
  accepted: "accepted",
  dedup: "accepted",
  queued_analysis: "accepted",
  analyzing: "accepted",
  analysis_failed: "accepted",
  published: "published",
  merged: "merged",
  hidden: "hidden",
  rejected: "rejected",
  purged: "rejected",
  deleted: "deleted",
};

const contributionPhases: Record<string, TrackingPhase> = {
  submitted: "pending",
  screening: "pending",
  second_opinion: "second_opinion",
  accepted: "accepted",
  integrated: "integrated",
  rejected: "rejected",
  purged: "rejected",
  deleted: "deleted",
};

const reportPhases: Record<string, TrackingPhase> = {
  submitted: "pending",
  screening: "pending",
  second_opinion: "second_opinion",
  accepted: "received",
  reviewed: "reviewed",
  rejected: "rejected",
  purged: "rejected",
  deleted: "deleted",
};

const phasesByKind: Record<TrackedKind, Record<string, TrackingPhase>> = {
  idea: ideaPhases,
  contribution: contributionPhases,
  report: reportPhases,
};

export function trackingPhase(item: Pick<TrackedItem, "kind" | "status">): TrackingPhase {
  return phasesByKind[item.kind][item.status] ?? "unknown";
}

/** The author may erase the text while it still exists. */
export function canDelete(item: TrackedItem): boolean {
  return item.status !== "deleted" && item.status !== "purged";
}

/**
 * A human review may be requested once, for an automated decision against the author (a
 * masking decided by the administrator is not one: the API answers `review_not_available`).
 */
export function canRequestReview(item: TrackedItem): boolean {
  const phase = trackingPhase(item);
  return (
    (phase === "rejected" || phase === "hidden") &&
    item.status !== "purged" &&
    item.review_request === null &&
    item.decision?.automated !== false
  );
}

export const trackedKindLabels: Record<TrackedKind, string> = {
  idea: "Idée",
  contribution: "Contribution",
  report: "Signalement",
};
