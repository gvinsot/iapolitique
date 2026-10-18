/**
 * French labels for the enumerations fixed by the analysis contract (AGENTS.md §6.2 and §13.2).
 * Categories and impact publics come from the taxonomy served by the API, not from here.
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
