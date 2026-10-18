/**
 * What the tracking page shows (POST /api/track, AGENTS.md §7.1 and §13.2). Statuses are the
 * raw backend statuses (backend/src/domain/status.rs) plus `deleted` (erased by the author);
 * `describeTracking` turns them into the phase displayed to the author.
 *
 * Field names follow the API conventions (snake_case) so the generated types map onto them.
 */

export type TrackedKind = "idea" | "contribution" | "report";
export type ReviewOutcome = "maintain" | "update" | "hide";

export interface TrackedIdeaRef {
  slug: string;
  title: string;
}

/** Automated decision against the author (refusal or masking): DSA art. 17 statement. */
export interface TrackedDecision {
  /** reason_code of prompts/screening.yaml (refusal) or report review (masking). */
  reason_code: string;
  /** French explanation shown to the author. */
  reason_message: string;
  /** Day of the decision, YYYY-MM-DD. */
  decided_on: string;
  /** Day the text will be erased, YYYY-MM-DD, when a purge is scheduled. */
  purge_on: string | null;
  /** The decision followed a second opinion by a Mistral model (gray zone). */
  second_opinion: boolean;
}

export interface TrackedReviewRequest {
  requested_on: string;
  /** `pending` until an administrator handled it. */
  status: "pending" | "done";
}

export interface TrackedItem {
  kind: TrackedKind;
  status: string;
  submitted_on: string;
  /** Published idea, parent idea (merged duplicate) or idea concerned (contribution, report). */
  idea: TrackedIdeaRef | null;
  decision: TrackedDecision | null;
  /** Outcome of the AI review of a report. */
  report_outcome: ReviewOutcome | null;
  review_request: TrackedReviewRequest | null;
}

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

/** A human review may be requested once, for an automated decision against the author. */
export function canRequestReview(item: TrackedItem): boolean {
  const phase = trackingPhase(item);
  return (
    (phase === "rejected" || phase === "hidden") &&
    item.status !== "purged" &&
    item.review_request === null
  );
}

export const trackedKindLabels: Record<TrackedKind, string> = {
  idea: "Idée",
  contribution: "Contribution",
  report: "Signalement",
};

// --- Runtime validation of the API answer ---------------------------------------------------

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function str(value: unknown): string | null {
  return typeof value === "string" && value !== "" ? value : null;
}

function parseIdeaRef(value: unknown): TrackedIdeaRef | null {
  if (!isRecord(value)) return null;
  const slug = str(value.slug);
  const title = str(value.title);
  return slug && title ? { slug, title } : null;
}

function parseDecision(value: unknown): TrackedDecision | null {
  if (!isRecord(value)) return null;
  const reasonCode = str(value.reason_code);
  const reasonMessage = str(value.reason_message);
  const decidedOn = str(value.decided_on);
  if (!reasonCode || !reasonMessage || !decidedOn) return null;
  return {
    reason_code: reasonCode,
    reason_message: reasonMessage,
    decided_on: decidedOn,
    purge_on: str(value.purge_on),
    second_opinion: value.second_opinion === true,
  };
}

function parseReviewRequest(value: unknown): TrackedReviewRequest | null {
  if (!isRecord(value)) return null;
  const requestedOn = str(value.requested_on);
  if (!requestedOn) return null;
  return { requested_on: requestedOn, status: value.status === "done" ? "done" : "pending" };
}

const kinds: readonly TrackedKind[] = ["idea", "contribution", "report"];
const outcomes: readonly ReviewOutcome[] = ["maintain", "update", "hide"];

/** Validates the answer of POST /api/track; null when it is not usable. */
export function parseTrackedItem(value: unknown): TrackedItem | null {
  if (!isRecord(value)) return null;
  const kind = kinds.find((candidate) => candidate === value.kind);
  const status = str(value.status);
  const submittedOn = str(value.submitted_on);
  if (!kind || !status || !submittedOn) return null;
  return {
    kind,
    status,
    submitted_on: submittedOn,
    idea: parseIdeaRef(value.idea),
    decision: parseDecision(value.decision),
    report_outcome: outcomes.find((outcome) => outcome === value.report_outcome) ?? null,
    review_request: parseReviewRequest(value.review_request),
  };
}
