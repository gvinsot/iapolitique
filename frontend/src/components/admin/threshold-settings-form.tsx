import { Loader2Icon, SaveIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";

import { errorMessage } from "@/api/errors";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  type CriterionThreshold,
  type ThresholdSettings,
  validateDedupThreshold,
  validateThreshold,
} from "@/lib/admin-model";

type Target = "idea" | "contribution" | "report";

const targetLabels: Record<Target, string> = {
  idea: "Idées",
  contribution: "Contributions",
  report: "Signalements",
};

const targets: readonly Target[] = ["idea", "contribution", "report"];

interface ThresholdSettingsFormProps {
  initial: ThresholdSettings;
  onSubmit: (settings: ThresholdSettings) => Promise<void>;
}

function parseProbability(value: string): number {
  return value.trim() === "" ? Number.NaN : Number(value.replace(",", "."));
}

/**
 * Screening and duplicate thresholds (AGENTS.md §5.3, §5.4, §8). Values are probabilities of
 * passing a criterion: below `reject_below` the text is refused, above `accept_above` it is
 * accepted, in between a second opinion is requested. Never hard-coded: calibrated here.
 */
export function ThresholdSettingsForm({ initial, onSubmit }: ThresholdSettingsFormProps) {
  const id = useId();
  const [values, setValues] = useState<Record<string, string>>(() => {
    const entries: [string, string][] = [];
    for (const target of targets) {
      for (const threshold of initial[target]) {
        entries.push([`${target}.${threshold.key}.reject_below`, String(threshold.reject_below)]);
        entries.push([`${target}.${threshold.key}.accept_above`, String(threshold.accept_above)]);
      }
    }
    entries.push(["dedup", String(initial.dedup_similarity_threshold)]);
    return Object.fromEntries(entries);
  });
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; message: string } | null>(null);

  const read = (key: string) => parseProbability(values[key] ?? "");

  const build = (): { settings: ThresholdSettings; errors: Record<string, string> } => {
    const found: Record<string, string> = {};
    const settings: ThresholdSettings = {
      idea: [],
      contribution: [],
      report: [],
      dedup_similarity_threshold: read("dedup"),
    };
    for (const target of targets) {
      settings[target] = initial[target].map((threshold): CriterionThreshold => {
        const next = {
          key: threshold.key,
          reject_below: read(`${target}.${threshold.key}.reject_below`),
          accept_above: read(`${target}.${threshold.key}.accept_above`),
        };
        const error = validateThreshold(next);
        if (error) found[`${target}.${threshold.key}`] = error;
        return next;
      });
    }
    const dedupError = validateDedupThreshold(settings.dedup_similarity_threshold);
    if (dedupError) found.dedup = dedupError;
    return { settings, errors: found };
  };

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const { settings, errors: found } = build();
    setErrors(found);
    setStatus(null);
    if (Object.keys(found).length > 0) return;
    setPending(true);
    try {
      await onSubmit(settings);
      setStatus({ ok: true, message: "Seuils enregistrés." });
    } catch (cause) {
      setStatus({ ok: false, message: errorMessage(cause) });
    } finally {
      setPending(false);
    }
  };

  const field = (key: string, label: string, describedBy?: string) => (
    <div className="space-y-1">
      <Label htmlFor={`${id}-${key}`} className="text-xs text-muted-foreground">
        {label}
      </Label>
      <Input
        id={`${id}-${key}`}
        inputMode="decimal"
        value={values[key] ?? ""}
        onChange={(event) => {
          setValues({ ...values, [key]: event.target.value });
        }}
        aria-describedby={describedBy}
        aria-invalid={describedBy ? true : undefined}
        className="w-28 tabular-nums"
      />
    </div>
  );

  return (
    <form
      onSubmit={(event) => {
        void submit(event);
      }}
      noValidate
      className="space-y-8"
    >
      {targets.map((target) => (
        <fieldset key={target} className="space-y-3">
          <legend className="mb-2 text-lg font-semibold">{targetLabels[target]}</legend>
          {initial[target].map((threshold) => {
            const errorKey = `${target}.${threshold.key}`;
            const errorId = errors[errorKey] ? `${id}-${errorKey}-error` : undefined;
            return (
              <fieldset key={threshold.key} className="space-y-2 rounded-lg border p-3">
                <legend className="px-1 font-mono text-sm">{threshold.key}</legend>
                <div className="flex flex-wrap gap-4">
                  {field(`${errorKey}.reject_below`, "Rejet en dessous de", errorId)}
                  {field(`${errorKey}.accept_above`, "Acceptation au-dessus de", errorId)}
                </div>
                {errorId ? (
                  <p id={errorId} role="alert" className="text-sm font-medium text-destructive">
                    {errors[errorKey]}
                  </p>
                ) : null}
              </fieldset>
            );
          })}
        </fieldset>
      ))}

      <fieldset className="space-y-2">
        <legend className="mb-2 text-lg font-semibold">Doublons</legend>
        <p id={`${id}-dedup-help`} className="text-sm text-muted-foreground">
          Similarité cosinus à partir de laquelle une idée acceptée est rattachée à une idée
          publiée. En cas de doute, ne pas fusionner : préférer une valeur élevée.
        </p>
        {field(
          "dedup",
          "Seuil de similarité",
          errors.dedup ? `${id}-dedup-error` : `${id}-dedup-help`,
        )}
        {errors.dedup ? (
          <p id={`${id}-dedup-error`} role="alert" className="text-sm font-medium text-destructive">
            {errors.dedup}
          </p>
        ) : null}
      </fieldset>

      {status ? (
        <Alert variant={status.ok ? "info" : "destructive"} role={status.ok ? "status" : "alert"}>
          <AlertDescription>
            <p>{status.message}</p>
          </AlertDescription>
        </Alert>
      ) : null}

      <Button type="submit" disabled={pending}>
        {pending ? (
          <Loader2Icon className="animate-spin" aria-hidden="true" />
        ) : (
          <SaveIcon aria-hidden="true" />
        )}
        Enregistrer les seuils
      </Button>
    </form>
  );
}
