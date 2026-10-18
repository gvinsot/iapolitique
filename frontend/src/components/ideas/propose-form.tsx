import { Loader2Icon, SendIcon } from "lucide-react";
import { type SubmitEvent, useId, useRef, useState } from "react";

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
}

interface ProposeFormProps {
  onSubmit: (values: ProposeFormValues) => void | Promise<void>;
  submitting?: boolean;
}

const UNKNOWN_LEVEL = "unknown";

/** Idea submission form: free text plus optional administrative level and territory. */
export function ProposeForm({ onSubmit, submitting = false }: ProposeFormProps) {
  const id = useId();
  const textRef = useRef<HTMLTextAreaElement>(null);
  const [text, setText] = useState("");
  const [adminLevel, setAdminLevel] = useState<AdminLevel | null>(null);
  const [territory, setTerritory] = useState("");
  const [error, setError] = useState<string | null>(null);

  const length = ideaTextLength(text);
  const overLimit = length > IDEA_TEXT_MAX_LENGTH;
  const textHelpId = `${id}-text-help`;
  const counterId = `${id}-text-counter`;
  const errorId = `${id}-text-error`;

  const handleSubmit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const validationError = validateIdeaText(text);
    setError(validationError);
    if (validationError) {
      textRef.current?.focus();
      return;
    }
    const trimmedTerritory = territory.trim();
    void onSubmit({
      text: text.trim(),
      adminLevel,
      territory: trimmedTerritory === "" ? null : trimmedTerritory,
    });
  };

  return (
    <form onSubmit={handleSubmit} noValidate className="space-y-6">
      <div className="space-y-2">
        <Label htmlFor={`${id}-text`} className="text-base">
          Votre idée
        </Label>
        <p id={textHelpId} className="text-sm text-muted-foreground">
          Décrivez la mesure, l’administration concernée et, si possible, ce qu’elle permettrait
          d’économiser ou de financer. N’indiquez aucune information permettant d’identifier une
          personne.
        </p>
        <Textarea
          ref={textRef}
          id={`${id}-text`}
          name="text"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            if (error) setError(validateIdeaText(event.target.value));
          }}
          rows={10}
          required
          aria-invalid={error ? true : undefined}
          aria-describedby={cn(textHelpId, counterId, error && errorId)}
          className="min-h-48 text-base"
        />
        <div className="flex flex-wrap items-start justify-between gap-2 text-sm">
          {error ? (
            <p id={errorId} className="font-medium text-destructive" role="alert">
              {error}
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

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-sm text-muted-foreground">
          Une courte vérification anti-robot, sans service tiers, est effectuée à l’envoi.
        </p>
        <Button type="submit" size="lg" disabled={submitting} aria-busy={submitting}>
          {submitting ? (
            <Loader2Icon className="animate-spin" aria-hidden="true" />
          ) : (
            <SendIcon aria-hidden="true" />
          )}
          {submitting ? "Envoi en cours…" : "Envoyer mon idée"}
        </Button>
      </div>
    </form>
  );
}
