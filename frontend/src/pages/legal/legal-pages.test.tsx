import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { mockClipboard } from "@/test/browser-mocks";
import { stubFetch } from "@/test/fetch-mock";
import { legalApi, legalInfoFixture } from "@/test/legal-api";
import { renderRoute } from "@/test/render-route";

describe("legal pages", () => {
  it("shows the publisher identity, contact and page date from the API", async () => {
    stubFetch(legalApi());
    renderRoute("/mentions-legales");

    await screen.findByRole("heading", { level: 1, name: "Mentions légales" });
    const editor = await screen.findByText(/est édité à titre non professionnel par/);
    expect(editor).toHaveTextContent("Camille Exemple");
    expect(editor).toHaveTextContent("1 rue de l’Exemple, 75000 Paris");
    expect(editor).toHaveTextContent("01 23 45 67 89");
    expect(within(editor).getByRole("link", { name: "contact@example.org" })).toHaveAttribute(
      "href",
      "mailto:contact@example.org",
    );
    expect(screen.getByText(/Site auto-hébergé|auto-hébergé par l’éditeur/)).toBeInTheDocument();
    expect(screen.getByText("18 octobre 2026")).toHaveAttribute("datetime", "2026-10-18");
    expect(screen.queryByText("Contenu juridique à compléter")).not.toBeInTheDocument();
  });

  it("degrades to a visible notice when the publisher details are unavailable", async () => {
    stubFetch(legalApi({ error: { code: "server_error", message: "Erreur." } }, 500));
    renderRoute("/mentions-legales");

    expect(await screen.findByText("Coordonnées momentanément indisponibles")).toBeInTheDocument();
    expect(screen.getAllByText("coordonnées momentanément indisponibles").length).toBeGreaterThan(
      2,
    );
    // The texts shipped with the site keep their own date.
    expect(screen.getByText("18 octobre 2026")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Réessayer" })).toBeInTheDocument();
  });

  it("treats an incomplete answer as unavailable", async () => {
    const { publisher: _publisher, ...incomplete } = legalInfoFixture;
    stubFetch(legalApi(incomplete));
    renderRoute("/confidentialite");
    expect(await screen.findByText("Coordonnées momentanément indisponibles")).toBeInTheDocument();
  });

  it("describes processings, retention periods, local storage and rights", async () => {
    stubFetch(legalApi());
    renderRoute("/confidentialite");
    await screen.findByRole("heading", { level: 1, name: "Politique de confidentialité" });

    expect(
      await screen.findAllByRole("link", { name: "confidentialite@example.org" }),
    ).not.toHaveLength(0);
    for (const heading of [
      "Textes soumis (idées, améliorations, objections)",
      "Lien de suivi",
      "Signalements",
      "Protection contre les abus",
    ]) {
      expect(screen.getByRole("heading", { level: 3, name: heading })).toBeInTheDocument();
    }
    expect(screen.getByText(/supprimé 30 jours après le refus/)).toBeInTheDocument();
    expect(screen.getByText(/supprimé 12 mois après le masquage/)).toBeInTheDocument();
    expect(screen.getByText(/6 mois après leur traitement/)).toBeInTheDocument();
    expect(screen.getByText("iapolitique-theme")).toBeInTheDocument();
    expect(screen.getByText("iapolitique-tracking-links")).toBeInTheDocument();
    expect(screen.getByText(/votre lien de suivi est la preuve/)).toBeInTheDocument();
    expect(screen.getByText(/sous-traitant au sens de l’article 28/)).toBeInTheDocument();
  });

  it("publishes the numbered criteria, the score parameters and the licence clause", async () => {
    stubFetch(legalApi());
    renderRoute("/regles");
    await screen.findByRole("heading", { level: 1, name: "Règles du site" });

    expect(await screen.findByText("1.0")).toBeInTheDocument();
    expect(screen.getByText("20 octobre 2026")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 3, name: "4.4. Aucun particulier identifiable" }),
    ).toBeInTheDocument();
    expect(document.getElementById("critere-sujet")).toHaveTextContent("4.5.");
    expect(document.getElementById("score")).toHaveTextContent("Article 9 – Classement et score");
    expect(
      screen.getByText(/ni un sondage, ni un vote, ni une mesure d’opinion/),
    ).toBeInTheDocument();
    const licence = document.getElementById("licence-contribution")!;
    expect(licence).toHaveTextContent("Article 11 – Licence de votre contribution");
    expect(
      screen.getByText(/sous-licencier ces droits au public sous Licence Ouverte 2.0/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Contribution anonyme – iapolitique.fr/)).toBeInTheDocument();
    expect(document.getElementById("recours")).toHaveTextContent("Voies de recours");
  });

  it("redirects the former /cgu address, anchor included", async () => {
    stubFetch(legalApi());
    const { router } = renderRoute("/cgu#score");
    await screen.findByRole("heading", { level: 1, name: "Règles du site" });
    expect(router.state.location.pathname).toBe("/regles");
    expect(router.state.location.hash).toBe("#score");
  });

  it("offers the exports and a ready-to-copy attribution", async () => {
    stubFetch(legalApi());
    const writeText = vi.fn(() => Promise.resolve());
    const { user } = renderRoute("/licence");
    mockClipboard({ writeText });
    await screen.findByRole("heading", { level: 1, name: "Licence et réutilisation" });

    expect(screen.getByRole("link", { name: "Télécharger au format JSON" })).toHaveAttribute(
      "href",
      "/api/exports/ideas.json",
    );
    expect(screen.getByRole("link", { name: "Télécharger au format CSV" })).toHaveAttribute(
      "href",
      "/api/exports/ideas.csv",
    );
    expect(screen.getByText(/compatible avec la licence/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Copier la mention" }));
    expect(writeText).toHaveBeenCalledWith(
      expect.stringMatching(
        /^Source : iapolitique\.fr – .* mise à jour du AAAA-MM-JJ – Licence Ouverte 2\.0\. Analyses produites automatiquement/,
      ),
    );
    expect(await screen.findByText("Copié dans le presse-papiers.")).toBeInTheDocument();
  });

  it("invites to copy by hand when the clipboard is unavailable", async () => {
    stubFetch(legalApi());
    const { user } = renderRoute("/licence");
    mockClipboard(undefined);
    await user.click(await screen.findByRole("button", { name: "Copier la mention" }));
    expect(await screen.findByText(/La copie automatique a échoué/)).toBeInTheDocument();
  });
});
