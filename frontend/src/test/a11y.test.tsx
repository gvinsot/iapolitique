import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { storeAdminToken } from "@/api/admin-token";

import { audit } from "./axe";
import { adminApi, stubFetch } from "./fetch-mock";
import { renderRoute } from "./render-route";

const publicRoutes: [path: string, heading: string][] = [
  ["/", "Des idées concrètes pour des finances publiques plus saines"],
  ["/proposer", "Proposer une idée"],
  ["/idees/exemple", "Idée « exemple »"],
  ["/suivi", "Suivi de votre proposition"],
  ["/suivi#AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde", "Suivi de votre proposition"],
  ["/suivi#invalide", "Suivi de votre proposition"],
  ["/methodologie", "Méthodologie"],
  ["/a-propos", "À propos d’iapolitique"],
  ["/mentions-legales", "Mentions légales"],
  ["/confidentialite", "Politique de confidentialité"],
  ["/regles", "Règles du site"],
  ["/licence", "Licence et réutilisation"],
  ["/introuvable", "Page introuvable"],
];

describe("accessibility audit", () => {
  it.each(publicRoutes)("%s has no detectable violation", async (path, heading) => {
    renderRoute(path);
    await screen.findByRole("heading", { level: 1, name: heading });
    expect(await audit()).toEqual([]);
  });

  it("the dark theme has no detectable violation", async () => {
    window.localStorage.setItem("iapolitique-theme", "dark");
    renderRoute("/");
    await screen.findByRole("heading", { level: 1 });
    expect(document.documentElement).toHaveClass("dark");
    expect(await audit()).toEqual([]);
  });

  it("the admin sign-in has no detectable violation", async () => {
    stubFetch(adminApi("jeton"));
    renderRoute("/admin");
    await screen.findByRole("heading", { level: 1, name: "Accès à l’administration" });
    expect(await audit()).toEqual([]);
  });

  it("the admin dashboard has no detectable violation", async () => {
    storeAdminToken("jeton");
    stubFetch(adminApi("jeton"));
    renderRoute("/admin");
    await screen.findByRole("heading", { level: 1, name: "Tableau de bord" });
    expect(await audit()).toEqual([]);
  });
});
