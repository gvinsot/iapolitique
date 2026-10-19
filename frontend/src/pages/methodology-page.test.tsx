import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { apiRoutes } from "@/test/api-routes";
import { stubFetch } from "@/test/fetch-mock";
import { methodologyFixture } from "@/test/fixtures/reference";
import { renderRoute } from "@/test/render-route";

describe("MethodologyPage", () => {
  it("shows the versions, formula, grid and files in force, as plain text", async () => {
    stubFetch(apiRoutes({ "GET /api/methodology": methodologyFixture }));
    const { container } = renderRoute("/methodologie");

    const versions = await screen.findByRole("region", { name: "Versions en vigueur" });
    expect(versions).toHaveTextContent("sha256:0123456789ab");
    expect(versions).toHaveTextContent("Analyse, étape 1 : recherche web");

    const models = screen.getByRole("region", { name: "Modèles d’IA en vigueur (Mistral AI)" });
    expect(models).toHaveTextContent(
      "mistral-large-latest · effort de raisonnement : réflexion complète avant la réponse (high)",
    );
    expect(models).toHaveTextContent(
      "mistral-medium-3-5 · effort de raisonnement : réglage par défaut de Mistral",
    );
    expect(models).toHaveTextContent("mistral-embed");

    expect(screen.getByRole("heading", { name: "Formule du score (version 1)" })).toBeVisible();
    expect(container.querySelector("pre")).toHaveTextContent("score = gain net annuel");

    const ideas = screen.getByRole("region", { name: "Idées" });
    const criteria = within(ideas).getAllByRole("listitem");
    expect(criteria[0]).toHaveTextContent("on_topic");
    expect(criteria[0]).toHaveTextContent("Seuils ajustés par l’éditeur");
    expect(criteria[0]).toHaveTextContent("Refus en dessous de30 % de probabilité");
    expect(within(criteria[0]!).getByRole("link", { name: /Article 4.5/ })).toHaveAttribute(
      "href",
      "/regles#critere-sujet",
    );
    expect(criteria[1]).toHaveTextContent("niveau « difficile » ou supérieur");

    // Prompt texts are shown verbatim, never interpreted as HTML.
    expect(screen.getByText(/<script>alert\(1\)<\/script>/)).toBeInTheDocument();
    expect(container.querySelector("script:not([type])")).toBeNull();
  });

  it("says when a model refused the configured reasoning effort", async () => {
    stubFetch(
      apiRoutes({
        "GET /api/methodology": {
          ...methodologyFixture,
          models: {
            ...methodologyFixture.models,
            analysis: {
              model: "mistral-large-latest",
              reasoning_effort: null,
              configured_reasoning_effort: "high",
            },
          },
        },
      }),
    );
    renderRoute("/methodologie");
    const models = await screen.findByRole("region", {
      name: "Modèles d’IA en vigueur (Mistral AI)",
    });
    expect(models).toHaveTextContent(
      "mistral-large-latest · effort de raisonnement : réglage par défaut de Mistral (effort " +
        "configuré « high », refusé par ce modèle : le paramètre n’est pas envoyé)",
    );
    // An effort omitted by configuration is not a refusal.
    expect(models).toHaveTextContent(
      /mistral-medium-3-5 · effort de raisonnement : réglage par défaut de Mistral(?! \()/,
    );
  });

  it("explains in French when the texts cannot be loaded", async () => {
    stubFetch(
      apiRoutes({
        "GET /api/methodology": Response.json(
          { error: { code: "internal", message: "Le service rencontre un problème." } },
          { status: 500 },
        ),
      }),
    );
    renderRoute("/methodologie");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Les textes en vigueur n’ont pas pu être chargés",
    );
  });
});
