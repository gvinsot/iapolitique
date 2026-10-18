import {
  type AdminLevel,
  adminLevelLabels,
  type IdeaKind,
  ideaKindLabels,
  type IdeaSort,
  ideaSortLabels,
  isKeyOf,
} from "./labels";

/**
 * Ranking filters, kept in the URL so a filtered ranking can be shared and survives a reload.
 * Parameter names match the public API query (AGENTS.md §13.2).
 */
export interface IdeaFilters {
  sort: IdeaSort;
  adminLevel: AdminLevel | null;
  kind: IdeaKind | null;
  q: string;
}

export const DEFAULT_IDEA_SORT: IdeaSort = "score";
export const IDEA_SEARCH_MAX_LENGTH = 200;

export const defaultIdeaFilters: IdeaFilters = {
  sort: DEFAULT_IDEA_SORT,
  adminLevel: null,
  kind: null,
  q: "",
};

export function parseIdeaFilters(params: URLSearchParams): IdeaFilters {
  const sort = params.get("sort");
  const adminLevel = params.get("admin_level");
  const kind = params.get("kind");
  return {
    sort: isKeyOf(ideaSortLabels, sort) ? sort : DEFAULT_IDEA_SORT,
    adminLevel: isKeyOf(adminLevelLabels, adminLevel) ? adminLevel : null,
    kind: isKeyOf(ideaKindLabels, kind) ? kind : null,
    q: (params.get("q") ?? "").trim().slice(0, IDEA_SEARCH_MAX_LENGTH),
  };
}

/** Serialises filters, omitting defaults so URLs stay short and canonical. */
export function ideaFiltersToSearchParams(filters: IdeaFilters): URLSearchParams {
  const params = new URLSearchParams();
  if (filters.sort !== DEFAULT_IDEA_SORT) params.set("sort", filters.sort);
  if (filters.adminLevel) params.set("admin_level", filters.adminLevel);
  if (filters.kind) params.set("kind", filters.kind);
  const q = filters.q.trim();
  if (q) params.set("q", q.slice(0, IDEA_SEARCH_MAX_LENGTH));
  return params;
}

export function hasActiveIdeaFilters(filters: IdeaFilters): boolean {
  return ideaFiltersToSearchParams(filters).size > 0;
}
