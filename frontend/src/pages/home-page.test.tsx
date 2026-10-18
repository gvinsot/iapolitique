import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderRoute } from "@/test/render-route";

describe("HomePage ranking filters", () => {
  it("reads the filters from the URL", () => {
    renderRoute("/?sort=gain&kind=revenue&q=%C3%A9nergie");
    expect(screen.getByRole("combobox", { name: "Trier par" })).toHaveTextContent(
      "Gain annuel estimé",
    );
    expect(screen.getByRole("combobox", { name: "Type de mesure" })).toHaveTextContent(
      "Hausse de recette",
    );
    expect(screen.getByRole("searchbox", { name: "Rechercher" })).toHaveValue("énergie");
  });

  it("writes a changed filter to the URL and can reset all filters", async () => {
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

  it("invites visitors to propose an idea while the ranking is empty", () => {
    renderRoute("/");
    expect(
      screen.getByRole("heading", { name: "Aucune idée publiée pour le moment" }),
    ).toBeInTheDocument();
  });
});
