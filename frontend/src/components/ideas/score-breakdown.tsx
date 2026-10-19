import { CalculatorIcon } from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { formatEurosCompact, formatFactor, formatPoints, formatScore } from "@/lib/format";
import type { ScoreBreakdownData } from "@/lib/idea-model";
import { RULES_SCORE_ANCHOR } from "@/lib/rules";
import { cn } from "@/lib/utils";

export const SCORE_LABEL = "Score calculé par algorithme";

/** « 429/1000 », read « 429 points sur 1000 » by screen readers. */
export function ScorePoints({
  points,
  max = 1000,
  className,
}: {
  points: number;
  max?: number;
  className?: string;
}) {
  return (
    <strong className={cn("tabular-nums", className)}>
      <span aria-hidden="true">{formatPoints(points, max)}</span>
      <span className="sr-only">
        {points} points sur {max}
      </span>
    </strong>
  );
}

interface Component {
  label: string;
  value: string;
  explanation: string;
}

function components(data: ScoreBreakdownData): Component[] {
  return [
    {
      label: "Gain net annuel",
      value: formatEurosCompact(data.net_annual_eur),
      explanation: "Gain annuel médian moins le coût de mise en œuvre amorti sur 5 ans",
    },
    {
      label: "Faisabilité",
      value: `× ${formatFactor(data.feasibility_factor)}`,
      explanation: "Note de faisabilité divisée par 5",
    },
    {
      label: "Confiance des données",
      value: `× ${formatFactor(data.confidence_factor)}`,
      explanation: "0,5 si faible, 0,75 si moyenne, 1 si élevée",
    },
    {
      label: "Délai avant effet",
      value: `× ${formatFactor(data.delay_factor)}`,
      explanation: "1 / (1 + délai en mois / 24)",
    },
  ];
}

/**
 * The four components of the composite score (AGENTS.md §6.3) and their product. Always
 * presented as an algorithmic score, never as a poll, with a link to its explanation.
 */
export function ScoreBreakdown({
  data,
  className,
}: {
  data: ScoreBreakdownData;
  className?: string;
}) {
  return (
    <div className={cn("space-y-4", className)}>
      <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {components(data).map((item) => (
          <li key={item.label} className="space-y-1 rounded-lg border bg-card p-4">
            <p className="text-sm font-medium text-muted-foreground">{item.label}</p>
            <p className="text-xl font-semibold tabular-nums">{item.value}</p>
            <p className="text-xs text-muted-foreground">{item.explanation}</p>
          </li>
        ))}
      </ul>
      <div className="flex flex-col gap-2 rounded-lg border border-primary/30 bg-accent p-4 text-accent-foreground sm:flex-row sm:items-center sm:justify-between">
        <p className="flex items-center gap-2">
          <CalculatorIcon className="size-4 shrink-0" aria-hidden="true" />
          <span>
            {SCORE_LABEL} :{" "}
            <ScorePoints points={data.points} max={data.max_points} className="text-lg" />
          </span>
        </p>
        <p className="text-sm">
          Formule version {data.scoring_version} ·{" "}
          <Link
            to={`${paths.terms}#${RULES_SCORE_ANCHOR}`}
            className="font-medium underline underline-offset-4 hover:no-underline"
          >
            Comment ce score est-il calculé ?
          </Link>
        </p>
      </div>
      <p className="text-xs text-muted-foreground">
        Score brut (produit des composantes) : {formatScore(data.score)}. Il est ramené sur{" "}
        {data.max_points} points par une échelle fixe : 0 jusqu’à 1 000, {data.max_points} à partir
        de 100 milliards, 125 points de plus chaque fois que le score brut est multiplié par 10. Ce
        score résulte d’une formule appliquée aux estimations de l’analyse ; ce n’est ni un sondage,
        ni un vote, ni une mesure d’opinion.
      </p>
    </div>
  );
}
