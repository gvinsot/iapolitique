import { BotIcon, ClockIcon, LayersIcon, UsersIcon } from "lucide-react";
import { type ReactNode, useId, useMemo } from "react";
import { Link } from "react-router";

import type { IdeaDetail } from "@/api/types";
import { paths } from "@/app/paths";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import { aiAnalysisJsonLd } from "@/lib/ai-marking";
import { formatDate, formatInteger, formatMonths } from "@/lib/format";
import { labelOf } from "@/lib/taxonomy-labels";
import { cn } from "@/lib/utils";

import { AiGenerated } from "./ai-generated-notice";
import { AssumptionsList, ProsCons } from "./analysis-lists";
import { CitedText } from "./cited-text";
import { ContributionsList } from "./contributions-list";
import { GainRange } from "./gain-range";
import {
  AdminLevelBadge,
  CategoryBadge,
  ConfidenceBadge,
  FeasibilityBadge,
  KindBadge,
  LegalVehicleBadge,
  TerritoryBadge,
} from "./idea-badges";
import { ImpactGrid } from "./impact-grid";
import { OriginalContribution } from "./original-contribution";
import { RightOfReply } from "./right-of-reply";
import { ScoreBreakdown } from "./score-breakdown";
import { SourcesList } from "./sources-list";

const linkClass = "font-medium text-primary underline underline-offset-4 hover:no-underline";

function plural(count: number, one: string, many: string): string {
  return `${formatInteger(count)} ${count <= 1 ? one : many}`;
}

/**
 * Page of a published idea (GET /api/ideas/{slug}). Every generated block is preceded by the
 * AI notice and marked for machines (AGENTS.md §7.5); the visitor's own text, the rights of
 * reply and the provenance stay apart from it. Generated texts are rendered as plain text.
 */
export function IdeaDetailView({ idea, actions }: { idea: IdeaDetail; actions?: ReactNode }) {
  const sourceIds = useMemo(
    () => new Set(idea.analysis.sources.map((source) => source.id)),
    [idea.analysis.sources],
  );
  const jsonLd = useMemo(
    () =>
      aiAnalysisJsonLd({
        title: idea.title,
        url: idea.canonical_url,
        model: idea.ai.model,
        generatedAt: idea.ai.generated_at,
        analysisVersion: String(idea.ai.analysis_version),
      }),
    [idea.title, idea.canonical_url, idea.ai],
  );
  const hasReplies = idea.rights_of_reply.length > 0;

  return (
    <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_20rem]">
      <div className="min-w-0 space-y-8">
        <AiGenerated jsonLd={jsonLd} aria-label="Analyse générée par IA">
          <IdeaHeader idea={idea} />
          <KeyFigures idea={idea} />
          {hasReplies ? null : <AnalysisBody idea={idea} sourceIds={sourceIds} />}
        </AiGenerated>

        {hasReplies ? (
          <>
            <div className="space-y-4">
              {idea.rights_of_reply.map((reply, index) => (
                <RightOfReply key={`${reply.published_at}-${index}`} reply={reply} />
              ))}
            </div>
            <AiGenerated aria-label="Suite de l’analyse générée par IA">
              <AnalysisBody idea={idea} sourceIds={sourceIds} />
            </AiGenerated>
          </>
        ) : null}

        {actions ? (
          <section aria-labelledby="participer" className="space-y-4 border-t pt-8">
            <h2 id="participer" className="text-xl font-semibold">
              Participer
            </h2>
            <p className="text-pretty text-muted-foreground">
              Une amélioration, une objection argumentée ou une erreur à signaler ? Les textes
              envoyés sont filtrés automatiquement, puis pris en compte par une nouvelle analyse.
            </p>
            {actions}
          </section>
        ) : null}
      </div>

      <aside aria-label="Origine et provenance" className="space-y-6">
        <OriginalContribution text={idea.original.text} headingLevel={2} />
        <Provenance idea={idea} />
      </aside>
    </div>
  );
}

function IdeaHeader({ idea }: { idea: IdeaDetail }) {
  return (
    <header className="space-y-4">
      <div className="flex flex-wrap gap-2">
        <AdminLevelBadge level={idea.admin_level} />
        {idea.territory?.name ? <TerritoryBadge territory={idea.territory} /> : null}
        <CategoryBadge category={idea.category} />
        <KindBadge kind={idea.kind} />
        <LegalVehicleBadge vehicle={idea.analysis.legal_vehicle} />
      </div>
      <h1 className="text-3xl font-semibold text-pretty sm:text-4xl">{idea.title}</h1>
      <p className="text-lg text-pretty text-muted-foreground">{idea.summary}</p>
      <ul className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground">
        {idea.published_on ? <li>Publiée le {formatDate(idea.published_on)}</li> : null}
        <li>Analyse du {formatDate(idea.updated_at)}</li>
        <li className="flex items-center gap-1">
          <UsersIcon className="size-4" aria-hidden="true" />
          {idea.proposal_count <= 1
            ? "Proposée 1 fois"
            : `Proposée ${formatInteger(idea.proposal_count)} fois`}
        </li>
        <li>
          {plural(idea.contribution_count, "contribution intégrée", "contributions intégrées")}
        </li>
      </ul>
    </header>
  );
}

function KeyFigures({ idea }: { idea: IdeaDetail }) {
  const { analysis } = idea;
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <div className="rounded-lg border bg-card p-4">
        <GainRange range={analysis.annual_gain_eur} label="Gain brut annuel estimé" />
      </div>
      <div className="rounded-lg border bg-card p-4">
        <GainRange range={analysis.implementation_cost_eur} label="Coût de mise en œuvre" />
      </div>
      <div className="flex flex-wrap items-center gap-2 sm:col-span-2">
        <FeasibilityBadge score={analysis.feasibility.score} />
        <ConfidenceBadge confidence={analysis.confidence} />
        <span className="inline-flex items-center gap-1 text-sm text-muted-foreground">
          <ClockIcon className="size-4" aria-hidden="true" />
          Premiers effets : {formatMonths(analysis.time_to_effect_months)}
        </span>
      </div>
    </div>
  );
}

const TABS = [
  { value: "analyse", label: "Analyse" },
  { value: "chiffrage", label: "Chiffrage" },
  { value: "impacts", label: "Impacts" },
  { value: "contributions", label: "Contributions" },
] as const;

function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="space-y-3">
      <h3 id={id} className="text-lg font-semibold">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Paragraphs({ text, sourceIds }: { text: string; sourceIds: ReadonlySet<string> }) {
  const paragraphs = text.split(/\n+/).filter((paragraph) => paragraph.trim() !== "");
  if (paragraphs.length === 0) return <p className="text-muted-foreground">Non précisé.</p>;
  return (
    <div className="space-y-3">
      {paragraphs.map((paragraph, index) => (
        <p key={index} className="text-pretty">
          <CitedText text={paragraph} sourceIds={sourceIds} />
        </p>
      ))}
    </div>
  );
}

function AnalysisBody({ idea, sourceIds }: { idea: IdeaDetail; sourceIds: ReadonlySet<string> }) {
  const { analysis } = idea;
  const labels = useTaxonomyLabels();
  return (
    <div className="space-y-8">
      <section aria-labelledby="score-titre" className="space-y-4">
        <h2 id="score-titre" className="text-xl font-semibold">
          Score de rentabilité
        </h2>
        <ScoreBreakdown data={idea.score} />
      </section>

      <Tabs defaultValue="analyse">
        <TabsList className="w-full justify-start overflow-x-auto">
          {TABS.map((tab) => (
            <TabsTrigger key={tab.value} value={tab.value}>
              {tab.label}
              {tab.value === "contributions" && idea.contributions.length > 0
                ? ` (${idea.contributions.length})`
                : null}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="analyse" className="space-y-8 pt-6">
          <h2 className="sr-only">Analyse</h2>
          <Section title="Proposition améliorée">
            <Paragraphs text={analysis.improved_proposal} sourceIds={sourceIds} />
          </Section>
          <Section title="Situation actuelle">
            <Paragraphs text={analysis.existing_situation} sourceIds={sourceIds} />
          </Section>
          <Section title="Pour et contre">
            <ProsCons
              pros={analysis.pros}
              cons={analysis.cons}
              risks={analysis.risks}
              sourceIds={sourceIds}
              headingLevel={4}
            />
          </Section>
          <Section title="Mise en œuvre">
            <p>
              Texte nécessaire : {labelOf(labels.legalVehicles, analysis.legal_vehicle)}.
              Faisabilité {analysis.feasibility.score}/5.
            </p>
            {analysis.feasibility.obstacles.length > 0 ? (
              <ul className="list-disc space-y-1 pl-5">
                {analysis.feasibility.obstacles.map((obstacle, index) => (
                  <li key={index}>
                    <CitedText text={obstacle} sourceIds={sourceIds} />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">Aucun obstacle particulier n’est identifié.</p>
            )}
          </Section>
          {analysis.changes_since_previous.trim() ? (
            <Section title="Changements depuis l’analyse précédente">
              <Paragraphs text={analysis.changes_since_previous} sourceIds={sourceIds} />
            </Section>
          ) : null}
        </TabsContent>

        <TabsContent value="chiffrage" className="space-y-8 pt-6">
          <h2 className="sr-only">Chiffrage</h2>
          <Section title="Gain annuel">
            <GainRange
              range={analysis.annual_gain_eur}
              label="Gain brut annuel en régime de croisière"
            />
            <Paragraphs text={analysis.annual_gain_eur.calculation} sourceIds={sourceIds} />
          </Section>
          <Section title="Coût de mise en œuvre">
            <GainRange range={analysis.implementation_cost_eur} label="Coût ponctuel" />
            <Paragraphs text={analysis.implementation_cost_eur.calculation} sourceIds={sourceIds} />
          </Section>
          <Section title="Hypothèses de calcul">
            <AssumptionsList assumptions={analysis.assumptions} sourceIds={sourceIds} />
          </Section>
          <Section title="Qualité des données">
            <p>
              Confiance : {labelOf(labels.confidenceLevels, analysis.confidence).toLowerCase()}.
            </p>
            <Paragraphs text={analysis.confidence_explanation} sourceIds={sourceIds} />
          </Section>
        </TabsContent>

        <TabsContent value="impacts" className="space-y-4 pt-6">
          <h2 className="sr-only">Impacts</h2>
          <p className="text-sm text-muted-foreground">
            Effet attendu de la mesure pour chaque public, décrit sans référence à un camp
            politique.
          </p>
          <ImpactGrid impacts={analysis.impacts} sourceIds={sourceIds} />
        </TabsContent>

        <TabsContent value="contributions" className="space-y-4 pt-6">
          <h2 className="sr-only">Contributions</h2>
          <ContributionsList
            contributions={idea.contributions}
            totalCount={idea.contribution_count}
          />
        </TabsContent>
      </Tabs>

      <section aria-labelledby="sources-titre" className="space-y-4">
        <h2 id="sources-titre" className="text-xl font-semibold">
          Sources
        </h2>
        <SourcesList sources={analysis.sources} />
      </section>
    </div>
  );
}

function Provenance({ idea }: { idea: IdeaDetail }) {
  const { ai } = idea;
  const rows: [string, string][] = [
    ["Fournisseur", ai.provider],
    ["Modèle", ai.model],
    ["Généré le", formatDate(ai.generated_at)],
    ["Version de l’analyse", String(ai.analysis_version)],
    ["Prompt d’analyse", ai.prompt_version],
    ...(ai.research_prompt_version
      ? ([["Prompt de recherche", ai.research_prompt_version]] as [string, string][])
      : []),
    ["Schéma de sortie", ai.schema_version],
    ["Formule de score", ai.scoring_version],
    ["Recherches web", formatInteger(ai.web_search_count)],
    ["Relecture humaine", ai.human_reviewed ? "oui" : "non"],
  ];
  return (
    <section
      aria-labelledby="provenance"
      className={cn("space-y-3 rounded-lg border bg-card p-4 text-sm")}
    >
      <h2 id="provenance" className="flex items-center gap-2 text-base font-semibold">
        <BotIcon className="size-4" aria-hidden="true" />
        Provenance de l’analyse
      </h2>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5">
        {rows.map(([term, value]) => (
          <div key={term} className="contents">
            <dt className="text-muted-foreground">{term}</dt>
            <dd className="font-medium break-words">{value}</dd>
          </div>
        ))}
      </dl>
      <p className="flex items-start gap-2 text-muted-foreground">
        <LayersIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <span>
          Les prompts et la formule de chaque version sont publiés sur la page{" "}
          <Link to={paths.methodology} className={linkClass}>
            Méthodologie
          </Link>
          .
        </span>
      </p>
    </section>
  );
}
