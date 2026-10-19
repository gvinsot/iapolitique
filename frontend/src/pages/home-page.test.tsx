import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { IdeaList } from "@/api/types";
import { apiError, apiRoutes } from "@/test/api-routes";
import { stubFetch } from "@/test/fetch-mock";
import { ideaSummaryFixture } from "@/test/fixtures/ideas";
import { statsFixture, taxonomyFixture } from "@/test/fixtures/reference";
import { renderRoute } from "@/test/render-route";

function ideaList(patch: Partial<IdeaList> = {}): IdeaList {
  return {
    items: [
      ideaSummaryFixture,
      { ...ideaSummaryFixture, slug: "autre-idee", title: "Une autre idée", score: 1000 },
    ],
    page: 1,
    per_page: 20,
    total: 2,
    total_pages: 1,
    search: null,
    ...patch,
  };
}

const emptyList = ideaList({ items: [], total: 0, total_pages: 0 });

describe("HomePage ranking", () => {
  it("lists the published ideas in ranking order with the AI notice", async () => {
    stubFetch(
      apiRoutes({
        "GET /api/ideas": ideaList(),
        "GET /api/stats": statsFixture,
        "GET /api/taxonomy": taxonomyFixture,
      }),
    );
    renderRoute("/");

    const list = await screen.findByRole("list", { name: "Idées classées" });
    const cards = within(list).getAllByRole("article");
    expect(cards).toHaveLength(2);
    expect(cards[0]).toHaveTextContent("1.");
    expect(cards[1]).toHaveTextContent("2.");
    expect(within(cards[0]!).getByRole("link", { name: ideaSummaryFixture.title })).toHaveAttribute(
      "href",
      `/idees/${ideaSummaryFixture.slug}`,
    );
    expect(screen.getByRole("status")).toHaveTextContent("2 idées");
    expect(screen.getByRole("note")).toHaveTextContent(/Analyse générée automatiquement/);
    expect(screen.getByRole("link", { name: "toutes les idées en JSON" })).toHaveAttribute(
      "href",
      "/api/exports/ideas.json",
    );
  });

  it("sends the filters of the URL to the API", async () => {
    const fetch = stubFetch(
      apiRoutes({
        "GET /api/ideas": ideaList(),
        "GET /api/stats": statsFixture,
        "GET /api/taxonomy": taxonomyFixture,
      }),
    );
    renderRoute("/?sort=gain&kind=revenue&category=transports&q=%C3%A9nergie&page=2");

    expect(screen.getByRole("combobox", { name: "Trier par" })).toHaveTextContent(
      "Gain annuel estimé",
    );
    expect(screen.getByRole("searchbox", { name: "Rechercher" })).toHaveValue("énergie");
    await screen.findAllByRole("article");
    const request = fetch.mock.calls
      .map(([input]) => new URL((input as Request).url))
      .find((url) => url.pathname === "/api/ideas");
    expect(Object.fromEntries(request?.searchParams ?? [])).toEqual({
      sort: "gain",
      kind: "revenue",
      category: "transports",
      q: "énergie",
      page: "2",
      per_page: "20",
    });
    // Not ranked by score: no rank number.
    expect(screen.getAllByRole("article")[0]).not.toHaveTextContent(/^1\./);
    expect(await screen.findByRole("combobox", { name: "Catégorie" })).toHaveTextContent(
      "Transports",
    );
  });

  it("writes a changed filter to the URL and can reset all filters", async () => {
    stubFetch(apiRoutes({ "GET /api/ideas": ideaList(), "GET /api/stats": statsFixture }));
    const { router, user } = renderRoute("/");
    expect(screen.queryByRole("button", { name: "Réinitialiser les filtres" })).toBeNull();

    await user.click(screen.getByRole("combobox", { name: "Niveau administratif" }));
    await user.click(await screen.findByRole("option", { name: "Commune" }));
    expect(router.state.location.search).toBe("?admin_level=commune");

    await user.type(screen.getByRole("searchbox", { name: "Rechercher" }), "cantines");
    await user.type(screen.getByRole("textbox", { name: "Territoire" }), "Nantes{Enter}");
    const params = new URLSearchParams(router.state.location.search);
    expect(params.get("q")).toBe("cantines");
    expect(params.get("territory")).toBe("Nantes");

    await user.click(screen.getByRole("button", { name: "Réinitialiser les filtres" }));
    expect(router.state.location.search).toBe("");
    expect(screen.getByRole("searchbox", { name: "Rechercher" })).toHaveValue("");
  });

  it("paginates with shareable links that keep the filters", async () => {
    stubFetch(
      apiRoutes({
        "GET /api/ideas": ideaList({ page: 2, total: 45, total_pages: 3 }),
        "GET /api/stats": statsFixture,
      }),
    );
    renderRoute("/?page=2&kind=savings");

    const pagination = await screen.findByRole("navigation", { name: "Pagination" });
    expect(within(pagination).getByRole("link", { name: "Page 3" })).toHaveAttribute(
      "href",
      "/?kind=savings&page=3",
    );
    expect(screen.getByRole("status")).toHaveTextContent("45 idées · page 2 sur 3");
    // Ranks continue from the previous pages.
    expect(screen.getAllByRole("article")[0]).toHaveTextContent("21.");
  });

  it("says when the search only found approaching results", async () => {
    stubFetch(apiRoutes({ "GET /api/ideas": ideaList({ search: "approximate" }) }));
    renderRoute("/?q=cantine");
    expect(await screen.findByText(/voici des résultats approchants/)).toHaveTextContent(
      "« cantine »",
    );
  });

  it("invites visitors to propose an idea while the ranking is empty", async () => {
    stubFetch(apiRoutes({ "GET /api/ideas": emptyList }));
    renderRoute("/");
    expect(
      await screen.findByRole("heading", { name: "Aucune idée publiée pour le moment" }),
    ).toBeInTheDocument();
  });

  it("offers to clear the filters when nothing matches", async () => {
    stubFetch(apiRoutes({ "GET /api/ideas": emptyList }));
    renderRoute("/?q=introuvable");
    expect(
      await screen.findByRole("heading", { name: "Aucune idée ne correspond à ces critères" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Voir toutes les idées" })).toHaveAttribute(
      "href",
      "/",
    );
  });

  it("explains a failure in French and retries", async () => {
    let calls = 0;
    stubFetch(
      apiRoutes({
        "GET /api/ideas": () => {
          calls += 1;
          return calls === 1
            ? apiError(400, "invalid_query", "Le paramètre « category » est inconnu.")
            : ideaList();
        },
      }),
    );
    const { user } = renderRoute("/?category=inconnue");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Le paramètre « category » est inconnu.",
    );
    await user.click(screen.getByRole("button", { name: "Réessayer" }));
    expect(await screen.findAllByRole("article")).toHaveLength(2);
  });

  it("shows the public counters", async () => {
    stubFetch(apiRoutes({ "GET /api/ideas": emptyList, "GET /api/stats": statsFixture }));
    renderRoute("/");
    const stats = await screen.findByRole("region", { name: "La plateforme en chiffres" });
    expect(stats).toHaveTextContent(/idées publiées\s*42/);
    expect(stats).toHaveTextContent(/idées reçues\s*128/);
    expect(stats).toHaveTextContent("Dernière publication le 18 octobre 2026");
  });
});
