import type { CategoryItem, Taxonomy, TaxonomyItem } from "@/api/types";

import {
  adminLevelLabels,
  confidenceLabels,
  contributionKindLabels,
  ideaKindLabels,
  impactEffectLabels,
  legalVehicleLabels,
  reportReasonLabels,
} from "./labels";

/**
 * French labels of the enumerations, as published by GET /api/taxonomy (prompts/taxonomy.yaml),
 * over the typed fallback of lib/labels.ts. A key unknown to both is shown as is rather than
 * hidden, so a new backend value stays visible until the site is updated.
 */

export type LabelMap = Readonly<Record<string, string>>;

export interface TaxonomyLabels {
  adminLevels: LabelMap;
  ideaKinds: LabelMap;
  confidenceLevels: LabelMap;
  legalVehicles: LabelMap;
  impactEffects: LabelMap;
  reportReasons: LabelMap;
  contributionKinds: LabelMap;
  /** Categories of the taxonomy, in file order (empty until it is loaded). */
  categories: readonly CategoryItem[];
  /** Whether the labels come from the API (false: fallback only). */
  fromApi: boolean;
}

function merge(fallback: LabelMap, items: readonly TaxonomyItem[] | undefined): LabelMap {
  if (!items) return fallback;
  const merged: Record<string, string> = { ...fallback };
  for (const item of items) {
    if (item.label.trim() !== "") merged[item.key] = item.label;
  }
  return merged;
}

export function taxonomyLabels(taxonomy?: Taxonomy): TaxonomyLabels {
  return {
    adminLevels: merge(adminLevelLabels, taxonomy?.admin_levels),
    ideaKinds: merge(ideaKindLabels, taxonomy?.idea_kinds),
    confidenceLevels: merge(confidenceLabels, taxonomy?.confidence_levels),
    legalVehicles: merge(legalVehicleLabels, taxonomy?.legal_vehicles),
    impactEffects: merge(impactEffectLabels, taxonomy?.impact_effects),
    reportReasons: merge(reportReasonLabels, taxonomy?.report_reasons),
    contributionKinds: merge(contributionKindLabels, taxonomy?.contribution_kinds),
    categories: taxonomy?.categories ?? [],
    fromApi: taxonomy !== undefined,
  };
}

/** The label of `key`, or the key itself when no label is known. */
export function labelOf(labels: LabelMap, key: string): string {
  return Object.hasOwn(labels, key) ? (labels[key] ?? key) : key;
}

/** Fallback labels, used until GET /api/taxonomy answers (and in its absence). */
export const fallbackTaxonomyLabels: TaxonomyLabels = taxonomyLabels();

/** Options of a closed enumeration, in the order of its fallback map, with the labels in force. */
export function labelledOptions<K extends string>(
  fallback: Readonly<Record<K, string>>,
  labels: LabelMap,
): [K, string][] {
  return (Object.keys(fallback) as K[]).map((key) => [key, labelOf(labels, key)]);
}
