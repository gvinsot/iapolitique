import { CheckIcon, RotateCcwIcon, XIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

import { errorMessage } from "@/api/errors";
import type {
  AdminCriterionScore,
  AdminScreening,
  AdminScreeningLabelRequest,
  CriterionVerdict,
} from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  decisionLabels,
  labelFor,
  labelVerdictLabels,
  screeningOutcomeLabels,
  targetKindLabels,
} from "@/lib/admin-model";
import { formatDateTime, formatFactor } from "@/lib/format";
import { cn } from "@/lib/utils";

const verdictLabels: Record<CriterionVerdict, string> = {
  pass: "réussi",
  fail: "échec",
  gray: "zone grise",
};

const pct = (value: number) => `${Math.min(100, Math.max(0, value * 100))}%`;

/** Probability of passing a criterion, with its two thresholds marked on the bar. */
function CriterionBar({ score }: { score: AdminCriterionScore }) {
  return (
    <li className="grid grid-cols-[minmax(0,7rem)_minmax(0,1fr)_auto] items-center gap-3 text-sm sm:grid-cols-[9rem_minmax(0,1fr)_9rem]">
      <span className="truncate font-mono text-xs">{score.key}</span>
      <div className="relative h-2.5 rounded-full bg-muted" aria-hidden="true">
        {score.probability !== null ? (
          <div
            className={cn(
              "absolute inset-y-0 left-0 rounded-full bg-primary",
              score.verdict === "fail" && "bg-warning",
              score.verdict === "gray" && "bg-chart-4",
            )}
            style={{ width: pct(score.probability) }}
          />
        ) : null}
        {score.reject_below !== null ? (
          <span
            className="absolute -inset-y-1 w-0.5 bg-foreground/70"
            style={{ left: pct(score.reject_below) }}
          />
        ) : null}
        {score.accept_above !== null ? (
          <span
            className="absolute -inset-y-1 w-0.5 bg-foreground/70"
            style={{ left: pct(score.accept_above) }}
          />
        ) : null}
      </div>
      <span className="text-right tabular-nums">
        {score.probability === null ? "—" : formatFactor(score.probability)}
        {score.verdict ? ` · ${verdictLabels[score.verdict]}` : null}
        {score.reject_below !== null && score.accept_above !== null ? (
          <span className="sr-only">
            {" "}
            (rejet sous {formatFactor(score.reject_below)}, acceptation à partir de{" "}
            {formatFactor(score.accept_above)})
          </span>
        ) : null}
      </span>
    </li>
  );
}

interface DecisionCardProps {
  screening: AdminScreening;
  onLabel: (id: string, request: AdminScreeningLabelRequest) => Promise<void>;
  onUnlabel: (id: string) => Promise<void>;
}

function LabelButton({
  pressed,
  disabled,
  onClick,
  children,
}: {
  pressed: boolean;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Button
      type="button"
      size="sm"
      variant={pressed ? "default" : "outline"}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}

function DecisionCard({ screening, onLabel, onUnlabel }: DecisionCardProps) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const label = screening.label;

  const run = async (action: () => Promise<void>) => {
    setPending(true);
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  const decided = screening.final_decision !== null;

  return (
    <li className="space-y-3 rounded-lg border bg-card p-4">
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <Badge variant="secondary">{labelFor(targetKindLabels, screening.target_kind)}</Badge>
        {screening.outcome ? (
          <Badge variant={screening.outcome === "rejected" ? "outline" : "default"}>
            {screeningOutcomeLabels[screening.outcome]}
          </Badge>
        ) : (
          <Badge variant="destructive">Erreur : {screening.error_code ?? "inconnue"}</Badge>
        )}
        {screening.reason_code ? (
          <span className="font-mono text-xs">{screening.reason_code}</span>
        ) : null}
        <span className="text-muted-foreground">
          {formatDateTime(screening.created_at)} · grille {screening.grid_version}
          {screening.latency_ms !== null ? ` · ${screening.latency_ms} ms` : null}
        </span>
      </div>
      <p className="text-sm text-pretty whitespace-pre-line">
        {screening.excerpt ?? (
          <span className="text-muted-foreground italic">
            Texte purgé : seuls les scores sont conservés.
          </span>
        )}
      </p>
      <ul className="space-y-2" aria-label="Probabilités par critère">
        {screening.criteria.map((score) => (
          <CriterionBar key={score.key} score={score} />
        ))}
      </ul>
      {screening.second_opinion ? (
        <div className="rounded-md bg-muted/50 p-3 text-sm">
          <p className="font-medium">
            Second avis
            {screening.second_opinion.model ? ` (${screening.second_opinion.model})` : ""} :{" "}
            {screening.second_opinion.decision
              ? decisionLabels[screening.second_opinion.decision].toLowerCase()
              : "en attente"}
            {screening.second_opinion.reason_code
              ? ` · ${screening.second_opinion.reason_code}`
              : null}
          </p>
          {screening.second_opinion.explanation ? (
            <p className="text-pretty text-muted-foreground">
              {screening.second_opinion.explanation}
            </p>
          ) : null}
        </div>
      ) : null}
      <p className="text-sm">
        Décision finale :{" "}
        <strong>
          {screening.final_decision
            ? decisionLabels[screening.final_decision].toLowerCase()
            : "aucune (erreur ou second avis en attente)"}
        </strong>
        {label ? (
          <>
            {" "}
            · étiquette : {labelVerdictLabels[label.label].toLowerCase()} (attendu :{" "}
            {decisionLabels[label.expected].toLowerCase()})
          </>
        ) : null}
      </p>
      <div
        role="group"
        aria-label="Étiquetage de la décision"
        className="flex flex-wrap items-center gap-2"
      >
        {decided ? (
          <>
            <LabelButton
              pressed={label?.label === "correct"}
              disabled={pending}
              onClick={() => {
                void run(() => onLabel(screening.id, { label: "correct" }));
              }}
            >
              <CheckIcon aria-hidden="true" />
              Décision juste
            </LabelButton>
            <LabelButton
              pressed={label?.label === "incorrect"}
              disabled={pending}
              onClick={() => {
                void run(() => onLabel(screening.id, { label: "incorrect" }));
              }}
            >
              <XIcon aria-hidden="true" />
              Décision erronée
            </LabelButton>
          </>
        ) : (
          <>
            <LabelButton
              pressed={label?.expected === "accept"}
              disabled={pending}
              onClick={() => {
                void run(() => onLabel(screening.id, { label: "incorrect", expected: "accept" }));
              }}
            >
              Aurait dû être acceptée
            </LabelButton>
            <LabelButton
              pressed={label?.expected === "reject"}
              disabled={pending}
              onClick={() => {
                void run(() => onLabel(screening.id, { label: "incorrect", expected: "reject" }));
              }}
            >
              Aurait dû être refusée
            </LabelButton>
          </>
        )}
        {label ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={pending}
            onClick={() => {
              void run(() => onUnlabel(screening.id));
            }}
          >
            <RotateCcwIcon aria-hidden="true" />
            Retirer l’étiquette
          </Button>
        ) : null}
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
  screenings: AdminScreening[];
  /** Stores the administrator's label, used to calibrate the thresholds (AGENTS.md §8). */
  onLabel: (id: string, request: AdminScreeningLabelRequest) => Promise<void>;
  onUnlabel: (id: string) => Promise<void>;
}

/** Raw scores of the decision model per criterion, with labelling buttons. */
export function ScreeningDecisions({ screenings, onLabel, onUnlabel }: ScreeningDecisionsProps) {
  if (screenings.length === 0) {
    return <p className="text-muted-foreground">Aucune décision ne correspond.</p>;
  }
  return (
    <ul className="space-y-4" aria-label="Décisions de filtrage">
      {screenings.map((screening) => (
        <DecisionCard
          key={screening.id}
          screening={screening}
          onLabel={onLabel}
          onUnlabel={onUnlabel}
        />
      ))}
    </ul>
  );
}
