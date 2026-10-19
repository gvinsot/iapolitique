/**
 * Sealed copy of escalated content (owner decision D3, AGENTS.md §7.5): what the admin API
 * returns in `AdminEscalationSeal.content`. The spec types it as a free JSON object, so it is
 * read defensively here: every field is optional text, anything unexpected becomes null. The
 * copy is shown as plain text only, never as HTML.
 */

export interface SealedIdea {
  status: string | null;
  slug: string | null;
  title: string | null;
  summary: string | null;
  originalText: string | null;
  submittedTerritory: string | null;
  submittedAdminLevel: string | null;
  submittedAt: string | null;
  publishedAt: string | null;
  hiddenAt: string | null;
  hiddenReason: string | null;
}

export interface SealedReview {
  outcome: string | null;
  escalate: boolean | null;
  explanation: string | null;
  model: string | null;
  promptVersion: string | null;
  reviewedAt: string | null;
}

export interface SealedReport {
  reason: string | null;
  details: string | null;
  reportedAt: string | null;
}

export interface SealedContent {
  idea: SealedIdea | null;
  /** Content of the analysis in force at the escalation, as stored (null when none). */
  analysis: unknown;
  review: SealedReview | null;
  reports: SealedReport[];
  /** Sealed afterwards by migration 0016 from what was left of the escalation. */
  backfilled: boolean;
}

type JsonObject = Record<string, unknown>;

function asObject(value: unknown): JsonObject | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as JsonObject)
    : null;
}

function text(object: JsonObject, key: string): string | null {
  const value = object[key];
  if (typeof value === "string") return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return null;
}

function readIdea(value: unknown): SealedIdea | null {
  const idea = asObject(value);
  if (!idea) return null;
  return {
    status: text(idea, "status"),
    slug: text(idea, "slug"),
    title: text(idea, "title"),
    summary: text(idea, "summary"),
    originalText: text(idea, "original_text"),
    submittedTerritory: text(idea, "submitted_territory"),
    submittedAdminLevel: text(idea, "submitted_admin_level"),
    submittedAt: text(idea, "submitted_at"),
    publishedAt: text(idea, "published_at"),
    hiddenAt: text(idea, "hidden_at"),
    hiddenReason: text(idea, "hidden_reason"),
  };
}

function readReview(value: unknown): SealedReview | null {
  const review = asObject(value);
  if (!review) return null;
  return {
    outcome: text(review, "outcome"),
    escalate: typeof review.escalate === "boolean" ? review.escalate : null,
    explanation: text(review, "explanation"),
    model: text(review, "model"),
    promptVersion: text(review, "prompt_version"),
    reviewedAt: text(review, "reviewed_at"),
  };
}

function readReports(value: unknown): SealedReport[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry) => {
    const report = asObject(entry);
    return report
      ? [
          {
            reason: text(report, "reason"),
            details: text(report, "details"),
            reportedAt: text(report, "reported_at"),
          },
        ]
      : [];
  });
}

export function readSealContent(content: unknown): SealedContent {
  const root = asObject(content) ?? {};
  const analysis = root.analysis;
  return {
    idea: readIdea(root.idea),
    analysis: analysis === undefined ? null : analysis,
    review: readReview(root.review),
    reports: readReports(root.reports),
    backfilled: root.backfilled === true,
  };
}

/** Plain-text rendering of the sealed analysis (indented JSON, shown in a `<pre>`). */
export function analysisText(analysis: unknown): string | null {
  if (analysis === null || analysis === undefined) return null;
  try {
    return JSON.stringify(analysis, null, 2);
  } catch {
    return null;
  }
}
