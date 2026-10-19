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
  sealFixture,
  sealsFixture,
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

  it("shows the logs of a job as plain text and fetches only the new lines", async () => {
    const running = { ...jobsFixture[0], status: "running" as const, finished_at: null };
    const line = (id: number, message: string, level = "info") => ({
      id,
      logged_at: "2026-10-19T16:00:00Z",
      level,
      target: "iapolitique::ai::mistral",
      message,
      fields: { purpose: "analysis_research", attempt: 1 },
    });
    const calls = adminRoutes({
      "GET /api/admin/jobs": adminPage([running]),
      "GET /api/admin/jobs/42/logs": (_request: Request, url: URL) =>
        url.searchParams.get("after") === "2"
          ? { job: running, items: [line(3, "web research answer structure")], has_more: false }
          : {
              job: running,
              items: [
                line(1, "Mistral call started"),
                line(2, "<b>Mistral call failed</b>", "warn"),
              ],
              has_more: false,
            },
    });
    const { user } = renderRoute("/admin/jobs");
    await user.click(await screen.findByRole("button", { name: "Voir les logs du job n° 42" }));
    const dialog = await screen.findByRole("dialog", { name: /Logs du job n° 42/ });
    expect(await within(dialog).findByText("Mistral call started")).toBeInTheDocument();
    // Plain text: never rendered as HTML.
    expect(within(dialog).getByText("<b>Mistral call failed</b>")).toBeInTheDocument();
    expect(within(dialog).getAllByText(/purpose=analysis_research attempt=1/)).toHaveLength(2);
    expect(
      within(dialog).getByText(/Actualisation automatique toutes les 3 secondes/),
    ).toBeVisible();

    await user.click(within(dialog).getByRole("button", { name: "Actualiser" }));
    expect(await within(dialog).findByText("web research answer structure")).toBeInTheDocument();
    expect(within(dialog).getByText(/3 lignes/)).toBeInTheDocument();
    const reads = calls.filter((call) => call.path === "/api/admin/jobs/42/logs");
    expect(reads[0]?.query).toEqual({});
    expect(reads.at(-1)?.query).toEqual({ after: "2" });
    expect(reads.every((call) => call.authorization === `Bearer ${TOKEN}`)).toBe(true);
  });

  it("does not refresh the logs of a finished job by itself", async () => {
    adminRoutes({
      "GET /api/admin/jobs": adminPage(jobsFixture),
      "GET /api/admin/jobs/42/logs": { job: jobsFixture[0], items: [], has_more: false },
    });
    const { user } = renderRoute("/admin/jobs");
    await user.click(await screen.findByRole("button", { name: "Voir les logs du job n° 42" }));
    const dialog = await screen.findByRole("dialog", { name: /Logs du job n° 42/ });
    expect(await within(dialog).findByText(/Aucune ligne de log pour ce job/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Le job est terminé/)).toBeInTheDocument();
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

  it("reads a sealed copy only once the administrator asks for it", async () => {
    const calls = adminRoutes({
      "GET /api/admin/escalation-seals": adminPage(sealsFixture),
      "GET /api/admin/escalations/e-1/seal": sealFixture,
    });
    const { user } = renderRoute("/admin/copies-scellees");
    const [first] = await screen.findAllByRole("link", {
      name: /Consulter la copie scellée de l’escalade/,
    });
    if (!first) throw new Error("missing link");
    await user.click(first);
    expect(
      await screen.findByRole("heading", { name: "Consultation journalisée" }),
    ).toBeInTheDocument();
    expect(calls.some((call) => call.path.endsWith("/seal"))).toBe(false);

    await user.click(screen.getByRole("button", { name: "Afficher la copie scellée" }));
    expect(await screen.findByRole("article", { name: "Copie scellée" })).toBeInTheDocument();
    const reads = calls.filter((call) => call.path === "/api/admin/escalations/e-1/seal");
    expect(reads).toHaveLength(1);
    expect(reads[0]?.authorization).toBe(`Bearer ${TOKEN}`);
  });

  it("says when a sealed copy no longer exists", async () => {
    adminRoutes({
      "GET /api/admin/escalations/e-9/seal": apiError(
        404,
        "seal_not_found",
        "Aucune copie scellée pour cette escalade.",
      ),
    });
    const { user } = renderRoute("/admin/escalades/e-9/copie-scellee");
    await user.click(await screen.findByRole("button", { name: "Afficher la copie scellée" }));
    expect(
      await screen.findByText("Aucune copie scellée pour cette escalade."),
    ).toBeInTheDocument();
  });

  it("checks the Mistral key on demand and shows the result", async () => {
    const calls = adminRoutes({
      "GET /api/admin/dashboard": {
        ...dashboardFixture,
        alerts: [
          {
            code: "mistral_auth",
            since: "2026-10-19T06:00:00Z",
            last_seen_at: "2026-10-19T07:45:00Z",
            http_status: 401,
            failures: 3,
            message: "Clé Mistral refusée depuis le 19/10/2026 à 08:00 (HTTP 401).",
          },
        ],
      },
      "POST /api/admin/provider-alerts/mistral/check": {
        result: "ok",
        http_status: 200,
        alert_cleared: true,
        jobs_due: 2,
        message: "Clé Mistral acceptée : alerte close, 2 jobs IA en attente repartent maintenant.",
      },
    });
    const { user } = renderRoute("/admin");
    await user.click(
      await screen.findByRole("button", { name: "Vérifier la clé Mistral maintenant" }),
    );
    expect(
      await screen.findByText(
        "Clé Mistral acceptée : alerte close, 2 jobs IA en attente repartent maintenant.",
      ),
    ).toBeInTheDocument();
    const posts = calls.filter((call) => call.path === "/api/admin/provider-alerts/mistral/check");
    expect(posts).toHaveLength(1);
    expect(posts[0]).toMatchObject({ method: "POST", body: null });
  });

  it("shows the Mistral key alert and requeues every failed analysis after confirmation", async () => {
    const answers = [
      { requeued: 500, remaining: 1 },
      { requeued: 1, remaining: 0 },
    ];
    const calls = adminRoutes({
      "GET /api/admin/dashboard": {
        ...dashboardFixture,
        alerts: [
          {
            code: "mistral_auth",
            since: "2026-10-19T06:00:00Z",
            last_seen_at: "2026-10-19T07:45:00Z",
            http_status: 401,
            failures: 3,
            message: "Clé Mistral refusée depuis le 19/10/2026 à 08:00 (HTTP 401).",
          },
        ],
      },
      "POST /api/admin/ideas/reanalyze-failed": () =>
        Response.json(answers.shift() ?? { requeued: 0, remaining: 0 }, { status: 202 }),
    });
    const { user } = renderRoute("/admin");
    expect(
      await screen.findByText("Clé Mistral refusée depuis le 19/10/2026 à 08:00 (HTTP 401)."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Relancer toutes les analyses échouées/ }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Relancer les analyses" }));
    expect(await screen.findByText("501 analyses remises en file.")).toBeInTheDocument();
    const posts = calls.filter((call) => call.path === "/api/admin/ideas/reanalyze-failed");
    expect(posts).toHaveLength(2);
    expect(posts[0]).toMatchObject({
      method: "POST",
      body: null,
      authorization: `Bearer ${TOKEN}`,
    });
  });
});
