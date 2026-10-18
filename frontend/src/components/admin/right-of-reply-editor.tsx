import { Loader2Icon, SendIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";

import { errorMessage } from "@/api/errors";
import { RightOfReply } from "@/components/ideas/right-of-reply";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { type RightOfReplyDraft, rightOfReplyDeadline } from "@/lib/admin-model";
import { formatDate } from "@/lib/format";
import { formatCharacterCount, ideaTextLength } from "@/lib/idea-text";

export const RIGHT_OF_REPLY_MAX_LENGTH = 5000;
export const RIGHT_OF_REPLY_AUTHOR_MAX_LENGTH = 200;

interface RightOfReplyEditorProps {
  idea: { slug: string; title: string };
  initial?: RightOfReplyDraft | null;
  onSubmit: (draft: RightOfReplyDraft) => Promise<void>;
  /** Injected for tests; defaults to today. */
  today?: Date;
}

interface DraftErrors {
  author_label?: string;
  body?: string;
  requested_on?: string;
}

function validateDraft(draft: RightOfReplyDraft): DraftErrors {
  const errors: DraftErrors = {};
  if (draft.author_label.trim() === "") {
    errors.author_label = "Indiquez comment la personne qui répond souhaite être désignée.";
  }
  const length = ideaTextLength(draft.body);
  if (length === 0) errors.body = "Saisissez le texte de la réponse, tel que reçu.";
  else if (length > RIGHT_OF_REPLY_MAX_LENGTH) {
    errors.body = `La réponse dépasse ${formatCharacterCount(length, RIGHT_OF_REPLY_MAX_LENGTH)}.`;
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(draft.requested_on)) {
    errors.requested_on = "Indiquez la date de réception de la demande.";
  }
  return errors;
}

/**
 * Publication of a right of reply under an idea (LCEN art. 1-1 III, AGENTS.md §7.5): the text is
 * published as received, within 3 days of the request. Shows the deadline and a preview.
 */
export function RightOfReplyEditor({
  idea,
  initial = null,
  onSubmit,
  today = new Date(),
}: RightOfReplyEditorProps) {
  const id = useId();
  const [draft, setDraft] = useState<RightOfReplyDraft>(
    initial ?? { author_label: "", body: "", requested_on: today.toISOString().slice(0, 10) },
  );
  const [errors, setErrors] = useState<DraftErrors>({});
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; message: string } | null>(null);

  const deadline = /^\d{4}-\d{2}-\d{2}$/.test(draft.requested_on)
    ? rightOfReplyDeadline(draft.requested_on)
    : null;
  const overdue = deadline !== null && deadline < today.toISOString().slice(0, 10);

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const found = validateDraft(draft);
    setErrors(found);
    setStatus(null);
    if (Object.keys(found).length > 0) return;
    setPending(true);
    try {
      await onSubmit({
        ...draft,
        author_label: draft.author_label.trim(),
        body: draft.body.trim(),
      });
      setStatus({ ok: true, message: "Réponse publiée sous l’idée." });
    } catch (cause) {
      setStatus({ ok: false, message: errorMessage(cause) });
    } finally {
      setPending(false);
    }
  };

  const describedBy = (key: keyof DraftErrors, extra?: string) =>
    [extra, errors[key] ? `${id}-${key}-error` : undefined].filter(Boolean).join(" ") || undefined;

  return (
    <div className="space-y-6">
      <form
        onSubmit={(event) => {
          void submit(event);
        }}
        noValidate
        className="space-y-4"
        aria-label={`Droit de réponse sur « ${idea.title} »`}
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor={`${id}-author`}>Auteur de la réponse</Label>
            <Input
              id={`${id}-author`}
              value={draft.author_label}
              maxLength={RIGHT_OF_REPLY_AUTHOR_MAX_LENGTH}
              placeholder="Ex. : la direction de l’établissement concerné"
              onChange={(event) => {
                setDraft({ ...draft, author_label: event.target.value });
              }}
              aria-invalid={errors.author_label ? true : undefined}
              aria-describedby={describedBy("author_label")}
            />
            {errors.author_label ? (
              <p id={`${id}-author_label-error`} role="alert" className="text-sm text-destructive">
                {errors.author_label}
              </p>
            ) : null}
          </div>
          <div className="space-y-2">
            <Label htmlFor={`${id}-requested`}>Demande reçue le</Label>
            <Input
              id={`${id}-requested`}
              type="date"
              value={draft.requested_on}
              onChange={(event) => {
                setDraft({ ...draft, requested_on: event.target.value });
              }}
              aria-invalid={errors.requested_on ? true : undefined}
              aria-describedby={describedBy("requested_on", `${id}-deadline`)}
            />
            <p
              id={`${id}-deadline`}
              className={
                overdue ? "text-sm font-medium text-destructive" : "text-sm text-muted-foreground"
              }
            >
              {deadline
                ? `À publier au plus tard le ${formatDate(deadline)}${overdue ? " : délai dépassé." : "."}`
                : "Le délai légal est de 3 jours après la réception."}
            </p>
            {errors.requested_on ? (
              <p id={`${id}-requested_on-error`} role="alert" className="text-sm text-destructive">
                {errors.requested_on}
              </p>
            ) : null}
          </div>
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${id}-body`}>Texte de la réponse (reproduit sans modification)</Label>
          <Textarea
            id={`${id}-body`}
            value={draft.body}
            rows={8}
            onChange={(event) => {
              setDraft({ ...draft, body: event.target.value });
            }}
            aria-invalid={errors.body ? true : undefined}
            aria-describedby={describedBy("body", `${id}-counter`)}
          />
          <p id={`${id}-counter`} className="text-sm text-muted-foreground tabular-nums">
            {formatCharacterCount(ideaTextLength(draft.body), RIGHT_OF_REPLY_MAX_LENGTH)}
          </p>
          {errors.body ? (
            <p id={`${id}-body-error`} role="alert" className="text-sm text-destructive">
              {errors.body}
            </p>
          ) : null}
        </div>
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
            <SendIcon aria-hidden="true" />
          )}
          Publier la réponse
        </Button>
      </form>

      {draft.body.trim() !== "" && draft.author_label.trim() !== "" ? (
        <div className="space-y-2">
          <p className="text-sm font-medium text-muted-foreground">Aperçu</p>
          <RightOfReply
            reply={{
              author_label: draft.author_label.trim(),
              body: draft.body.trim(),
              published_at: today.toISOString().slice(0, 10),
            }}
          />
        </div>
      ) : null}
    </div>
  );
}
