import type {
  AdminLevel,
  Confidence,
  ContributionKind,
  IdeaKind,
  IdeaSort,
  ImpactDirection,
  LegalVehicle,
  ReportReason,
} from "@/api/types";

export type {
  AdminLevel,
  Confidence,
  ContributionKind,
  IdeaKind,
  IdeaSort,
  LegalVehicle,
  ReportReason,
} from "@/api/types";
export type ImpactEffect = ImpactDirection;

/**
 * French labels for the closed enumerations of the API (AGENTS.md §6.2 and §13.2), typed against
 * the generated types so a new backend value fails the build. They are the fallback of the
 * labels served by GET /api/taxonomy (hooks/use-taxonomy.ts), which take precedence: these
 * only show while the taxonomy loads or when it cannot be reached.
 */

export const adminLevelLabels = {
  etat: "État",
  securite_sociale: "Sécurité sociale",
  region: "Région",
  departement: "Département",
  intercommunalite: "Intercommunalité",
  commune: "Commune",
  multi: "Plusieurs niveaux",
} as const satisfies Record<AdminLevel, string>;

export const ideaKindLabels = {
  savings: "Baisse de dépense",
  revenue: "Hausse de recette",
  mixed: "Mixte",
} as const satisfies Record<IdeaKind, string>;

export const ideaSortLabels = {
  score: "Rentabilité (score)",
  gain: "Gain annuel estimé",
  feasibility: "Faisabilité",
  delay: "Délai avant effet",
  recent: "Plus récentes",
} as const satisfies Record<IdeaSort, string>;

export const confidenceLabels = {
  low: "Faible",
  medium: "Moyenne",
  high: "Élevée",
} as const satisfies Record<Confidence, string>;

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
} as const satisfies Record<LegalVehicle, string>;

/** Expected effect of a measure on a public (taxonomy.yaml `impact_effects`). */
export const impactEffectLabels = {
  positive: "Positif",
  neutral: "Neutre",
  negative: "Négatif",
  uncertain: "Incertain",
} as const satisfies Record<ImpactDirection, string>;

/** Kinds of contributions (taxonomy.yaml `contribution_kinds`, plus merged duplicates). */
export const contributionKindLabels = {
  amelioration: "Amélioration",
  objection: "Objection",
  doublon: "Proposition similaire",
} as const satisfies Record<ContributionKind, string>;

/** Reasons of a report (taxonomy.yaml `report_reasons`). */
export const reportReasonLabels = {
  erreur_chiffrage: "Erreur de chiffrage",
  source_erronee: "Source erronée ou absente",
  contenu_problematique: "Contenu problématique",
  doublon: "Doublon d’une autre idée",
  autre: "Autre",
} as const satisfies Record<ReportReason, string>;

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
