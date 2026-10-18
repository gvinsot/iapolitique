import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { readAdminToken, storeAdminToken } from "@/api/admin-token";
import { adminApi, stubFetch } from "@/test/fetch-mock";
import { renderRoute } from "@/test/render-route";

describe("admin access", () => {
  it("asks for the token before showing any admin page", async () => {
    const fetch = stubFetch(adminApi("bon-jeton"));
    renderRoute("/admin/couts");

    expect(
      await screen.findByRole("heading", { level: 1, name: "Accès à l’administration" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("navigation", { name: "Navigation de l’administration" }),
    ).not.toBeInTheDocument();
    expect(fetch).not.toHaveBeenCalled();
  });

  it("requires a non-empty token", async () => {
    stubFetch(adminApi("bon-jeton"));
    const { user } = renderRoute("/admin");
    await user.click(await screen.findByRole("button", { name: "Se connecter" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Saisissez le jeton.");
  });

  it("signs in with a valid token and keeps it for the tab only", async () => {
    stubFetch(adminApi("bon-jeton"));
    const { user } = renderRoute("/admin/couts");

    await user.type(await screen.findByLabelText("Jeton d’administration"), "bon-jeton");
    await user.click(screen.getByRole("button", { name: "Se connecter" }));

    expect(await screen.findByRole("heading", { level: 1, name: "Coûts IA" })).toBeInTheDocument();
    expect(
      screen.getByRole("navigation", { name: "Navigation de l’administration" }),
    ).toBeInTheDocument();
    expect(window.sessionStorage.getItem("iapolitique-admin-token")).toBe("bon-jeton");
    expect(window.localStorage.length).toBe(0);
  });

  it("reports a refused token", async () => {
    stubFetch(adminApi("bon-jeton"));
    const { user } = renderRoute("/admin");

    await user.type(await screen.findByLabelText("Jeton d’administration"), "mauvais");
    await user.click(screen.getByRole("button", { name: "Se connecter" }));

    expect(await screen.findByText("Jeton refusé")).toBeInTheDocument();
  });

  it("reuses a stored token and signs out", async () => {
    storeAdminToken("bon-jeton");
    stubFetch(adminApi("bon-jeton"));
    const { user } = renderRoute("/admin");

    expect(
      await screen.findByRole("heading", { level: 1, name: "Tableau de bord" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Se déconnecter" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Accès à l’administration" }),
    ).toBeInTheDocument();
    expect(readAdminToken()).toBeNull();
  });

  it("explains when the admin API cannot be reached", async () => {
    storeAdminToken("bon-jeton");
    stubFetch(() => Promise.reject(new TypeError("Failed to fetch")));
    renderRoute("/admin");

    expect(await screen.findByText("Administration injoignable")).toBeInTheDocument();
    expect(screen.getByText(/réseau local/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Réessayer" })).toBeInTheDocument();
  });
});
