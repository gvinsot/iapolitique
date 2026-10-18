import { useSearchParams } from "react-router";

import { type IdeaFilters, ideaFiltersToSearchParams, parseIdeaFilters } from "@/lib/idea-filters";

/** Reads and writes the ranking filters in the URL query string. */
export function useIdeaFilters() {
  const [searchParams, setSearchParams] = useSearchParams();
  const filters = parseIdeaFilters(searchParams);

  const update = (patch: Partial<IdeaFilters>) => {
    setSearchParams(ideaFiltersToSearchParams({ ...filters, ...patch }), {
      replace: true,
      preventScrollReset: true,
    });
  };

  return { filters, update };
}
