import { FileTextIcon, SlidersHorizontalIcon } from "lucide-react";
import { Link } from "react-router";

import type {
  Methodology,
  MethodologyFile,
  MethodologyModel,
  ScreeningCriterion,
  ScreeningSection,
} from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import { formatFactor, formatPercent } from "@/lib/format";
import { labelOf } from "@/lib/taxonomy-labels";

const linkClass = "font-medium text-primary underline underline-offset-4 hover:no-underline";

const SECTION_TITLES: Record<ScreeningSection["kind"], string> = {
  idea: "Idées",
  contribution: "Améliorations et objections",
  report: "Signalements",
};

const FILE_KINDS: Record<MethodologyFile["kind"], string> = {
  screening: "Grille de filtrage",
  taxonomy: "Taxonomie",
  prompt: "Prompt",
  schema: "Schéma de sortie",
};

const PROMPT_NAMES: Record<string, string> = {
  analysis_research: "Analyse, étape 1 : recherche web",
  analysis: "Analyse, étape 2 : structuration",
  second_opinion: "Second avis (zone grise)",
  report_review: "Revue des signalements",
};

/** The reasoning effort sent to Mistral (AGENTS.md §6.1), in words. */
function reasoningLabel(effort: MethodologyModel["reasoning_effort"]): string {
  switch (effort) {
    case "high":
      return "réflexion complète avant la réponse (high)";
    case "none":
      return "réflexion minimale (none)";
    default:
      return "réglage par défaut de Mistral";
  }
}

/** What makes a criterion pass, in words. */
function passCondition(criterion: ScreeningCriterion): string {
  if (criterion.question_type === "score") {
    const level = criterion.levels[criterion.pass_from_level ?? 0];
    return level ? `niveau « ${level} » ou supérieur` : "niveau suffisant";
  }
  const answer = criterion.pass_when ? criterion.answers?.yes : criterion.answers?.no;
  const fallback = criterion.pass_when ? "oui" : "non";
  return `réponse « ${answer ?? fallback} »`;
}

function CriterionRow({ criterion, index }: { criterion: ScreeningCriterion; index: number }) {
  return (
    <li className="space-y-3 rounded-lg border bg-card p-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-sm text-muted-foreground">{index + 1}.</span>
        <code className="rounded bg-muted px-1.5 py-0.5 text-sm">{criterion.key}</code>
        <Badge variant="outline">
          {criterion.question_type === "score" ? "Échelle de niveaux" : "Question oui / non"}
        </Badge>
        {criterion.thresholds_adjusted ? (
          <Badge variant="secondary">
            <SlidersHorizontalIcon aria-hidden="true" />
            Seuils ajustés par l’éditeur
          </Badge>
        ) : null}
      </div>
      <p className="text-pretty whitespace-pre-line">{criterion.instructions}</p>
      {criterion.levels.length > 0 ? (
        <p className="text-sm text-muted-foreground">
          Niveaux, du plus bas au plus haut : {criterion.levels.join(" · ")}.
        </p>
      ) : null}
      <dl className="grid gap-x-4 gap-y-1 text-sm sm:grid-cols-[12rem_minmax(0,1fr)]">
        <dt className="text-muted-foreground">Critère réussi si</dt>
        <dd>{passCondition(criterion)}</dd>
        <dt className="text-muted-foreground">Refus en dessous de</dt>
        <dd className="tabular-nums">{formatPercent(criterion.reject_below)} de probabilité</dd>
        <dt className="text-muted-foreground">Acceptation à partir de</dt>
        <dd className="tabular-nums">{formatPercent(criterion.accept_above)} de probabilité</dd>
        <dt className="text-muted-foreground">Motif donné en cas de refus</dt>
        <dd className="text-pretty">{criterion.reason_message}</dd>
        {criterion.rule ? (
          <>
            <dt className="text-muted-foreground">Règle</dt>
            <dd>
              <Link to={criterion.rule.url} className={linkClass}>
                Article {criterion.rule.article} : {criterion.rule.title}
              </Link>
            </dd>
          </>
        ) : null}
      </dl>
    </li>
  );
}

/**
 * Prompts, screening grid, taxonomy and score formula exactly as used in production
 * (GET /api/methodology, AGENTS.md §6.5). Every text is rendered as plain text.
 */
export function MethodologyInForce({ methodology }: { methodology: Methodology }) {
  const labels = useTaxonomyLabels();
  const { models, scoring, screening } = methodology;
  const modelRows: [string, MethodologyModel | string][] = [
    ["Analyse (recherche web, puis structuration)", models.analysis],
    ["Second avis et revue des signalements", models.review],
    ["Détection des doublons (embeddings)", models.embedding],
  ];
  const versions: [string, string][] = [
    ["Grille de filtrage (screening.yaml)", methodology.screening_version],
    ["Taxonomie (taxonomy.yaml)", methodology.taxonomy_version],
    ...Object.entries(methodology.prompt_versions).map(([name, version]): [string, string] => [
      PROMPT_NAMES[name] ?? name,
      version,
    ]),
    ["Schéma de sortie de l’analyse", methodology.analysis_schema_version],
    ["Formule de score", scoring.version],
    ["Format des exports", methodology.export_schema_version],
    ["Règles du site (CGU)", methodology.cgu_version],
  ];

  return (
    <div className="space-y-10">
      <section aria-labelledby="versions-en-vigueur" className="space-y-3">
        <h3 id="versions-en-vigueur">Versions en vigueur</h3>
        <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-[minmax(0,1fr)_auto]">
          {versions.map(([name, version]) => (
            <div key={name} className="contents">
              <dt className="text-muted-foreground">{name}</dt>
              <dd className="font-mono break-all">{version}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section aria-labelledby="modeles-en-vigueur" className="space-y-3">
        <h3 id="modeles-en-vigueur">Modèles d’IA en vigueur (Mistral AI)</h3>
        <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-[minmax(0,1fr)_auto]">
          {modelRows.map(([usage, model]) => (
            <div key={usage} className="contents">
              <dt className="text-muted-foreground">{usage}</dt>
              <dd>
                {typeof model === "string" ? (
                  <code className="break-all">{model}</code>
                ) : (
                  <>
                    <code className="break-all">{model.model}</code> · effort de raisonnement :{" "}
                    {reasoningLabel(model.reasoning_effort)}
                    {model.configured_reasoning_effort &&
                    model.configured_reasoning_effort !== model.reasoning_effort ? (
                      <>
                        {" "}
                        (effort configuré « {model.configured_reasoning_effort} », refusé par ce
                        modèle : le paramètre n’est pas envoyé)
                      </>
                    ) : null}
                  </>
                )}
              </dd>
            </div>
          ))}
        </dl>
      </section>

      <section aria-labelledby="formule" className="space-y-3">
        <h3 id="formule">Formule du score (version {scoring.version})</h3>
        <pre className="overflow-x-auto rounded-lg border bg-muted/50 p-4 text-sm whitespace-pre">
          {scoring.formula}
        </pre>
        <ul>
          <li>Coût de mise en œuvre amorti sur {scoring.amortization_years} ans.</li>
          <li>Faisabilité notée de 1 à {scoring.max_feasibility}.</li>
          <li>
            Le facteur de délai est divisé par deux pour un délai de{" "}
            {scoring.delay_half_life_months} mois.
          </li>
          <li>
            Facteur de confiance :{" "}
            {scoring.confidence_factors
              .map(
                (item) =>
                  `${labelOf(labels.confidenceLevels, item.level).toLowerCase()} ${formatFactor(item.factor)}`,
              )
              .join(", ")}
            .
          </li>
        </ul>
      </section>

      <section aria-labelledby="grille" className="space-y-4">
        <h3 id="grille">Grille de filtrage (version {screening.version})</h3>
        <p>
          Les critères sont évalués dans cet ordre ; le motif d’un refus est celui du premier
          critère en échec. Les seuils portent sur la probabilité de réussir le critère.
        </p>
        {screening.sections.map((section) => (
          <section
            key={section.kind}
            aria-labelledby={`grille-${section.kind}`}
            className="space-y-3"
          >
            <h4 id={`grille-${section.kind}`} className="text-base font-semibold">
              {SECTION_TITLES[section.kind]}
            </h4>
            <ol className="list-none space-y-3 pl-0">
              {section.criteria.map((criterion, index) => (
                <CriterionRow key={criterion.key} criterion={criterion} index={index} />
              ))}
            </ol>
          </section>
        ))}
      </section>

      <section aria-labelledby="fichiers" className="space-y-3">
        <h3 id="fichiers">Textes intégraux</h3>
        <p>
          Fichiers du dossier <code>prompts/</code> tels qu’ils sont chargés par le service, sans
          modification.
        </p>
        <ul className="list-none space-y-2 pl-0">
          {methodology.files.map((file) => (
            <li key={file.file}>
              <details className="group rounded-lg border bg-card">
                <summary className="flex cursor-pointer flex-wrap items-center gap-2 rounded-lg p-3 font-medium">
                  <FileTextIcon className="size-4 text-muted-foreground" aria-hidden="true" />
                  <code>{file.file}</code>
                  <span className="text-sm font-normal text-muted-foreground">
                    {FILE_KINDS[file.kind]}
                    {file.version ? ` · version ${file.version}` : null}
                  </span>
                </summary>
                <pre className="max-h-[32rem] overflow-auto border-t p-4 text-xs leading-relaxed whitespace-pre-wrap">
                  {file.content}
                </pre>
              </details>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
