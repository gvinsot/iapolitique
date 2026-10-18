import { CheckIcon, XIcon } from "lucide-react";
import { useState } from "react";

import { errorMessage } from "@/api/errors";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type {
  CriterionScore,
  CriterionVerdict,
  DecisionLabel,
  ScreeningDecisionRow,
} from "@/lib/admin-model";
import { formatFactor } from "@/lib/format";
import { trackedKindLabels } from "@/lib/tracking-model";
import { cn } from "@/lib/utils";

const verdictLabels: Record<CriterionVerdict, string> = {
  pass: "réussi",
  fail: "échec",
  gray: "zone grise",
};

const outcomeLabels: Record<ScreeningDecisionRow["outcome"], string> = {
  accepted: "Acceptée",
  rejected: "Refusée",
  gray_zone: "Zone grise",
};

const dateTime = new Intl.DateTimeFormat("fr-FR", {
  dateStyle: "short",
  timeStyle: "short",
  timeZone: "Europe/Paris",
});

/** Probability of passing a criterion, with its two thresholds marked on the bar. */
function CriterionBar({ score }: { score: CriterionScore }) {
  const pct = (value: number) => `${Math.min(100, Math.max(0, value * 100))}%`;
  return (
    <li className="grid grid-cols-[8rem_minmax(0,1fr)_7.5rem] items-center gap-3 text-sm">
      <span className="truncate font-mono text-xs">{score.key}</span>
      <div className="relative h-2.5 rounded-full bg-muted" aria-hidden="true">
        <div
          className={cn(
            "absolute inset-y-0 left-0 rounded-full",
            score.verdict === "fail" ? "bg-warning" : "bg-primary",
            score.verdict === "gray" && "bg-chart-4",
          )}
          style={{ width: pct(score.probability) }}
        />
        <span
          className="absolute -inset-y-1 w-0.5 bg-foreground/70"
          style={{ left: pct(score.reject_below) }}
        />
        <span
          className="absolute -inset-y-1 w-0.5 bg-foreground/70"
          style={{ left: pct(score.accept_above) }}
        />
      </div>
      <span className="tabular-nums">
        {formatFactor(score.probability)} · {verdictLabels[score.verdict]}
        <span className="sr-only">
          {" "}
          (rejet sous {formatFactor(score.reject_below)}, acceptation au-dessus de{" "}
          {formatFactor(score.accept_above)})
        </span>
      </span>
    </li>
  );
}

interface DecisionCardProps {
  decision: ScreeningDecisionRow;
  onLabel: (id: string, label: DecisionLabel) => Promise<void>;
}

function DecisionCard({ decision, onLabel }: DecisionCardProps) {
  const [label, setLabel] = useState<DecisionLabel | null>(decision.label);
  const [pending, setPending] = useState<DecisionLabel | null>(null);
  const [error, setError] = useState<string | null>(null);

  const choose = async (next: DecisionLabel) => {
    setPending(next);
    setError(null);
    try {
      await onLabel(decision.id, next);
      setLabel(next);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(null);
    }
  };

  return (
    <li className="space-y-3 rounded-lg border bg-card p-4">
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <Badge variant="secondary">{trackedKindLabels[decision.target]}</Badge>
        <Badge variant={decision.outcome === "rejected" ? "outline" : "default"}>
          {outcomeLabels[decision.outcome]}
        </Badge>
        <span className="text-muted-foreground">
          {dateTime.format(new Date(decision.decided_at))} · grille {decision.grid_version}
        </span>
      </div>
      <p className="text-sm text-pretty whitespace-pre-line">
        {decision.excerpt ?? (
          <span className="text-muted-foreground italic">
            Texte purgé : seuls les scores sont conservés.
          </span>
        )}
      </p>
      <ul className="space-y-2" aria-label="Probabilités par critère">
        {decision.criteria.map((score) => (
          <CriterionBar key={score.key} score={score} />
        ))}
      </ul>
      <div
        role="group"
        aria-label="Étiquetage de la décision"
        className="flex flex-wrap items-center gap-2"
      >
        <Button
          type="button"
          size="sm"
          variant={label === "correct" ? "default" : "outline"}
          aria-pressed={label === "correct"}
          disabled={pending !== null}
          onClick={() => {
            void choose("correct");
          }}
        >
          <CheckIcon aria-hidden="true" />
          Décision juste
        </Button>
        <Button
          type="button"
          size="sm"
          variant={label === "incorrect" ? "default" : "outline"}
          aria-pressed={label === "incorrect"}
          disabled={pending !== null}
          onClick={() => {
            void choose("incorrect");
          }}
        >
          <XIcon aria-hidden="true" />
          Décision erronée
        </Button>
        {error ? (
          <span role="alert" className="text-sm font-medium text-destructive">
            {error}
          </span>
        ) : null}
      </div>
    </li>
  );
}

interface ScreeningDecisionsProps {
  decisions: ScreeningDecisionRow[];
  /** Stores the administrator's label, used to calibrate the thresholds (AGENTS.md §8). */
  onLabel: (id: string, label: DecisionLabel) => Promise<void>;
}

/** Raw scores of the decision model per criterion, with labelling buttons. */
export function ScreeningDecisions({ decisions, onLabel }: ScreeningDecisionsProps) {
  if (decisions.length === 0) {
    return <p className="text-muted-foreground">Aucune décision à afficher.</p>;
  }
  return (
    <ul className="space-y-4">
      {decisions.map((decision) => (
        <DecisionCard key={decision.id} decision={decision} onLabel={onLabel} />
      ))}
    </ul>
  );
}
