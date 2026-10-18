import { describe, expect, it } from "vitest";

import {
  defaultIdeaFilters,
  hasActiveIdeaFilters,
  ideaFiltersToSearchParams,
  parseIdeaFilters,
} from "./idea-filters";

describe("idea filters", () => {
  it("parses known values and falls back to defaults otherwise", () => {
    expect(parseIdeaFilters(new URLSearchParams(""))).toEqual(defaultIdeaFilters);
    expect(
      parseIdeaFilters(
        new URLSearchParams("sort=feasibility&admin_level=region&kind=mixed&q=%20bus%20"),
      ),
    ).toEqual({ sort: "feasibility", adminLevel: "region", kind: "mixed", q: "bus" });
    expect(
      parseIdeaFilters(new URLSearchParams("sort=votes&admin_level=europe&kind=__proto__")),
    ).toEqual(defaultIdeaFilters);
  });

  it("caps the search length", () => {
    const filters = parseIdeaFilters(new URLSearchParams({ q: "x".repeat(500) }));
    expect(filters.q).toHaveLength(200);
  });

  it("omits defaults when serialising and round-trips", () => {
    expect(ideaFiltersToSearchParams(defaultIdeaFilters).toString()).toBe("");
    const filters = { sort: "delay", adminLevel: "commune", kind: "savings", q: "eau" } as const;
    const params = ideaFiltersToSearchParams(filters);
    expect(params.toString()).toBe("sort=delay&admin_level=commune&kind=savings&q=eau");
    expect(parseIdeaFilters(params)).toEqual(filters);
  });

  it("detects active filters", () => {
    expect(hasActiveIdeaFilters(defaultIdeaFilters)).toBe(false);
    expect(hasActiveIdeaFilters({ ...defaultIdeaFilters, q: "  " })).toBe(false);
    expect(hasActiveIdeaFilters({ ...defaultIdeaFilters, kind: "revenue" })).toBe(true);
  });
});
