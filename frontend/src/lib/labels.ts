/**
 * French labels for the enumerations fixed by the analysis contract (AGENTS.md §6.2 and §13.2).
 * Categories and impact publics come from the taxonomy served by the API, not from here; the
 * other lists mirror the fixed backend enums (backend/src/domain/types.rs).
 */

export const adminLevelLabels = {
  etat: "État",
  securite_sociale: "Sécurité sociale",
  region: "Région",
  departement: "Département",
  intercommunalite: "Intercommunalité",
  commune: "Commune",
  multi: "Plusieurs niveaux",
} as const;
export type AdminLevel = keyof typeof adminLevelLabels;

export const ideaKindLabels = {
  savings: "Baisse de dépense",
  revenue: "Hausse de recette",
  mixed: "Mixte",
} as const;
export type IdeaKind = keyof typeof ideaKindLabels;

export const ideaSortLabels = {
  score: "Rentabilité (score)",
  gain: "Gain annuel estimé",
  feasibility: "Faisabilité",
  delay: "Délai avant effet",
  recent: "Plus récentes",
} as const;
export type IdeaSort = keyof typeof ideaSortLabels;

export const confidenceLabels = {
  low: "Faible",
  medium: "Moyenne",
  high: "Élevée",
} as const;
export type Confidence = keyof typeof confidenceLabels;

/** Legal vehicle of a measure (taxonomy.yaml `legal_vehicles`, backend enum LegalVehicle). */
export const legalVehicleLabels = {
  loi: "Loi ordinaire ou organique",
  loi_de_finances: "Loi de finances ou de financement de la sécurité sociale",
  decret: "Décret",
  arrete: "Arrêté",
  deliberation: "Délibération d’une collectivité",
  circulaire: "Circulaire ou instruction",
  organisation_interne: "Décision d’organisation interne",
  autre: "Autre",
} as const;
export type LegalVehicle = keyof typeof legalVehicleLabels;

/** Expected effect of a measure on a public (taxonomy.yaml `impact_effects`). */
export const impactEffectLabels = {
  positive: "Positif",
  neutral: "Neutre",
  negative: "Négatif",
  uncertain: "Incertain",
} as const;
export type ImpactEffect = keyof typeof impactEffectLabels;

/** Kinds of contributions (taxonomy.yaml `contribution_kinds`, plus merged duplicates). */
export const contributionKindLabels = {
  amelioration: "Amélioration",
  objection: "Objection",
  doublon: "Proposition similaire",
} as const;
export type ContributionKind = keyof typeof contributionKindLabels;

/** Reasons of a report (taxonomy.yaml `report_reasons`). */
export const reportReasonLabels = {
  erreur_chiffrage: "Erreur de chiffrage",
  source_erronee: "Source erronée ou absente",
  contenu_problematique: "Contenu problématique",
  doublon: "Doublon d’une autre idée",
  autre: "Autre",
} as const;
export type ReportReason = keyof typeof reportReasonLabels;

/** Feasibility score of the analysis (1 to 5). */
export const feasibilityLabels: Record<number, string> = {
  1: "Très difficile",
  2: "Difficile",
  3: "Moyenne",
  4: "Plutôt facile",
  5: "Facile",
};

export function feasibilityLabel(score: number): string {
  return feasibilityLabels[Math.min(5, Math.max(1, Math.round(score)))] ?? "Inconnue";
}

export function isKeyOf<T extends Record<string, string>>(
  labels: T,
  value: string | null | undefined,
): value is Extract<keyof T, string> {
  return value != null && Object.hasOwn(labels, value);
}

export function entriesOf<T extends Record<string, string>>(
  labels: T,
): [Extract<keyof T, string>, string][] {
  return Object.entries(labels) as [Extract<keyof T, string>, string][];
}
