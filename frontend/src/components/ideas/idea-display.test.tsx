import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { aiAnalysisJsonLd } from "@/lib/ai-marking";
import {
  analysisFixture,
  contributionsFixture,
  ideaSummaryFixture,
  impactsFixture,
  rightOfReplyFixture,
  scoreBreakdownFixture,
} from "@/test/fixtures/ideas";
import { apiRoutes } from "@/test/api-routes";
import { stubFetch } from "@/test/fetch-mock";
import { taxonomyFixture } from "@/test/fixtures/reference";
import { renderComponent } from "@/test/render-component";

import { AiGenerated, AiGeneratedNotice } from "./ai-generated-notice";
import { AssumptionsList, ProsCons } from "./analysis-lists";
import { ContributionsList } from "./contributions-list";
import { GainRange } from "./gain-range";
import { FeasibilityBadge, LegalVehicleBadge } from "./idea-badges";
import { IdeaCard } from "./idea-card";
import { ImpactGrid } from "./impact-grid";
import { OriginalContribution } from "./original-contribution";
import { RightOfReply } from "./right-of-reply";
import { ScoreBreakdown } from "./score-breakdown";
import { SourcesList } from "./sources-list";

const plain = (value: string | null) => (value ?? "").replace(/\s/g, " ");
const sourceIds = new Set(analysisFixture.sources.map((source) => source.id));

describe("AI transparency", () => {
  it("shows the exact AI Act wording as an accessible note", () => {
    renderComponent(<AiGeneratedNotice />);
    const note = screen.getByRole("note");
    expect(note).toHaveTextContent(
      "Analyse générée automatiquement par intelligence artificielle (Mistral AI), sans relecture humaine. Elle peut contenir des erreurs ; vérifiez les sources citées. Ce n’est pas une position officielle.",
    );
    expect(note).toHaveAccessibleName(/Analyse générée automatiquement/);
  });

  it("marks generated blocks for machines and puts the notice before the content", () => {
    const jsonLd = aiAnalysisJsonLd({
      title: "Titre </script><script>alert(1)</script>",
      url: "https://iapolitique.fr/idees/exemple",
      model: "mistral-large-latest",
      generatedAt: "2026-10-12T08:30:00Z",
      analysisVersion: "3",
    });
    const { container } = renderComponent(
      <AiGenerated jsonLd={jsonLd} aria-label="Analyse">
        <p>Contenu généré</p>
      </AiGenerated>,
    );
    const block = container.querySelector('[data-ai-generated="true"]')!;
    expect(block.firstElementChild).toHaveAttribute("role", "note");
    const script = block.querySelector('script[type="application/ld+json"]')!;
    expect(script.textContent).not.toContain("</script>");
    const parsed = JSON.parse(script.textContent) as Record<string, unknown>;
    expect(parsed).toMatchObject({
      "@type": "CreativeWork",
      name: "Titre </script><script>alert(1)</script>",
      digitalSourceType: "https://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
      creator: { name: "mistral-large-latest", provider: { name: "Mistral AI" } },
    });
    expect(parsed.additionalProperty).toContainEqual({
      "@type": "PropertyValue",
      name: "human_reviewed",
      value: false,
    });
  });

  it("separates the visitor's original text, keeping its line breaks as plain text", () => {
    renderComponent(
      <OriginalContribution text={"Première ligne\n<b>pas du HTML</b>"} submittedOn="2026-10-01" />,
    );
    const region = screen.getByRole("region", {
      name: "Contribution d’un internaute (texte original, données personnelles masquées)",
    });
    expect(region).toHaveAttribute("data-source-type", "user");
    expect(region).not.toHaveAttribute("data-ai-generated");
    expect(within(region).getByText(/pas du HTML/).innerHTML).toContain("&lt;b&gt;");
    expect(region).toHaveTextContent(
      "Contribution anonyme – iapolitique.fr, proposée le 1 octobre 2026.",
    );
  });
});

describe("IdeaCard", () => {
  it("labels its badges with the taxonomy served by the API", async () => {
    stubFetch(apiRoutes({ "GET /api/taxonomy": taxonomyFixture }));
    renderComponent(<IdeaCard idea={ideaSummaryFixture} />);
    expect(await screen.findByText("Commune (libellé de la taxonomie)")).toBeInTheDocument();
    expect(screen.getByText("Économie de dépense")).toBeInTheDocument();
  });

  it("summarises an idea with its badges, estimate, score and proposal count", () => {
    const { container } = renderComponent(<IdeaCard idea={ideaSummaryFixture} rank={2} />);
    const card = container.querySelector("article")!;
    expect(card).toHaveAttribute("data-ai-generated", "true");
    expect(screen.getByRole("heading", { level: 3 })).toHaveTextContent(
      "2.Mutualiser les achats de fournitures des communes d’un même département",
    );
    expect(screen.getByRole("link")).toHaveAttribute(
      "href",
      "/idees/mutualiser-achats-fournitures-communes",
    );
    for (const text of ["Commune", "Gironde", "Achats publics", "Baisse de dépense"]) {
      expect(within(card).getByText(text)).toBeInTheDocument();
    }
    expect(plain(card.textContent)).toContain("Gain annuel estimé : 5 M € (entre 2 M € et 9 M €)");
    expect(card).toHaveTextContent("Faisabilité 4/5 · Plutôt facile");
    expect(card).toHaveTextContent("Confiance moyenne");
    expect(card).toHaveTextContent("Premiers effets : 1 an");
    expect(plain(card.textContent)).toContain("Score calculé par algorithme :410/1000");
    expect(card.textContent).not.toContain("1,92 M");
    expect(card).toHaveTextContent("Proposée 3 fois");
    expect(card).toHaveTextContent(/générés par IA/);
  });

  it("omits an empty territory", () => {
    renderComponent(
      <IdeaCard idea={{ ...ideaSummaryFixture, territory: null, proposal_count: 1 }} />,
    );
    expect(screen.queryByText("Gironde")).not.toBeInTheDocument();
    expect(screen.getByText("Proposée 1 fois")).toBeInTheDocument();
  });
});

describe("estimates and score", () => {
  it("gives the low, median and high values in text", () => {
    renderComponent(
      <GainRange range={analysisFixture.annual_gain_eur} label="Gain annuel estimé" />,
    );
    expect(plain(screen.getByText(/^Fourchette/).textContent)).toBe(
      "Fourchette : 5 M € (entre 2 M € et 9 M €)",
    );
  });

  it("lists the four score components with a link to their explanation", () => {
    renderComponent(<ScoreBreakdown data={scoreBreakdownFixture} />);
    for (const label of [
      "Gain net annuel",
      "Faisabilité",
      "Confiance des données",
      "Délai avant effet",
    ]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("× 0,8")).toBeInTheDocument();
    expect(screen.getByText("× 0,67")).toBeInTheDocument();
    expect(screen.getByText(/Score calculé par algorithme/)).toBeInTheDocument();
    // Points on 1000, read in full by screen readers; the raw score stays visible.
    expect(screen.getByText("410/1000")).toBeInTheDocument();
    expect(screen.getByText("410 points sur 1000")).toHaveClass("sr-only");
    expect(screen.getByText(/Score brut \(produit des composantes\) : 1,92 M/)).toBeInTheDocument();
  });

  it("never shows NaN when an older api sends no points", () => {
    const { points: _points, ...withoutPoints } = scoreBreakdownFixture;
    renderComponent(
      <ScoreBreakdown data={withoutPoints as unknown as typeof scoreBreakdownFixture} />,
    );
    expect(screen.queryByText(/NaN/)).not.toBeInTheDocument();
    expect(screen.getByText("score indisponible")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Comment ce score est-il calculé ?" })).toHaveAttribute(
      "href",
      "/regles#score",
    );
    expect(screen.getByText(/ni un sondage, ni un vote/)).toBeInTheDocument();
  });

  it("describes feasibility and legal vehicle in words", () => {
    renderComponent(
      <>
        <FeasibilityBadge score={2} />
        <LegalVehicleBadge vehicle="loi_de_finances" />
      </>,
    );
    expect(screen.getByText(/Faisabilité 2\/5 · Difficile/)).toBeInTheDocument();
    expect(
      screen.getByText("Loi de finances ou de financement de la sécurité sociale"),
    ).toBeInTheDocument();
  });
});

describe("analysis lists", () => {
  it("shows every impact public with an effect in words", () => {
    renderComponent(<ImpactGrid impacts={impactsFixture} sourceIds={sourceIds} />);
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(9);
    expect(items[3]).toHaveTextContent("EntreprisesEffet : Négatif");
    expect(items[5]).toHaveTextContent("Effet : Positif");
    expect(items[4]).toHaveTextContent("Effet : Incertain");
  });

  it("links [Sx] references to the sources list and keeps unknown ones as text", () => {
    renderComponent(
      <>
        <ProsCons
          pros={["Moins cher [S1]", "Inconnu [S9]"]}
          cons={["Moins de choix [S2]"]}
          risks={analysisFixture.risks}
          sourceIds={sourceIds}
        />
        <SourcesList sources={analysisFixture.sources} />
      </>,
    );
    expect(screen.getByRole("heading", { name: "Avantages" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Inconvénients" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Risques" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Source 1" })).toHaveAttribute("href", "#source-S1");
    expect(screen.getByText(/Inconnu \[S9\]/)).toBeInTheDocument();
    expect(document.getElementById("source-S1")).toHaveTextContent("Cour des comptes");
  });

  it("never renders a non-http source URL as a link", () => {
    renderComponent(<SourcesList sources={analysisFixture.sources} />);
    const link = screen.getByRole("link", { name: /Rapport sur la commande publique locale/ });
    expect(link).toHaveAttribute("href", "https://www.ccomptes.fr/fr/publications/exemple");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer nofollow ugc");
    expect(screen.queryByRole("link", { name: "Les groupements de commandes" })).toBeNull();
    expect(screen.getByText("Les groupements de commandes")).toBeInTheDocument();
  });

  it("numbers the assumptions", () => {
    renderComponent(
      <AssumptionsList assumptions={analysisFixture.assumptions} sourceIds={sourceIds} />,
    );
    expect(screen.getByRole("list").tagName).toBe("OL");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
  });

  it("shows neutral contribution summaries only", () => {
    renderComponent(<ContributionsList contributions={contributionsFixture} totalCount={5} />);
    expect(screen.getByText(/5 contributions reçues/)).toBeInTheDocument();
    expect(screen.getByText("Prise en compte")).toBeInTheDocument();
    expect(screen.getByText("Non retenue")).toBeInTheDocument();
    expect(screen.getByText("Objection")).toBeInTheDocument();
    expect(screen.getByText(/Intégré dans la proposition améliorée/)).toBeInTheDocument();
  });

  it("shows the right of reply verbatim with its legal basis", () => {
    renderComponent(<RightOfReply reply={rightOfReplyFixture} />);
    const region = screen.getByRole("region", { name: "Droit de réponse" });
    expect(region).toHaveTextContent("publiée le 15 octobre 2026");
    expect(region).toHaveTextContent("article 1-1, III");
    expect(region).not.toHaveAttribute("data-ai-generated");
  });
});
