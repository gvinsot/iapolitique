import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { storeAdminToken } from "@/api/admin-token";
import { apiError, apiRoutes } from "@/test/api-routes";
import { stubFetch } from "@/test/fetch-mock";
import {
  adminPage,
  dashboardFixture,
  IDEA_ID,
  ideasFixture,
  jobsFixture,
  repliesFixture,
  screeningsFixture,
  settingsFixture,
} from "@/test/fixtures/admin";
import { renderRoute } from "@/test/render-route";

const TOKEN = "jeton-admin";

interface Call {
  method: string;
  path: string;
  query: Record<string, string>;
  body: unknown;
  authorization: string | null;
}

/** Fake admin API: checks the bearer token like the backend and records every call. */
function adminRoutes(routes: Record<string, unknown>) {
  const calls: Call[] = [];
  const handler = apiRoutes({ "GET /api/admin/session": { authenticated: true }, ...routes });
  stubFetch(async (request) => {
    const url = new URL(request.url);
    const text = request.method === "GET" ? "" : await request.clone().text();
    calls.push({
      method: request.method,
      path: url.pathname,
      query: Object.fromEntries(url.searchParams),
      body: text ? (JSON.parse(text) as unknown) : null,
      authorization: request.headers.get("Authorization"),
    });
    if (
      url.pathname.startsWith("/api/admin") &&
      request.headers.get("Authorization") !== `Bearer ${TOKEN}`
    ) {
      return apiError(401, "unauthorized", "Jeton d’administration invalide.");
    }
    return handler(request);
  });
  storeAdminToken(TOKEN);
  return calls;
}

describe("admin pages", () => {
  it("shows the dashboard and its queues", async () => {
    adminRoutes({ "GET /api/admin/dashboard": dashboardFixture });
    renderRoute("/admin");
    expect(await screen.findByText("Jobs abandonnés")).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Filtrage sur 30 jours" })).toBeInTheDocument();
  });

  it("filters the ideas through the URL and hides one with a reason", async () => {
    const published = ideasFixture[1]!;
    const calls = adminRoutes({
      "GET /api/admin/ideas": adminPage(ideasFixture),
      [`POST /api/admin/ideas/${published.id}/hide`]: { ...published, status: "hidden" },
    });
    const { user, router } = renderRoute("/admin/idees?status=published");
    await screen.findByRole("table", { name: "Idées" });
    expect(calls.find((call) => call.path === "/api/admin/ideas")?.query).toMatchObject({
      status: "published",
      page: "1",
      per_page: "25",
    });

    await user.selectOptions(screen.getByLabelText("Statut"), "hidden");
    expect(router.state.location.search).toBe("?status=hidden");

    await user.click(await screen.findByRole("button", { name: "Masquer « Une idée publiée »" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByRole("textbox", { name: "Motif" }), "Chiffrage trompeur");
    await user.click(within(dialog).getByRole("button", { name: "Masquer l’idée" }));
    expect(await screen.findByText(/Idée masquée/)).toBeInTheDocument();
    const hide = calls.find((call) => call.method === "POST");
    expect(hide?.body).toEqual({ reason: "Chiffrage trompeur" });
    expect(hide?.authorization).toBe(`Bearer ${TOKEN}`);
  });

  it("retries a dead job and refreshes the list", async () => {
    const calls = adminRoutes({
      "GET /api/admin/jobs": adminPage(jobsFixture),
      "GET /api/admin/dashboard": dashboardFixture,
      "POST /api/admin/jobs/42/retry": { ...jobsFixture[0], status: "queued" },
    });
    const { user } = renderRoute("/admin/jobs?status=dead");
    await user.click(await screen.findByRole("button", { name: "Relancer le job n° 42" }));
    expect(await screen.findByText("Job n° 42 remis en file.")).toBeInTheDocument();
    await waitFor(() => {
      expect(calls.filter((call) => call.path === "/api/admin/jobs").length).toBeGreaterThan(1);
    });
  });

  it("labels a screening decision and downloads the calibration export", async () => {
    const calls = adminRoutes({
      "GET /api/admin/screenings": adminPage(screeningsFixture),
      "PUT /api/admin/screenings/s-1/label": {
        ...screeningsFixture[0],
        label: {
          label: "correct",
          expected: "reject",
          note: null,
          labeled_at: "2026-10-19T08:00:00Z",
        },
      },
      "GET /api/admin/screenings/export.jsonl": () =>
        new Response('{"id":"s-1"}\n', { headers: { "Content-Type": "application/x-ndjson" } }),
    });
    const createObjectURL = vi.fn(() => "blob:export");
    const revokeObjectURL = vi.fn();
    Object.assign(URL, { createObjectURL, revokeObjectURL });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);

    const { user } = renderRoute("/admin/decisions?labeled=false");
    expect(await screen.findByText("Allonger les vacances d’été.")).toBeInTheDocument();
    expect(calls.find((call) => call.path === "/api/admin/screenings")?.query).toMatchObject({
      labeled: "false",
    });
    await user.click(screen.getAllByRole("button", { name: "Décision juste" })[0]!);
    await waitFor(() => {
      expect(calls.find((call) => call.method === "PUT")?.body).toEqual({ label: "correct" });
    });

    await user.selectOptions(screen.getByLabelText("Fichier"), "samples");
    await user.click(screen.getByRole("button", { name: "Télécharger" }));
    await waitFor(() => {
      expect(click).toHaveBeenCalled();
    });
    const exportCall = calls.find((call) => call.path === "/api/admin/screenings/export.jsonl");
    expect(exportCall?.query).toEqual({ part: "samples" });
    expect(exportCall?.authorization).toBe(`Bearer ${TOKEN}`);
    expect(createObjectURL).toHaveBeenCalled();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:export");
    // jsdom has no object URLs: remove the stubs added above.
    Reflect.deleteProperty(URL, "createObjectURL");
    Reflect.deleteProperty(URL, "revokeObjectURL");
  });

  it("returns to the thresholds of the file", async () => {
    const calls = adminRoutes({
      "GET /api/admin/settings": settingsFixture,
      "PUT /api/admin/settings/screening-thresholds": settingsFixture,
    });
    const { user } = renderRoute("/admin/reglages");
    await user.click(await screen.findByRole("button", { name: "Revenir aux seuils du fichier" }));
    expect(await screen.findByText(/screening.yaml s’appliquent de nouveau/)).toBeInTheDocument();
    expect(calls.find((call) => call.method === "PUT")?.body).toEqual({});
  });

  it("publishes a right of reply under an idea", async () => {
    const calls = adminRoutes({
      "GET /api/admin/ideas": adminPage([ideasFixture[0]]),
      [`GET /api/admin/ideas/${IDEA_ID}/replies`]: repliesFixture,
      [`POST /api/admin/ideas/${IDEA_ID}/replies`]: repliesFixture[0],
    });
    const { user } = renderRoute(`/admin/idees/${IDEA_ID}/reponses`);
    expect(await screen.findByText(/Auteur : l’association des maires/)).toBeInTheDocument();
    expect(calls.find((call) => call.path === "/api/admin/ideas")?.query).toMatchObject({
      q: IDEA_ID,
    });

    await user.type(screen.getByRole("textbox", { name: "Auteur de la réponse" }), "la mairie");
    await user.type(
      screen.getByRole("textbox", { name: /Texte de la réponse/ }),
      "Ce dispositif existe déjà.",
    );
    await user.click(screen.getByRole("button", { name: "Publier la réponse" }));
    expect(await screen.findByText("Réponse publiée sous l’idée.")).toBeInTheDocument();
    expect(calls.find((call) => call.method === "POST")?.body).toMatchObject({
      author_label: "la mairie",
      body: "Ce dispositif existe déjà.",
    });
  });

  it("explains a failure of the admin API in French", async () => {
    adminRoutes({
      "GET /api/admin/reports": apiError(400, "invalid_query", "Paramètre inconnu : « tri »."),
    });
    renderRoute("/admin/signalements");
    expect(await screen.findByRole("alert")).toHaveTextContent("Paramètre inconnu : « tri ».");
  });
});
