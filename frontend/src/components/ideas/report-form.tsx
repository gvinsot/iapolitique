import { FlagIcon, Loader2Icon, ScaleIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";

import { AltchaField } from "@/components/forms/altcha-field";
import { FormLegalLinks } from "@/components/forms/submission-notices";
import { useAntiRobot } from "@/components/forms/use-anti-robot";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Textarea } from "@/components/ui/textarea";
import {
  formatCharacterCount,
  ideaTextLength,
  REPORT_DETAILS_MAX_LENGTH,
  REPORTER_EMAIL_MAX_LENGTH,
  REPORTER_NAME_MAX_LENGTH,
  validateOptionalEmail,
  validateReportDetails,
} from "@/lib/idea-text";
import { entriesOf, isKeyOf, type ReportReason, reportReasonLabels } from "@/lib/labels";
import { cn } from "@/lib/utils";

export interface ReportFormValues {
  reason: ReportReason;
  details: string | null;
  reporterName: string | null;
  reporterEmail: string | null;
  goodFaith: true;
  altcha: string;
}

interface ReportFormProps {
  onSubmit: (values: ReportFormValues) => void | Promise<void>;
  submitting?: boolean;
  /** Uses the ALTCHA mock verification (component tests only). */
  altchaTestMode?: boolean;
}

interface FormErrors {
  reason: string | null;
  details: string | null;
  email: string | null;
  goodFaith: string | null;
}

const noErrors: FormErrors = { reason: null, details: null, email: null, goodFaith: null };

function optional(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/**
 * Report of a published idea (AGENTS.md §5.6, §7.5, "DSA léger" and LCEN art. 6): reason,
 * optional details, optional name and e-mail, good-faith declaration, reminder of the penalty
 * for abusive reports, automated processing notice.
 */
export function ReportForm({ onSubmit, submitting = false, altchaTestMode }: ReportFormProps) {
  const id = useId();
  const { altchaRef, solving, error: altchaError, submitWith } = useAntiRobot();
  const [reason, setReason] = useState<ReportReason | null>(null);
  const [details, setDetails] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [goodFaith, setGoodFaith] = useState(false);
  const [errors, setErrors] = useState<FormErrors>(noErrors);

  const busy = submitting || solving;
  const detailsLength = ideaTextLength(details);
  const ids = {
    reasonError: `${id}-reason-error`,
    detailsHelp: `${id}-details-help`,
    detailsCounter: `${id}-details-counter`,
    detailsError: `${id}-details-error`,
    emailHelp: `${id}-email-help`,
    emailError: `${id}-email-error`,
    goodFaithError: `${id}-good-faith-error`,
  };

  const handleSubmit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy) return;
    const next: FormErrors = {
      reason: reason ? null : "Choisissez le motif du signalement.",
      details: validateReportDetails(details),
      email: validateOptionalEmail(email),
      goodFaith: goodFaith
        ? null
        : "Cochez la déclaration de bonne foi pour envoyer le signalement.",
    };
    setErrors(next);
    if (!reason || next.details || next.email || next.goodFaith) return;

    await submitWith((altcha) =>
      onSubmit({
        reason,
        details: optional(details),
        reporterName: optional(name),
        reporterEmail: optional(email),
        goodFaith: true,
        altcha,
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
        <legend className="mb-3 text-base font-medium">Motif du signalement</legend>
        <RadioGroup
          value={reason ?? ""}
          onValueChange={(value) => {
            const selected = isKeyOf(reportReasonLabels, value) ? value : null;
            setReason(selected);
            if (selected) setErrors({ ...errors, reason: null });
          }}
          aria-invalid={errors.reason ? true : undefined}
          aria-describedby={errors.reason ? ids.reasonError : undefined}
          className="gap-2"
        >
          {entriesOf(reportReasonLabels).map(([value, label]) => (
            <div key={value} className="flex items-center gap-3">
              <RadioGroupItem value={value} id={`${id}-${value}`} />
              <Label htmlFor={`${id}-${value}`} className="font-normal">
                {label}
              </Label>
            </div>
          ))}
        </RadioGroup>
        {errors.reason ? (
          <p id={ids.reasonError} role="alert" className="text-sm font-medium text-destructive">
            {errors.reason}
          </p>
        ) : null}
      </fieldset>

      <div className="space-y-2">
        <Label htmlFor={`${id}-details`}>Précision (facultatif)</Label>
        <p id={ids.detailsHelp} className="text-sm text-muted-foreground">
          Indiquez le passage, le chiffre ou la source en cause. Ne nommez aucun particulier.
        </p>
        <Textarea
          id={`${id}-details`}
          value={details}
          onChange={(event) => {
            setDetails(event.target.value);
            if (errors.details) {
              setErrors({ ...errors, details: validateReportDetails(event.target.value) });
            }
          }}
          rows={4}
          aria-invalid={errors.details ? true : undefined}
          aria-describedby={cn(
            ids.detailsHelp,
            ids.detailsCounter,
            errors.details && ids.detailsError,
          )}
        />
        <div className="flex flex-wrap items-start justify-between gap-2 text-sm">
          {errors.details ? (
            <p id={ids.detailsError} role="alert" className="font-medium text-destructive">
              {errors.details}
            </p>
          ) : (
            <span />
          )}
          <p
            id={ids.detailsCounter}
            className={cn(
              "tabular-nums",
              detailsLength > REPORT_DETAILS_MAX_LENGTH
                ? "text-destructive"
                : "text-muted-foreground",
            )}
          >
            {formatCharacterCount(detailsLength, REPORT_DETAILS_MAX_LENGTH)}
          </p>
        </div>
      </div>

      <fieldset className="space-y-3">
        <legend className="mb-1 text-base font-medium">Vos coordonnées (facultatif)</legend>
        <p id={ids.emailHelp} className="text-sm text-muted-foreground">
          Utiles seulement si l’éditeur doit vous recontacter. Elles sont supprimées six mois après
          le traitement du signalement et ne sont jamais publiées.
        </p>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor={`${id}-name`}>Nom</Label>
            <Input
              id={`${id}-name`}
              value={name}
              onChange={(event) => {
                setName(event.target.value);
              }}
              maxLength={REPORTER_NAME_MAX_LENGTH}
              autoComplete="name"
              aria-describedby={ids.emailHelp}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor={`${id}-email`}>Adresse e-mail</Label>
            <Input
              id={`${id}-email`}
              type="email"
              value={email}
              onChange={(event) => {
                setEmail(event.target.value);
                if (errors.email) {
                  setErrors({ ...errors, email: validateOptionalEmail(event.target.value) });
                }
              }}
              maxLength={REPORTER_EMAIL_MAX_LENGTH}
              autoComplete="email"
              aria-invalid={errors.email ? true : undefined}
              aria-describedby={cn(ids.emailHelp, errors.email && ids.emailError)}
            />
          </div>
        </div>
        {errors.email ? (
          <p id={ids.emailError} role="alert" className="text-sm font-medium text-destructive">
            {errors.email}
          </p>
        ) : null}
      </fieldset>

      <div className="space-y-3 rounded-lg border bg-muted/40 p-4 text-sm">
        <p className="flex items-start gap-2">
          <FlagIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <span>
            Votre signalement est d’abord traité automatiquement par une IA (Mistral AI), qui peut
            maintenir l’idée, faire refaire son analyse ou la masquer. Les contenus graves sont
            masqués et examinés par un humain. Un accusé de réception et la suite donnée sont
            consultables sur votre lien de suivi.
          </span>
        </p>
        <p className="flex items-start gap-2">
          <ScaleIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <span>
            Présenter sciemment un contenu comme illicite dans le but d’en obtenir le retrait ou
            d’en faire cesser la diffusion est puni d’un an d’emprisonnement et de 15 000 € d’amende
            (article 6, VII de la loi n° 2004-575 du 21 juin 2004).
          </span>
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-start gap-3">
          <Checkbox
            id={`${id}-good-faith`}
            checked={goodFaith}
            onCheckedChange={(checked) => {
              setGoodFaith(checked === true);
              if (checked === true) setErrors({ ...errors, goodFaith: null });
            }}
            aria-invalid={errors.goodFaith ? true : undefined}
            aria-describedby={errors.goodFaith ? ids.goodFaithError : undefined}
            className="mt-0.5"
          />
          <Label htmlFor={`${id}-good-faith`} className="block leading-snug font-normal">
            Je déclare de bonne foi que les informations de ce signalement sont exactes.
          </Label>
        </div>
        {errors.goodFaith ? (
          <p id={ids.goodFaithError} role="alert" className="text-sm font-medium text-destructive">
            {errors.goodFaith}
          </p>
        ) : null}
      </div>

      <AltchaField ref={altchaRef} error={altchaError} testMode={altchaTestMode} />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <FormLegalLinks />
        <Button type="submit" disabled={busy}>
          {busy ? (
            <Loader2Icon className="animate-spin" aria-hidden="true" />
          ) : (
            <FlagIcon aria-hidden="true" />
          )}
          {solving ? "Vérification…" : submitting ? "Envoi en cours…" : "Envoyer le signalement"}
        </Button>
      </div>
    </form>
  );
}
