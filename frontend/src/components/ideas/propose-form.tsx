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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { noConsents, type PublicationConsents, validateConsents } from "@/lib/consents";
import {
  formatCharacterCount,
  IDEA_TEXT_MAX_LENGTH,
  ideaTextLength,
  TERRITORY_MAX_LENGTH,
  validateIdeaText,
} from "@/lib/idea-text";
import { type AdminLevel, adminLevelLabels, entriesOf, isKeyOf } from "@/lib/labels";
import { cn } from "@/lib/utils";

export interface ProposeFormValues {
  text: string;
  adminLevel: AdminLevel | null;
  territory: string | null;
  /** ALTCHA payload, accepted once by the API. */
  altcha: string;
  /** Both boxes were checked: the API stores the rules version and the day. */
  acceptedLicence: true;
  acceptedPublication: true;
}

interface ProposeFormProps {
  onSubmit: (values: ProposeFormValues) => void | Promise<void>;
  submitting?: boolean;
  /** Uses the ALTCHA mock verification (component tests only). */
  altchaTestMode?: boolean;
}

const UNKNOWN_LEVEL = "unknown";

interface FormErrors {
  text: string | null;
  consents: string | null;
}

const noErrors: FormErrors = { text: null, consents: null };

/**
 * Idea submission form (AGENTS.md §5.1, §7.5): free text, optional administrative level and
 * territory, the two consents (never pre-checked), the personal data warning, the AI processing
 * notice and the self-hosted anti-robot check. Submission is injected by the page.
 */
export function ProposeForm({ onSubmit, submitting = false, altchaTestMode }: ProposeFormProps) {
  const id = useId();
  const textRef = useRef<HTMLTextAreaElement>(null);
  const { altchaRef, solving, error: altchaError, submitWith } = useAntiRobot();
  const [text, setText] = useState("");
  const [adminLevel, setAdminLevel] = useState<AdminLevel | null>(null);
  const [territory, setTerritory] = useState("");
  const [consents, setConsents] = useState<PublicationConsents>(noConsents);
  const [errors, setErrors] = useState<FormErrors>(noErrors);

  const length = ideaTextLength(text);
  const overLimit = length > IDEA_TEXT_MAX_LENGTH;
  const textHelpId = `${id}-text-help`;
  const warningId = `${id}-text-warning`;
  const counterId = `${id}-text-counter`;
  const errorId = `${id}-text-error`;
  const busy = submitting || solving;

  const handleSubmit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy) return;
    const textError = validateIdeaText(text);
    const consentsError = validateConsents(consents);
    setErrors({ text: textError, consents: consentsError });
    if (textError) {
      textRef.current?.focus();
      return;
    }
    if (consentsError) return;

    const trimmedTerritory = territory.trim();
    await submitWith((altcha) =>
      onSubmit({
        text: text.trim(),
        adminLevel,
        territory: trimmedTerritory === "" ? null : trimmedTerritory,
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
      <div className="space-y-2">
        <Label htmlFor={`${id}-text`} className="text-base">
          Votre idée
        </Label>
        <p id={textHelpId} className="text-sm text-muted-foreground">
          Décrivez la mesure, l’administration concernée et, si possible, ce qu’elle permettrait
          d’économiser ou de financer.
        </p>
        <PersonalDataWarning id={warningId} />
        <Textarea
          ref={textRef}
          id={`${id}-text`}
          name="text"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            if (errors.text) setErrors({ ...errors, text: validateIdeaText(event.target.value) });
          }}
          rows={10}
          required
          aria-invalid={errors.text ? true : undefined}
          aria-describedby={cn(textHelpId, warningId, counterId, errors.text && errorId)}
          className="min-h-48 text-base"
        />
        <div className="flex flex-wrap items-start justify-between gap-2 text-sm">
          {errors.text ? (
            <p id={errorId} className="font-medium text-destructive" role="alert">
              {errors.text}
            </p>
          ) : (
            <span />
          )}
          <p
            id={counterId}
            className={cn("tabular-nums", overLimit ? "text-destructive" : "text-muted-foreground")}
          >
            {formatCharacterCount(length)}
          </p>
        </div>
      </div>

      <div className="grid gap-6 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor={`${id}-level`}>Niveau administratif concerné (facultatif)</Label>
          <Select
            value={adminLevel ?? UNKNOWN_LEVEL}
            onValueChange={(value) => {
              setAdminLevel(isKeyOf(adminLevelLabels, value) ? value : null);
            }}
          >
            <SelectTrigger id={`${id}-level`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={UNKNOWN_LEVEL}>Je ne sais pas</SelectItem>
              {entriesOf(adminLevelLabels).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <Label htmlFor={`${id}-territory`}>Territoire concerné (facultatif)</Label>
          <Input
            id={`${id}-territory`}
            name="territory"
            value={territory}
            onChange={(event) => {
              setTerritory(event.target.value);
            }}
            maxLength={TERRITORY_MAX_LENGTH}
            placeholder="Ex. : Bretagne, Lyon, Seine-Saint-Denis"
            autoComplete="off"
          />
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
        <Button type="submit" size="lg" disabled={busy}>
          {busy ? (
            <Loader2Icon className="animate-spin" aria-hidden="true" />
          ) : (
            <SendIcon aria-hidden="true" />
          )}
          {solving ? "Vérification…" : submitting ? "Envoi en cours…" : "Envoyer mon idée"}
        </Button>
      </div>
    </form>
  );
}
