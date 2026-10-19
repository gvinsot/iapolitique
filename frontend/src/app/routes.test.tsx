import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { storeAdminToken } from "@/api/admin-token";
import { adminApi, stubFetch } from "@/test/fetch-mock";
import { renderRoute } from "@/test/render-route";

const VALID_TOKEN = "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde";

describe("routes", () => {
  it.each([
    ["/", "Des idées concrètes pour des finances publiques plus saines"],
    ["/proposer", "Proposer une idée"],
    ["/idees/reduire-les-doublons", "Idée « reduire-les-doublons »"],
    [`/suivi#${VALID_TOKEN}`, "Suivi de votre proposition"],
    [`/suivi/${VALID_TOKEN}`, "Suivi de votre proposition"],
    ["/methodologie", "Méthodologie"],
    ["/a-propos", "À propos d’iapolitique"],
    ["/mentions-legales", "Mentions légales"],
    ["/confidentialite", "Politique de confidentialité"],
    ["/regles", "Règles du site"],
    ["/licence", "Licence et réutilisation"],
  ])("%s renders its page heading", async (path, heading) => {
    renderRoute(path);
    expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
  });

  it.each([
    ["/admin", "Tableau de bord"],
    ["/admin/idees", "Idées"],
    ["/admin/idees/01a15179-046d-7418-b8de-ce2167202085/reponses", "Droits de réponse"],
    ["/admin/signalements", "Signalements"],
    ["/admin/moderation", "Signalements"],
    ["/admin/escalades", "Escalades"],
    ["/admin/reexamens", "Réexamens"],
    ["/admin/decisions", "Décisions de filtrage"],
    ["/admin/jobs", "Jobs"],
    ["/admin/couts", "Coûts IA"],
    ["/admin/reglages", "Réglages"],
    ["/admin/journal", "Journal"],
  ])("%s renders its admin page once signed in", async (path, heading) => {
    storeAdminToken("jeton");
    stubFetch(adminApi("jeton"));
    renderRoute(path);
    expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
  });

  it.each(["/n-existe-pas", "/idees", "/admin/inconnu"])(
    "%s renders the 404 page",
    async (path) => {
      storeAdminToken("jeton");
      stubFetch(adminApi("jeton"));
      renderRoute(path);
      expect(
        await screen.findByRole("heading", { level: 1, name: "Page introuvable" }),
      ).toBeInTheDocument();
      expect(screen.getByRole("link", { name: "Retour au classement" })).toHaveAttribute(
        "href",
        "/",
      );
      expect(document.title).toBe("Page introuvable · iapolitique");
    },
  );

  it("keeps the public layout around the 404 page", async () => {
    renderRoute("/page-inconnue");
    await screen.findByRole("heading", { level: 1, name: "Page introuvable" });
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
  });

  it("explains the four-step process on the about page", async () => {
    renderRoute("/a-propos");
    await screen.findByRole("heading", { level: 1, name: "À propos d’iapolitique" });
    for (const step of ["Dépôt anonyme", "Filtre par IA", "Analyse par Mistral", "Classement"]) {
      expect(screen.getByRole("heading", { level: 3, name: new RegExp(step) })).toBeInTheDocument();
    }
  });
});
