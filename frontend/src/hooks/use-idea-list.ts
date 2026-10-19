import { keepPreviousData } from "@tanstack/react-query";

import { $api } from "@/api/client";
import type { paths } from "@/api/schema";
import type { IdeaFilters } from "@/lib/idea-filters";

type IdeaListQuery = NonNullable<paths["/api/ideas"]["get"]["parameters"]["query"]>;

/** Ideas per page of the ranking (the API accepts 1 to 50). */
export const IDEAS_PER_PAGE = 20;

/** Query string of GET /api/ideas for the filters of the URL; defaults are left out. */
export function ideaListQuery(filters: IdeaFilters): IdeaListQuery {
  const query: IdeaListQuery = { per_page: IDEAS_PER_PAGE };
  if (filters.sort !== "score") query.sort = filters.sort;
  if (filters.adminLevel) query.admin_level = filters.adminLevel;
  if (filters.category) query.category = filters.category;
  if (filters.kind) query.kind = filters.kind;
  if (filters.territory) query.territory = filters.territory;
  if (filters.q) query.q = filters.q;
  if (filters.page > 1) query.page = filters.page;
  return query;
}

/** One page of the ranking; the previous page stays displayed while the next one loads. */
export function useIdeaList(filters: IdeaFilters) {
  return $api.useQuery(
    "get",
    "/api/ideas",
    { params: { query: ideaListQuery(filters) } },
    { placeholderData: keepPreviousData },
  );
}
