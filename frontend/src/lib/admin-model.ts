import type { ReportReason } from "./labels";
import type { ReviewOutcome, TrackedKind } from "./tracking-model";

/**
 * Display models of the administration (AGENTS.md §8), in the API's snake_case. Amounts of AI
 * costs are estimates in euros with decimals (prices per token), unlike idea amounts.
 */

export interface DashboardStats {
  jobs: { queued: number; running: number; failed: number };
  /** Screening outcomes over the last `window_days` days. */
  screening: { window_days: number; accepted: number; rejected: number; gray_zone: number };
  ai_costs: { today_eur: number; month_eur: number; daily_budget_eur: number };
  escalations_pending: number;
  review_requests_pending: number;
}

export type HiddenOrigin = "report_review" | "escalation" | "admin";

export interface HiddenIdeaRow {
  slug: string;
  title: string;
  hidden_on: string;
  origin: HiddenOrigin;
  reason: string;
}

export interface ReportRow {
  id: string;
  idea_slug: string;
  idea_title: string;
  reason: ReportReason;
  details: string | null;
  received_on: string;
  /** Raw report status (backend ReportStatus). */
  status: string;
  outcome: ReviewOutcome | null;
  escalate: boolean;
}

export type EscalationStatus = "pending" | "transmitted" | "dismissed";

export interface EscalationRow {
  id: string;
  idea_slug: string;
  idea_title: string;
  /** ISO date-time: the 24-hour target runs from here. */
  escalated_at: string;
  explanation: string;
  status: EscalationStatus;
}

export interface ReviewRequestRow {
  id: string;
  kind: TrackedKind;
  /** Masked original text, null once purged. */
  text: string | null;
  reason_code: string;
  message: string | null;
  requested_on: string;
  status: "pending" | "done";
}

export type CriterionVerdict = "pass" | "fail" | "gray";
export type DecisionLabel = "correct" | "incorrect";

export interface CriterionScore {
  key: string;
  /** Probability of passing the criterion, 0 to 1 (AGENTS.md §13.4). */
  probability: number;
  reject_below: number;
  accept_above: number;
  verdict: CriterionVerdict;
}

export interface ScreeningDecisionRow {
  id: string;
  target: TrackedKind;
  decided_at: string;
  outcome: "accepted" | "rejected" | "gray_zone";
  /** Text excerpt, null once purged (only scores remain for calibration). */
  excerpt: string | null;
  criteria: CriterionScore[];
  grid_version: string;
  label: DecisionLabel | null;
}

export interface CriterionThreshold {
  key: string;
  reject_below: number;
  accept_above: number;
}

export interface ThresholdSettings {
  idea: CriterionThreshold[];
  contribution: CriterionThreshold[];
  report: CriterionThreshold[];
  dedup_similarity_threshold: number;
}

/** Right of reply as edited by the administrator (LCEN art. 1-1 III). */
export interface RightOfReplyDraft {
  author_label: string;
  body: string;
  /** Day the request was received, YYYY-MM-DD: publication is due within 3 days. */
  requested_on: string;
}

/** Share of accepted decisions among clear-cut ones (gray zone excluded), or null. */
export function acceptanceRate(screening: DashboardStats["screening"]): number | null {
  const decided = screening.accepted + screening.rejected;
  return decided === 0 ? null : screening.accepted / decided;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Hours left before the 24-hour escalation target (negative once overdue). */
export function escalationHoursLeft(escalatedAt: string, now: Date = new Date()): number {
  return (Date.parse(escalatedAt) + DAY_MS - now.getTime()) / (60 * 60 * 1000);
}

/** Last day to publish a right of reply: 3 days after the request (LCEN art. 1-1 III). */
export function rightOfReplyDeadline(requestedOn: string): string {
  const date = new Date(`${requestedOn.slice(0, 10)}T12:00:00Z`);
  date.setUTCDate(date.getUTCDate() + 3);
  return date.toISOString().slice(0, 10);
}

/** French error for one criterion's thresholds, or null when valid. */
export function validateThreshold(threshold: CriterionThreshold): string | null {
  const { reject_below: reject, accept_above: accept } = threshold;
  if (![reject, accept].every((value) => Number.isFinite(value) && value >= 0 && value <= 1)) {
    return "Les seuils sont des probabilités comprises entre 0 et 1.";
  }
  if (reject > accept) {
    return "Le seuil de rejet doit être inférieur ou égal au seuil d’acceptation.";
  }
  return null;
}

export function validateDedupThreshold(value: number): string | null {
  return Number.isFinite(value) && value >= 0.5 && value <= 1
    ? null
    : "Le seuil de similarité est compris entre 0,5 et 1.";
}
