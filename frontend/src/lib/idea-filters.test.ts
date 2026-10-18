import { describe, expect, it } from "vitest";

import {
  defaultIdeaFilters,
  hasActiveIdeaFilters,
  ideaFiltersHref,
  ideaFiltersToSearchParams,
  parseIdeaFilters,
  patchIdeaFilters,
} from "./idea-filters";

describe("idea filters", () => {
  it("parses known values and falls back to defaults otherwise", () => {
    expect(parseIdeaFilters(new URLSearchParams(""))).toEqual(defaultIdeaFilters);
    expect(
      parseIdeaFilters(
        new URLSearchParams("sort=feasibility&admin_level=region&kind=mixed&q=%20bus%20"),
      ),
    ).toEqual({
      ...defaultIdeaFilters,
      sort: "feasibility",
      adminLevel: "region",
      kind: "mixed",
      q: "bus",
    });
    expect(
      parseIdeaFilters(
        new URLSearchParams(
          "sort=votes&admin_level=europe&kind=__proto__&category=Robert%20Tables&page=-3",
        ),
      ),
    ).toEqual(defaultIdeaFilters);
  });

  it("caps the search length", () => {
    const filters = parseIdeaFilters(new URLSearchParams({ q: "x".repeat(500) }));
    expect(filters.q).toHaveLength(200);
  });

  it("omits defaults when serialising and round-trips", () => {
    expect(ideaFiltersToSearchParams(defaultIdeaFilters).toString()).toBe("");
    const filters = {
      sort: "delay",
      adminLevel: "commune",
      category: "transports",
      kind: "savings",
      territory: "Lyon",
      q: "eau",
      page: 3,
    } as const;
    const params = ideaFiltersToSearchParams(filters);
    expect(params.toString()).toBe(
      "sort=delay&admin_level=commune&category=transports&kind=savings&territory=Lyon&q=eau&page=3",
    );
    expect(parseIdeaFilters(params)).toEqual(filters);
  });

  it("parses pages defensively", () => {
    expect(parseIdeaFilters(new URLSearchParams("page=2")).page).toBe(2);
    for (const page of ["0", "1.5", "abc", "999999", ""]) {
      expect(parseIdeaFilters(new URLSearchParams({ page })).page).toBe(1);
    }
  });

  it("goes back to the first page when a filter changes and builds page links", () => {
    const onPage3 = { ...defaultIdeaFilters, kind: "revenue", page: 3 } as const;
    expect(patchIdeaFilters(onPage3, { sort: "gain" }).page).toBe(1);
    expect(patchIdeaFilters(onPage3, { page: 4 }).page).toBe(4);
    expect(ideaFiltersHref(onPage3, 2)).toBe("?kind=revenue&page=2");
    expect(ideaFiltersHref(defaultIdeaFilters, 1)).toBe("?");
  });

  it("detects active filters", () => {
    expect(hasActiveIdeaFilters({ ...defaultIdeaFilters, page: 4 })).toBe(false);
    expect(hasActiveIdeaFilters({ ...defaultIdeaFilters, territory: "Nantes" })).toBe(true);
    expect(hasActiveIdeaFilters(defaultIdeaFilters)).toBe(false);
    expect(hasActiveIdeaFilters({ ...defaultIdeaFilters, q: "  " })).toBe(false);
    expect(hasActiveIdeaFilters({ ...defaultIdeaFilters, kind: "revenue" })).toBe(true);
  });
});
