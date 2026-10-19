import type {
  AdminCriterionThresholds,
  AdminScreening,
  AdminThresholdSection,
  AdminThresholdsUpdate,
  Decision,
  EscalationOutcome,
  IdeaStatus,
  JobKind,
  JobStatus,
  LabelVerdict,
  ReportStatus,
  ReviewResolution,
  ScreeningOutcome,
  TargetKind,
} from "@/api/types";

/**
 * Helpers and French labels of the administration (AGENTS.md §8, §13.2). The data itself uses
 * the generated API types; AI costs are estimates in euros with decimals, unlike idea amounts.
 */

export const targetKindLabels: Record<TargetKind, string> = {
  idea: "Idée",
  contribution: "Contribution",
  report: "Signalement",
};

export const targetSectionLabels: Record<TargetKind, string> = {
  idea: "Idées",
  contribution: "Améliorations et objections",
  report: "Signalements",
};

export const ideaStatusLabels: Record<IdeaStatus, string> = {
  submitted: "En attente de filtrage",
  screening: "En filtrage",
  second_opinion: "Second avis en cours",
  rejected: "Refusée",
  purged: "Purgée",
  accepted: "Acceptée",
  dedup: "Recherche de doublons",
  merged: "Fusionnée",
  queued_analysis: "Analyse en file",
  analyzing: "Analyse en cours",
  analysis_failed: "Analyse en échec",
  published: "Publiée",
  hidden: "Masquée",
  withdrawn: "Retirée par son auteur",
};

export const reportStatusLabels: Record<ReportStatus, string> = {
  submitted: "En attente de filtrage",
  screening: "En filtrage",
  second_opinion: "Second avis en cours",
  accepted: "En attente de revue",
  rejected: "Refusé",
  reviewed: "Examiné",
  purged: "Purgé",
};

export const jobStatusLabels: Record<JobStatus, string> = {
  queued: "En file",
  running: "En cours",
  succeeded: "Terminé",
  dead: "Abandonné",
};

export const jobKindLabels: Record<JobKind, string> = {
  screen_idea: "Filtrage d’une idée",
  second_opinion: "Second avis",
  dedup_idea: "Recherche de doublons",
  analyze_idea: "Analyse",
  screen_contribution: "Filtrage d’une contribution",
  screen_report: "Filtrage d’un signalement",
  review_reports: "Revue des signalements",
  generate_exports: "Génération des exports",
  purge: "Purge",
};

export const screeningOutcomeLabels: Record<ScreeningOutcome, string> = {
  accepted: "Acceptée",
  rejected: "Refusée",
  gray_zone: "Zone grise",
};

export const decisionLabels: Record<Decision, string> = {
  accept: "Accepter",
  reject: "Refuser",
};

export const labelVerdictLabels: Record<LabelVerdict, string> = {
  correct: "Décision juste",
  incorrect: "Décision erronée",
};

export const escalationOutcomeLabels: Record<EscalationOutcome, string> = {
  transmitted: "Transmis à PHAROS",
  dismissed: "Classé sans suite",
};

export const reviewResolutionLabels: Record<ReviewResolution, string> = {
  confirmed: "Décision confirmée",
  overturned: "Décision infirmée",
};

export const hiddenReasonLabels: Record<string, string> = {
  report_review: "Revue de signalement (IA)",
  escalation: "Escalade (contenu grave)",
  admin: "Action de l’administration",
};

export const reviewOutcomeLabels: Record<string, string> = {
  maintain: "Maintenue",
  update: "Mise à jour",
  hide: "Masquée",
};

export const aiPurposeLabels: Record<string, string> = {
  screening: "Filtrage (modèle de décision)",
  second_opinion: "Second avis",
  analysis_research: "Analyse : recherche web",
  analysis_structuring: "Analyse : structuration",
  embedding: "Embeddings (doublons)",
  report_review: "Revue des signalements",
};

/** `reasoning_effort` sent to Mistral (AGENTS.md §6.1); no value: omitted or not applicable. */
export const reasoningEffortLabels: Record<string, string> = {
  high: "Élevé (high)",
  none: "Minimal (none)",
};

/** A label from a map, or the raw key when the backend sent a value unknown here. */
export function labelFor(labels: Record<string, string>, key: string | null | undefined): string {
  if (!key) return "—";
  return Object.hasOwn(labels, key) ? (labels[key] ?? key) : key;
}

const HOUR_MS = 60 * 60 * 1000;

/** Hours left before the due time of an escalation (negative once overdue). */
export function hoursUntil(dueAt: string, now: Date = new Date()): number {
  return (Date.parse(dueAt) - now.getTime()) / HOUR_MS;
}

/** Last day to publish a right of reply: 3 days after the request (LCEN art. 1-1 III). */
export function rightOfReplyDeadline(requestedOn: string): string {
  const date = new Date(`${requestedOn.slice(0, 10)}T12:00:00Z`);
  date.setUTCDate(date.getUTCDate() + 3);
  return date.toISOString().slice(0, 10);
}

/** Today in Paris, `YYYY-MM-DD` (the API compares request days in Europe/Paris). */
export function parisToday(now: Date = new Date()): string {
  return new Intl.DateTimeFormat("en-CA", { timeZone: "Europe/Paris" }).format(now);
}

/**
 * The decision a labelled screening should have reached: `correct` keeps the decision reached,
 * `incorrect` inverts it. Null when no decision was reached (service error, pending second
 * opinion): the administrator must then say which decision was expected.
 */
export function expectedFromLabel(
  screening: Pick<AdminScreening, "final_decision">,
  label: LabelVerdict,
): Decision | null {
  if (!screening.final_decision) return null;
  if (label === "correct") return screening.final_decision;
  return screening.final_decision === "accept" ? "reject" : "accept";
}

/** French error for one criterion's thresholds, or null when valid. */
export function validateThresholds(rejectBelow: number, acceptAbove: number): string | null {
  if (
    ![rejectBelow, acceptAbove].every((value) => Number.isFinite(value) && value >= 0 && value <= 1)
  ) {
    return "Les seuils sont des probabilités comprises entre 0 et 1.";
  }
  if (rejectBelow > acceptAbove) {
    return "Le seuil de rejet doit être inférieur ou égal au seuil d’acceptation.";
  }
  return null;
}

export function validateDedupThreshold(value: number, minimum: number): string | null {
  return Number.isFinite(value) && value >= minimum && value <= 1
    ? null
    : `Le seuil de similarité est compris entre ${String(minimum).replace(".", ",")} et 1.`;
}

export interface ThresholdValues {
  reject_below: number;
  accept_above: number;
}

/**
 * Body of PUT /api/admin/settings/screening-thresholds (AGENTS.md §13.6): only the thresholds
 * that differ from screening.yaml are sent, so a value equal to the file's is not an override.
 */
export function thresholdOverrides(
  sections: readonly AdminThresholdSection[],
  values: (kind: TargetKind, criterion: AdminCriterionThresholds) => ThresholdValues,
): AdminThresholdsUpdate {
  const update: AdminThresholdsUpdate = {};
  for (const section of sections) {
    for (const criterion of section.criteria) {
      const next = values(section.kind, criterion);
      const override: { reject_below?: number; accept_above?: number } = {};
      if (next.reject_below !== criterion.default_reject_below) {
        override.reject_below = next.reject_below;
      }
      if (next.accept_above !== criterion.default_accept_above) {
        override.accept_above = next.accept_above;
      }
      if (Object.keys(override).length > 0) {
        update[section.kind] = { ...update[section.kind], [criterion.key]: override };
      }
    }
  }
  return update;
}
