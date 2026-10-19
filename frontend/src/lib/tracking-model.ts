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
  // The author's own link answers `deleted` once the idea is withdrawn (§7.1); defensive.
  withdrawn: "deleted",
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
  return item.status !== "deleted" && item.status !== "purged" && item.status !== "withdrawn";
}

/**
 * Deleting a published (or hidden) idea withdraws it instead of erasing it (owner decision D2,
 * AGENTS.md §5.2): it leaves the site at once, stays visible to the publisher only and is purged
 * 12 months later. Any other text is erased at once.
 */
export function deletionWithdraws(item: Pick<TrackedItem, "kind" | "status">): boolean {
  return item.kind === "idea" && (item.status === "published" || item.status === "hidden");
}

/**
 * The idea of this contribution, report or merged duplicate was withdrawn by its author: the
 * API then sends `idea: null` (always sent, optional in the spec for older clients).
 */
export function ideaWithdrawn(item: Pick<TrackedItem, "idea_withdrawn">): boolean {
  return item.idea_withdrawn === true;
}

/**
 * A human review may be requested once, for an automated decision against the author (a
 * masking decided by the administrator is not one: the API answers `review_not_available`), and
 * never once the idea concerned was withdrawn by its author (same answer).
 */
export function canRequestReview(item: TrackedItem): boolean {
  const phase = trackingPhase(item);
  return (
    (phase === "rejected" || phase === "hidden") &&
    !ideaWithdrawn(item) &&
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
