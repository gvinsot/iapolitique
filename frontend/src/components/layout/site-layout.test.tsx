import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderRoute } from "@/test/render-route";

describe("SiteLayout", () => {
  it("exposes the header, main content and footer landmarks", () => {
    renderRoute("/");

    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("link", { name: /iapolitique/ })).toHaveAttribute("href", "/");
    expect(
      within(banner).getByText(
        "Des idées concrètes pour des finances publiques plus saines, sans parti pris",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveAttribute("id", "contenu");
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
  });

  it("offers a skip link as the first focusable element", async () => {
    const { user } = renderRoute("/");
    await user.tab();
    const skipLink = screen.getByRole("link", { name: "Aller au contenu principal" });
    expect(skipLink).toHaveFocus();
    expect(skipLink).toHaveAttribute("href", "#contenu");
  });

  it("lists the main sections in the navigation and marks the current page", () => {
    renderRoute("/methodologie");
    const nav = screen.getByRole("navigation", { name: "Navigation principale" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((link) => [link.textContent, link.getAttribute("href")])).toEqual([
      ["Classement", "/"],
      ["Proposer une idée", "/proposer"],
      ["Méthodologie", "/methodologie"],
      ["À propos", "/a-propos"],
    ]);
    expect(within(nav).getByRole("link", { name: "Méthodologie" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: "Classement" })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("shows legal links and the mandatory notices in the footer", () => {
    renderRoute("/");
    const footer = screen.getByRole("contentinfo");
    const legal = within(footer).getByRole("navigation", { name: "Informations légales" });
    expect(within(legal).getByRole("link", { name: "Mentions légales" })).toHaveAttribute(
      "href",
      "/mentions-legales",
    );
    expect(within(legal).getByRole("link", { name: "Confidentialité" })).toHaveAttribute(
      "href",
      "/confidentialite",
    );
    expect(within(legal).getByRole("link", { name: "Règles du site (CGU)" })).toHaveAttribute(
      "href",
      "/regles",
    );
    expect(within(legal).getByRole("link", { name: "Licence et réutilisation" })).toHaveAttribute(
      "href",
      "/licence",
    );
    expect(
      within(footer).getByText(
        "Site citoyen indépendant et non officiel, sans lien avec l’État ni aucune administration",
      ),
    ).toBeInTheDocument();
    expect(within(footer).getByText("Analyses générées par IA.")).toBeInTheDocument();
    expect(within(footer).getByText(/ni un sondage ni une mesure d’opinion/)).toBeInTheDocument();
  });

  it("opens and closes the mobile menu", async () => {
    const { user } = renderRoute("/");
    const toggle = screen.getByRole("button", { name: "Ouvrir le menu" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("navigation", { name: "Navigation principale (mobile)" }),
    ).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAccessibleName("Fermer le menu");
    const mobileNav = screen.getByRole("navigation", { name: "Navigation principale (mobile)" });

    await user.click(within(mobileNav).getByRole("link", { name: "À propos" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "À propos d’iapolitique" }),
    ).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("closes the mobile menu with Escape", async () => {
    const { user } = renderRoute("/");
    const toggle = screen.getByRole("button", { name: "Ouvrir le menu" });
    await user.click(toggle);
    await user.keyboard("{Escape}");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("moves focus to the main content and updates the title after navigation", async () => {
    const { user } = renderRoute("/");
    expect(document.title).toBe(
      "iapolitique · Des idées concrètes pour des finances publiques plus saines, sans parti pris",
    );
    const nav = screen.getByRole("navigation", { name: "Navigation principale" });
    await user.click(within(nav).getByRole("link", { name: "Méthodologie" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Méthodologie" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
    expect(document.title).toBe("Méthodologie · iapolitique");
  });
});
