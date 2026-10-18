import { Loader2Icon, SendIcon } from "lucide-react";
import { type SubmitEvent, useId, useRef, useState } from "react";

import { AltchaField } from "@/components/forms/altcha-field";
import { PublicationConsentsField } from "@/components/forms/publication-consents";
import {
  AiProcessingNotice,
  FormLegalLinks,
  PersonalDataWarning,
} from "@/components/forms/submission-notices";
import { useAntiRobot } from "@/components/forms/use-anti-robot";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Textarea } from "@/components/ui/textarea";
import { noConsents, type PublicationConsents, validateConsents } from "@/lib/consents";
import {
  CONTRIBUTION_TEXT_MAX_LENGTH,
  formatCharacterCount,
  ideaTextLength,
  validateContributionText,
} from "@/lib/idea-text";
import { cn } from "@/lib/utils";

/** Kinds a visitor can submit (merged duplicates are created by the system only). */
export type SubmittableContributionKind = "amelioration" | "objection";

const kinds: { value: SubmittableContributionKind; label: string; description: string }[] = [
  {
    value: "amelioration",
    label: "Amélioration",
    description: "Précise, complète ou corrige l’idée.",
  },
  {
    value: "objection",
    label: "Objection",
    description: "Signale un inconvénient, un risque ou un obstacle.",
  },
];

export interface ContributionFormValues {
  kind: SubmittableContributionKind;
  text: string;
  altcha: string;
  acceptedLicence: true;
  acceptedPublication: true;
}

interface ContributionFormProps {
  onSubmit: (values: ContributionFormValues) => void | Promise<void>;
  submitting?: boolean;
  /** Uses the ALTCHA mock verification (component tests only). */
  altchaTestMode?: boolean;
}

interface FormErrors {
  kind: string | null;
  text: string | null;
  consents: string | null;
}

const noErrors: FormErrors = { kind: null, text: null, consents: null };

/**
 * Improvement or objection on a published idea (AGENTS.md §5.5): screened like an idea, then
 * integrated by a new analysis. Same consents, warnings and anti-robot check as the idea form.
 */
export function ContributionForm({
  onSubmit,
  submitting = false,
  altchaTestMode,
}: ContributionFormProps) {
  const id = useId();
  const textRef = useRef<HTMLTextAreaElement>(null);
  const { altchaRef, solving, error: altchaError, submitWith } = useAntiRobot();
  const [kind, setKind] = useState<SubmittableContributionKind | null>(null);
  const [text, setText] = useState("");
  const [consents, setConsents] = useState<PublicationConsents>(noConsents);
  const [errors, setErrors] = useState<FormErrors>(noErrors);

  const length = ideaTextLength(text);
  const busy = submitting || solving;
  const kindErrorId = `${id}-kind-error`;
  const warningId = `${id}-warning`;
  const counterId = `${id}-counter`;
  const textErrorId = `${id}-text-error`;

  const handleSubmit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy) return;
    const next: FormErrors = {
      kind: kind ? null : "Choisissez s’il s’agit d’une amélioration ou d’une objection.",
      text: validateContributionText(text),
      consents: validateConsents(consents),
    };
    setErrors(next);
    if (next.text) textRef.current?.focus();
    if (!kind || next.text || next.consents) return;

    await submitWith((altcha) =>
      onSubmit({
        kind,
        text: text.trim(),
        altcha,
        acceptedLicence: true,
        acceptedPublication: true,
      }),
    );
  };

  return (
    <form
      onSubmit={(event) => {
        void handleSubmit(event);
      }}
      noValidate
      className="space-y-6"
      aria-busy={busy}
    >
      <fieldset className="space-y-3">
        <legend className="mb-3 text-base font-medium">Type de contribution</legend>
        <RadioGroup
          value={kind ?? ""}
          onValueChange={(value) => {
            const selected = kinds.find((item) => item.value === value)?.value ?? null;
            setKind(selected);
            if (selected) setErrors({ ...errors, kind: null });
          }}
          aria-invalid={errors.kind ? true : undefined}
          aria-describedby={errors.kind ? kindErrorId : undefined}
          className="grid gap-3 sm:grid-cols-2"
        >
          {kinds.map((item) => (
            <Label
              key={item.value}
              htmlFor={`${id}-${item.value}`}
              className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal has-data-[state=checked]:border-primary"
            >
              <RadioGroupItem value={item.value} id={`${id}-${item.value}`} className="mt-0.5" />
              <span className="space-y-1">
                <span className="block font-medium">{item.label}</span>
                <span className="block text-sm text-muted-foreground">{item.description}</span>
              </span>
            </Label>
          ))}
        </RadioGroup>
        {errors.kind ? (
          <p id={kindErrorId} role="alert" className="text-sm font-medium text-destructive">
            {errors.kind}
          </p>
        ) : null}
      </fieldset>

      <div className="space-y-2">
        <Label htmlFor={`${id}-text`} className="text-base">
          Votre contribution
        </Label>
        <PersonalDataWarning id={warningId} />
        <Textarea
          ref={textRef}
          id={`${id}-text`}
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            if (errors.text) {
              setErrors({ ...errors, text: validateContributionText(event.target.value) });
            }
          }}
          rows={6}
          required
          aria-invalid={errors.text ? true : undefined}
          aria-describedby={cn(warningId, counterId, errors.text && textErrorId)}
          className="min-h-32 text-base"
        />
        <div className="flex flex-wrap items-start justify-between gap-2 text-sm">
          {errors.text ? (
            <p id={textErrorId} role="alert" className="font-medium text-destructive">
              {errors.text}
            </p>
          ) : (
            <span />
          )}
          <p
            id={counterId}
            className={cn(
              "tabular-nums",
              length > CONTRIBUTION_TEXT_MAX_LENGTH ? "text-destructive" : "text-muted-foreground",
            )}
          >
            {formatCharacterCount(length, CONTRIBUTION_TEXT_MAX_LENGTH)}
          </p>
        </div>
      </div>

      <AiProcessingNotice />

      <PublicationConsentsField
        value={consents}
        onChange={(value) => {
          setConsents(value);
          if (errors.consents) setErrors({ ...errors, consents: validateConsents(value) });
        }}
        error={errors.consents}
      />

      <AltchaField ref={altchaRef} error={altchaError} testMode={altchaTestMode} />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <FormLegalLinks />
        <Button type="submit" disabled={busy}>
          {busy ? (
            <Loader2Icon className="animate-spin" aria-hidden="true" />
          ) : (
            <SendIcon aria-hidden="true" />
          )}
          {solving ? "Vérification…" : submitting ? "Envoi en cours…" : "Envoyer ma contribution"}
        </Button>
      </div>
    </form>
  );
}
