import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, type Mock } from "vitest";

import { ADMIN_TOKEN_KEY, readAdminToken, storeAdminToken } from "@/api/admin-token";
import { apiError } from "@/test/api-routes";
import { adminApi, stubFetch } from "@/test/fetch-mock";
import { renderRoute } from "@/test/render-route";

/** The public port: no admin route at all (AGENTS.md §14.4). */
function publicHost(): Response {
  return apiError(404, "not_found", "Ressource introuvable.");
}

/** `Authorization` header of every request the page sent. */
function sentAuthorization(fetch: Mock): (string | null)[] {
  return fetch.mock.calls.map(([input]) =>
    input instanceof Request ? input.headers.get("Authorization") : null,
  );
}

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
    // Only the token-less probe that proves the admin API answers here (401).
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(sentAuthorization(fetch)).toEqual([null]);
  });

  it("shows no sign-in form on the public host and sends no token there (ADM-1, FE-5)", async () => {
    const fetch = stubFetch(publicHost);
    renderRoute("/admin");

    expect(
      await screen.findByText("Administration indisponible à cette adresse"),
    ).toBeInTheDocument();
    expect(screen.getByText(/uniquement sur l’adresse du réseau local/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Jeton d’administration")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Réessayer" })).not.toBeInTheDocument();
    expect(sentAuthorization(fetch)).toEqual([null]);
  });

  it("drops a stored token on the public host without ever sending it (ADM-1, FE-5)", async () => {
    storeAdminToken("bon-jeton");
    const fetch = stubFetch(publicHost);
    renderRoute("/admin");

    expect(
      await screen.findByText("Administration indisponible à cette adresse"),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(readAdminToken()).toBeNull();
    });
    expect(window.sessionStorage.getItem(ADMIN_TOKEN_KEY)).toBeNull();
    expect(screen.queryByRole("button", { name: "Se déconnecter" })).not.toBeInTheDocument();
    expect(sentAuthorization(fetch).every((value) => value === null)).toBe(true);
  });

  it("drops a token that the API answers with 404", async () => {
    storeAdminToken("bon-jeton");
    // Probe answered by an admin API, then 404 for the authenticated call (e.g. a proxy that
    // routes /api/admin only without credentials): the token is dropped the same way.
    stubFetch((request) =>
      request.headers.get("Authorization") === null
        ? apiError(401, "unauthorized", "Jeton d’administration invalide.")
        : apiError(404, "not_found", "Ressource introuvable."),
    );
    renderRoute("/admin");

    expect(
      await screen.findByText("Administration indisponible à cette adresse"),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(readAdminToken()).toBeNull();
    });
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
    // Finding AUTH-3: a refused token never stays in the tab.
    expect(window.sessionStorage.getItem(ADMIN_TOKEN_KEY)).toBeNull();
  });

  it("forgets a token refused in the middle of a session and asks for a new one (AUTH-3)", async () => {
    storeAdminToken("ancien-jeton");
    let rotated = false;
    const fetch = stubFetch((request) => {
      const url = new URL(request.url);
      const authorization = request.headers.get("Authorization");
      if (authorization === null || rotated) {
        return apiError(401, "unauthorized", "Jeton d’administration invalide.");
      }
      if (url.pathname === "/api/admin/session") return Response.json({ authenticated: true });
      // The token is rotated while the page is open: the next admin call is refused.
      rotated = true;
      return apiError(401, "unauthorized", "Jeton d’administration invalide.");
    });
    renderRoute("/admin/couts");

    expect(await screen.findByText("Jeton refusé")).toBeInTheDocument();
    expect(screen.getByLabelText("Jeton d’administration")).toBeInTheDocument();
    expect(readAdminToken()).toBeNull();
    expect(
      screen.queryByRole("navigation", { name: "Navigation de l’administration" }),
    ).not.toBeInTheDocument();
    const sent = sentAuthorization(fetch).filter((value) => value !== null);
    expect(sent.every((value) => value === "Bearer ancien-jeton")).toBe(true);
  });

  it("shows the French message of the lockout and keeps the form (AUTH-3)", async () => {
    storeAdminToken("bon-jeton");
    stubFetch((request) =>
      request.headers.get("Authorization") === null
        ? apiError(401, "unauthorized", "Jeton d’administration invalide.")
        : new Response(
            JSON.stringify({
              error: {
                code: "admin_locked",
                message:
                  "Trop d'échecs d'authentification : l'administration est verrouillée quelques minutes.",
              },
            }),
            { status: 429, headers: { "Content-Type": "application/json", "Retry-After": "240" } },
          ),
    );
    renderRoute("/admin");

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Administration verrouillée");
    expect(alert).toHaveTextContent("l'administration est verrouillée quelques minutes.");
    expect(alert).toHaveTextContent("Réessayez dans 4 minutes.");
    expect(screen.getByLabelText("Jeton d’administration")).toBeInTheDocument();
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
