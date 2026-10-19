import { useMemo } from "react";

import { $api } from "@/api/client";
import { type TaxonomyLabels, taxonomyLabels } from "@/lib/taxonomy-labels";

/** The taxonomy in force (GET /api/taxonomy); it changes only with a deployment. */
export function useTaxonomy() {
  return $api.useQuery("get", "/api/taxonomy", {}, { staleTime: 60 * 60_000 });
}

/**
 * Labels of the taxonomy served by the API, falling back to the typed labels of lib/labels.ts
 * while it loads or when it cannot be reached: components always have something to show.
 */
export function useTaxonomyLabels(): TaxonomyLabels {
  const { data } = useTaxonomy();
  return useMemo(() => taxonomyLabels(data), [data]);
}
