import { Loader2Icon, RotateCcwIcon, SaveIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";

import { errorMessage } from "@/api/errors";
import type { AdminSettings, AdminThresholdsUpdate } from "@/api/types";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  targetSectionLabels,
  thresholdOverrides,
  validateDedupThreshold,
  validateThresholds,
} from "@/lib/admin-model";
import { formatFactor } from "@/lib/format";

interface Status {
  ok: boolean;
  message: string;
}

function parseProbability(value: string): number {
  return value.trim() === "" ? Number.NaN : Number(value.replace(",", "."));
}

function StatusMessage({ status }: { status: Status | null }) {
  if (!status) return null;
  return (
    <Alert variant={status.ok ? "info" : "destructive"} role={status.ok ? "status" : "alert"}>
      <AlertDescription>
        <p>{status.message}</p>
      </AlertDescription>
    </Alert>
  );
}

/** Runs a save with its pending and result states. */
function useSave() {
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<Status | null>(null);
  const run = async (action: () => Promise<void>, success: string) => {
    setPending(true);
    setStatus(null);
    try {
      await action();
      setStatus({ ok: true, message: success });
    } catch (cause) {
      setStatus({ ok: false, message: errorMessage(cause) });
    } finally {
      setPending(false);
    }
  };
  return { pending, status, setStatus, run };
}

const key = (kind: string, criterion: string, field: string) => `${kind}.${criterion}.${field}`;

interface ThresholdSettingsFormProps {
  settings: AdminSettings;
  onSave: (update: AdminThresholdsUpdate) => Promise<void>;
}

/**
 * Screening thresholds (AGENTS.md §5.3, §13.6). Values are probabilities of passing a criterion:
 * below `reject_below` the text is refused, from `accept_above` it is accepted, in between a
 * second opinion is requested. Only the values differing from screening.yaml are saved.
 */
export function ThresholdSettingsForm({ settings, onSave }: ThresholdSettingsFormProps) {
  const id = useId();
  const initialValues = () =>
    Object.fromEntries(
      settings.screening.flatMap((section) =>
        section.criteria.flatMap((criterion) => [
          [key(section.kind, criterion.key, "reject_below"), String(criterion.reject_below)],
          [key(section.kind, criterion.key, "accept_above"), String(criterion.accept_above)],
        ]),
      ),
    );
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const save = useSave();

  const read = (name: string) => parseProbability(values[name] ?? "");

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const found: Record<string, string> = {};
    for (const section of settings.screening) {
      for (const criterion of section.criteria) {
        const error = validateThresholds(
          read(key(section.kind, criterion.key, "reject_below")),
          read(key(section.kind, criterion.key, "accept_above")),
        );
        if (error) found[`${section.kind}.${criterion.key}`] = error;
      }
    }
    setErrors(found);
    save.setStatus(null);
    if (Object.keys(found).length > 0) return;
    const update = thresholdOverrides(settings.screening, (kind, criterion) => ({
      reject_below: read(key(kind, criterion.key, "reject_below")),
      accept_above: read(key(kind, criterion.key, "accept_above")),
    }));
    await save.run(() => onSave(update), "Seuils de filtrage enregistrés.");
  };

  const resetToFile = async () => {
    setErrors({});
    await save.run(
      () => onSave({}),
      "Les seuils du fichier screening.yaml s’appliquent de nouveau.",
    );
  };

  return (
    <form
      onSubmit={(event) => {
        void submit(event);
      }}
      noValidate
      className="space-y-8"
      aria-label="Seuils de filtrage"
    >
      <p className="text-sm text-muted-foreground">
        Grille {settings.grid_version}. Chaque critère a deux seuils sur la probabilité de le
        réussir : refus en dessous du premier, acceptation à partir du second, second avis entre les
        deux. Les valeurs par défaut sont celles de <code>screening.yaml</code>.
      </p>
      {settings.screening.map((section) => (
        <fieldset key={section.kind} className="space-y-3">
          <legend className="mb-2 text-lg font-semibold">
            {targetSectionLabels[section.kind]}
          </legend>
          <div className="grid gap-3 lg:grid-cols-2">
            {section.criteria.map((criterion) => {
              const errorKey = `${section.kind}.${criterion.key}`;
              const errorId = errors[errorKey] ? `${id}-${errorKey}-error` : undefined;
              return (
                <fieldset key={criterion.key} className="space-y-2 rounded-lg border bg-card p-3">
                  <legend className="flex items-center gap-2 px-1 font-mono text-sm">
                    {criterion.key}
                    {criterion.adjusted ? <Badge variant="secondary">ajusté</Badge> : null}
                  </legend>
                  <div className="flex flex-wrap gap-4">
                    {(["reject_below", "accept_above"] as const).map((field) => {
                      const name = key(section.kind, criterion.key, field);
                      const fallback =
                        field === "reject_below"
                          ? criterion.default_reject_below
                          : criterion.default_accept_above;
                      return (
                        <div key={field} className="space-y-1">
                          <Label
                            htmlFor={`${id}-${name}`}
                            className="text-xs text-muted-foreground"
                          >
                            {field === "reject_below"
                              ? "Refus en dessous de"
                              : "Acceptation à partir de"}
                          </Label>
                          <Input
                            id={`${id}-${name}`}
                            inputMode="decimal"
                            value={values[name] ?? ""}
                            onChange={(event) => {
                              setValues({ ...values, [name]: event.target.value });
                            }}
                            aria-describedby={errorId}
                            aria-invalid={errorId ? true : undefined}
                            className="w-28 tabular-nums"
                          />
                          <p className="text-xs text-muted-foreground">
                            fichier : {formatFactor(fallback)}
                          </p>
                        </div>
                      );
                    })}
                  </div>
                  {errorId ? (
                    <p id={errorId} role="alert" className="text-sm font-medium text-destructive">
                      {errors[errorKey]}
                    </p>
                  ) : null}
                </fieldset>
              );
            })}
          </div>
        </fieldset>
      ))}
      <StatusMessage status={save.status} />
      <div className="flex flex-wrap gap-3">
        <Button type="submit" disabled={save.pending}>
          {save.pending ? (
            <Loader2Icon className="animate-spin" aria-hidden="true" />
          ) : (
            <SaveIcon aria-hidden="true" />
          )}
          Enregistrer les seuils
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={save.pending}
          onClick={() => {
            void resetToFile();
          }}
        >
          <RotateCcwIcon aria-hidden="true" />
          Revenir aux seuils du fichier
        </Button>
      </div>
    </form>
  );
}

interface DedupThresholdFormProps {
  settings: AdminSettings;
  onSave: (value: number | null) => Promise<void>;
}

/** Cosine similarity from which an accepted idea is merged into a published one (§5.4). */
export function DedupThresholdForm({ settings, onSave }: DedupThresholdFormProps) {
  const id = useId();
  const dedup = settings.dedup_similarity_threshold;
  const [value, setValue] = useState(String(dedup.value));
  const [error, setError] = useState<string | null>(null);
  const save = useSave();

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const parsed = parseProbability(value);
    const found = validateDedupThreshold(parsed, dedup.minimum);
    setError(found);
    save.setStatus(null);
    if (found) return;
    await save.run(() => onSave(parsed), "Seuil de doublon enregistré.");
  };

  return (
    <form
      onSubmit={(event) => {
        void submit(event);
      }}
      noValidate
      className="space-y-3"
      aria-label="Seuil de doublon"
    >
      <p id={`${id}-help`} className="text-sm text-muted-foreground">
        Similarité cosinus à partir de laquelle une idée acceptée est rattachée à l’idée publiée la
        plus proche. Valeur de l’environnement : {formatFactor(dedup.default)}
        {dedup.adjusted ? " (remplacée par le réglage ci-dessous)" : ""}. En cas de doute, ne pas
        fusionner : préférer une valeur élevée.
      </p>
      <div className="space-y-1">
        <Label htmlFor={`${id}-value`}>Seuil de similarité</Label>
        <Input
          id={`${id}-value`}
          inputMode="decimal"
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
          }}
          aria-describedby={error ? `${id}-error` : `${id}-help`}
          aria-invalid={error ? true : undefined}
          className="w-28 tabular-nums"
        />
      </div>
      {error ? (
        <p id={`${id}-error`} role="alert" className="text-sm font-medium text-destructive">
          {error}
        </p>
      ) : null}
      <StatusMessage status={save.status} />
      <div className="flex flex-wrap gap-3">
        <Button type="submit" disabled={save.pending}>
          <SaveIcon aria-hidden="true" />
          Enregistrer le seuil
        </Button>
        {dedup.adjusted ? (
          <Button
            type="button"
            variant="outline"
            disabled={save.pending}
            onClick={() => {
              setError(null);
              setValue(String(dedup.default));
              void save.run(
                () => onSave(null),
                "Le seuil de l’environnement s’applique de nouveau.",
              );
            }}
          >
            <RotateCcwIcon aria-hidden="true" />
            Revenir à la valeur de l’environnement
          </Button>
        ) : null}
      </div>
    </form>
  );
}
