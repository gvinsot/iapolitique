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
  /** Category key of the taxonomy (validated by the API, which knows the taxonomy). */
  category: string | null;
  kind: IdeaKind | null;
  /** Territory name or INSEE code, free text. */
  territory: string;
  q: string;
  /** 1-based page of the ranking. */
  page: number;
}

export const DEFAULT_IDEA_SORT: IdeaSort = "score";
export const IDEA_SEARCH_MAX_LENGTH = 200;
export const IDEA_TERRITORY_MAX_LENGTH = 120;
/** Upper bound accepted from the URL, far above any real page count. */
const MAX_PAGE = 10_000;
/** Taxonomy keys are snake_case identifiers (prompts/taxonomy.yaml). */
const CATEGORY_KEY_PATTERN = /^[a-z][a-z0-9_]{0,63}$/;

export const defaultIdeaFilters: IdeaFilters = {
  sort: DEFAULT_IDEA_SORT,
  adminLevel: null,
  category: null,
  kind: null,
  territory: "",
  q: "",
  page: 1,
};

function parsePage(value: string | null): number {
  if (!value || !/^\d{1,5}$/.test(value)) return 1;
  const page = Number(value);
  return page >= 1 && page <= MAX_PAGE ? page : 1;
}

export function parseIdeaFilters(params: URLSearchParams): IdeaFilters {
  const sort = params.get("sort");
  const adminLevel = params.get("admin_level");
  const category = params.get("category");
  const kind = params.get("kind");
  return {
    sort: isKeyOf(ideaSortLabels, sort) ? sort : DEFAULT_IDEA_SORT,
    adminLevel: isKeyOf(adminLevelLabels, adminLevel) ? adminLevel : null,
    category: category && CATEGORY_KEY_PATTERN.test(category) ? category : null,
    kind: isKeyOf(ideaKindLabels, kind) ? kind : null,
    territory: (params.get("territory") ?? "").trim().slice(0, IDEA_TERRITORY_MAX_LENGTH),
    q: (params.get("q") ?? "").trim().slice(0, IDEA_SEARCH_MAX_LENGTH),
    page: parsePage(params.get("page")),
  };
}

/** Serialises filters, omitting defaults so URLs stay short and canonical. */
export function ideaFiltersToSearchParams(filters: IdeaFilters): URLSearchParams {
  const params = new URLSearchParams();
  if (filters.sort !== DEFAULT_IDEA_SORT) params.set("sort", filters.sort);
  if (filters.adminLevel) params.set("admin_level", filters.adminLevel);
  if (filters.category) params.set("category", filters.category);
  if (filters.kind) params.set("kind", filters.kind);
  const territory = filters.territory.trim();
  if (territory) params.set("territory", territory.slice(0, IDEA_TERRITORY_MAX_LENGTH));
  const q = filters.q.trim();
  if (q) params.set("q", q.slice(0, IDEA_SEARCH_MAX_LENGTH));
  if (filters.page > 1) params.set("page", String(filters.page));
  return params;
}

/** Applies a change; any change other than the page itself goes back to the first page. */
export function patchIdeaFilters(filters: IdeaFilters, patch: Partial<IdeaFilters>): IdeaFilters {
  return { ...filters, page: 1, ...patch };
}

/** Relative URL of a page of the ranking with the same filters ("?sort=gain&page=2"). */
export function ideaFiltersHref(filters: IdeaFilters, page: number): string {
  const query = ideaFiltersToSearchParams({ ...filters, page }).toString();
  return query ? `?${query}` : "?";
}

/** Whether the sort, a filter or the search differs from the defaults (the page does not count). */
export function hasActiveIdeaFilters(filters: IdeaFilters): boolean {
  return ideaFiltersToSearchParams({ ...filters, page: 1 }).size > 0;
}
