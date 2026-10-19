import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { altchaChallenge } from "@/test/altcha";
import { audit } from "@/test/axe";
import { apiError, apiRoutes } from "@/test/api-routes";
import { stubFetch } from "@/test/fetch-mock";
import { ideaDetailFixture } from "@/test/fixtures/ideas";
import { acceptedIdeaResultFixture, TRACKING_TOKEN } from "@/test/fixtures/tracking";
import { renderRoute } from "@/test/render-route";

const SLUG = ideaDetailFixture.slug;
const PATH = `/idees/${SLUG}`;

function detailRoutes(extra: Record<string, unknown> = {}) {
  return apiRoutes({ [`GET /api/ideas/${SLUG}`]: ideaDetailFixture, ...extra });
}

describe("IdeaPage", () => {
  it("shows the analysis behind the AI notice, apart from the original text", async () => {
    stubFetch(detailRoutes());
    renderRoute(PATH);

    expect(
      await screen.findByRole("heading", { level: 1, name: ideaDetailFixture.title }),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(document.title).toBe(`${ideaDetailFixture.title} · iapolitique`);
    });

    const generated = screen.getByRole("region", { name: "Suite de l’analyse générée par IA" });
    expect(generated).toHaveAttribute("data-ai-generated", "true");
    const header = screen.getByRole("region", { name: "Analyse générée par IA" });
    expect(within(header).getByRole("note")).toHaveTextContent(
      "Analyse générée automatiquement par intelligence artificielle (Mistral AI), sans relecture humaine.",
    );

    const original = screen.getByRole("region", { name: /Contribution d’un internaute/ });
    expect(original).not.toHaveAttribute("data-ai-generated");
    expect(original.closest("[data-ai-generated]")).toBeNull();
    expect(original).toHaveTextContent("[donnée masquée]");

    // The right of reply sits between the two generated blocks, never inside them.
    const reply = screen.getByRole("region", { name: "Droit de réponse" });
    expect(reply.closest("[data-ai-generated]")).toBeNull();

    expect(screen.getByRole("region", { name: "Provenance de l’analyse" })).toHaveTextContent(
      "mistral-large-latest",
    );
    expect(screen.getByText("Score calculé par algorithme", { exact: false })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Sources" })).toBeInTheDocument();
  });

  it("has no detectable accessibility violation", async () => {
    stubFetch(detailRoutes());
    renderRoute(PATH);
    await screen.findByRole("heading", { level: 1, name: ideaDetailFixture.title });
    expect(await audit()).toEqual([]);
  });

  it("marks the analysis for machines with escaped JSON-LD and a canonical link", async () => {
    stubFetch(detailRoutes());
    const { container } = renderRoute(PATH);
    await screen.findByRole("heading", { level: 1, name: ideaDetailFixture.title });

    const script = container.querySelector('script[type="application/ld+json"]');
    const data = JSON.parse(script?.textContent ?? "{}") as Record<string, unknown>;
    expect(data["@type"]).toBe("CreativeWork");
    expect(data.url).toBe(ideaDetailFixture.canonical_url);
    expect(document.head.querySelector('link[rel="canonical"]')).toHaveAttribute(
      "href",
      ideaDetailFixture.canonical_url,
    );
    expect(document.head.querySelector('meta[name="description"]')).toHaveAttribute(
      "content",
      ideaDetailFixture.summary,
    );
  });

  it("lists integrated contributions and the calculation in their tabs", async () => {
    stubFetch(detailRoutes());
    const { user } = renderRoute(PATH);
    await user.click(await screen.findByRole("tab", { name: "Contributions (2)" }));
    expect(screen.getByText("Ouvrir la centrale aux établissements publics locaux.")).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "Chiffrage" }));
    expect(screen.getByRole("heading", { name: "Hypothèses de calcul" })).toBeVisible();
  });

  it("answers 404 for an unknown, hidden or purged idea", async () => {
    stubFetch(
      apiRoutes({
        "GET /api/ideas/inconnue": apiError(404, "idea_not_found", "Cette idée n’existe pas."),
      }),
    );
    renderRoute("/idees/inconnue");
    expect(
      await screen.findByRole("heading", { level: 1, name: "Idée introuvable" }),
    ).toBeInTheDocument();
    expect(document.title).toBe("Idée introuvable · iapolitique");
  });

  it("does not call the API for a malformed slug", async () => {
    const fetch = stubFetch(apiRoutes({}));
    renderRoute("/idees/Pas%20un%20slug");
    expect(
      await screen.findByRole("heading", { level: 1, name: "Idée introuvable" }),
    ).toBeInTheDocument();
    expect(
      fetch.mock.calls.some(([input]) =>
        new URL((input as Request).url).pathname.startsWith("/api/ideas/"),
      ),
    ).toBe(false);
  });

  it("moves a merged duplicate to the address of the idea it joined", async () => {
    // fetch follows the 301 of GET /api/ideas/{merged}: the body is the parent's page.
    stubFetch(apiRoutes({ "GET /api/ideas/ancienne-idee": ideaDetailFixture }));
    const { router } = renderRoute("/idees/ancienne-idee");
    await screen.findByRole("heading", { level: 1, name: ideaDetailFixture.title });
    await waitFor(() => {
      expect(router.state.location.pathname).toBe(PATH);
    });
  });

  it("explains a network failure in French", async () => {
    stubFetch(() => Promise.reject(new TypeError("Failed to fetch")));
    renderRoute(PATH);
    expect(await screen.findByRole("alert", {}, { timeout: 15_000 })).toHaveTextContent(
      "Impossible de joindre le serveur",
    );
  });

  it("sends a contribution and shows its tracking link", async () => {
    const bodies: unknown[] = [];
    stubFetch(
      detailRoutes({
        "GET /api/altcha/challenge": () => altchaChallenge(),
        [`POST /api/ideas/${SLUG}/contributions`]: async (request: Request) => {
          bodies.push(await request.json());
          return Response.json(
            { ...acceptedIdeaResultFixture, status: "accepted" },
            { status: 201 },
          );
        },
      }),
    );
    const { user } = renderRoute(PATH);
    await user.click(
      await screen.findByRole("button", { name: "Proposer une amélioration ou une objection" }),
    );
    await screen.findByText(/Je ne suis pas un robot|Chargement de la vérification/);
    await user.click(screen.getByRole("radio", { name: /Amélioration/ }));
    await user.type(
      screen.getByRole("textbox", { name: "Votre contribution" }),
      "Ouvrir aussi la centrale aux syndicats intercommunaux du département.",
    );
    await user.click(screen.getByRole("checkbox", { name: /J’accepte la licence/ }));
    await user.click(screen.getByRole("checkbox", { name: /sera publié s’il est accepté/ }));
    await user.click(screen.getByRole("button", { name: "Envoyer ma contribution" }));

    expect(
      await screen.findByRole("heading", { name: "Merci, c’est enregistré" }),
    ).toBeInTheDocument();
    expect(screen.getByText(new RegExp(`suivi#${TRACKING_TOKEN}`))).toBeInTheDocument();
    expect(bodies).toEqual([
      expect.objectContaining({ kind: "amelioration", accept_licence: true }),
    ]);
  });

  it("keeps the report form and shows the API message when a report is refused", async () => {
    stubFetch(
      detailRoutes({
        "GET /api/altcha/challenge": () => altchaChallenge(),
        [`POST /api/ideas/${SLUG}/reports`]: apiError(
          429,
          "rate_limited",
          "Trop de signalements envoyés aujourd’hui.",
        ),
      }),
    );
    const { user } = renderRoute(PATH);
    await user.click(await screen.findByRole("button", { name: "Signaler un problème" }));
    await screen.findByText(/Je ne suis pas un robot|Chargement de la vérification/);
    await user.click(screen.getByRole("radio", { name: "Erreur de chiffrage" }));
    await user.click(screen.getByRole("checkbox", { name: /de bonne foi/ }));
    await user.click(screen.getByRole("button", { name: "Envoyer le signalement" }));

    expect(await screen.findByText("Votre signalement n’a pas pu être envoyé")).toBeInTheDocument();
    expect(screen.getByText("Trop de signalements envoyés aujourd’hui.")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Erreur de chiffrage" })).toBeChecked();
  });
});
