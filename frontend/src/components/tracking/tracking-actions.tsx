import { Loader2Icon, MessageSquareWarningIcon, Trash2Icon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";
import { Link } from "react-router";

import { errorMessage } from "@/api/errors";
import { paths } from "@/app/paths";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { formatInteger } from "@/lib/format";
import { formatCharacterCount, ideaTextLength } from "@/lib/idea-text";
import {
  canDelete,
  canRequestReview,
  deletionWithdraws,
  type TrackedItem,
} from "@/lib/tracking-model";

export const REVIEW_MESSAGE_MAX_LENGTH = 1000;

interface TrackingActionsProps {
  item: TrackedItem;
  /** Erases the text; rejects with an ApiError on failure. */
  onDelete: () => Promise<void>;
  /** Queues a human review; rejects with an ApiError on failure. */
  onRequestReview: (message: string | null) => Promise<void>;
}

/**
 * Author actions of the tracking page (AGENTS.md §7.1): deletion (immediate erasure, or
 * withdrawal of a published idea, owner decision D2), confirmed in an accessible dialog (never
 * window.confirm), and a human review request with an optional message.
 */
export function TrackingActions({ item, onDelete, onRequestReview }: TrackingActionsProps) {
  const headingId = useId();
  const deletable = canDelete(item);
  const reviewable = canRequestReview(item);
  if (!deletable && !reviewable) return null;

  return (
    <section aria-labelledby={headingId} className="space-y-4 rounded-lg border bg-card p-4 sm:p-5">
      <h2 id={headingId} className="text-lg font-semibold">
        Vos actions
      </h2>
      {reviewable ? <ReviewRequest onRequestReview={onRequestReview} /> : null}
      {deletable ? (
        <DeleteContribution onDelete={onDelete} withdraws={deletionWithdraws(item)} />
      ) : null}
    </section>
  );
}

const linkClass = "font-medium underline underline-offset-4 hover:no-underline";

const DELETION_TEXTS = {
  erase: {
    intro:
      "La suppression efface immédiatement votre texte du site et le retire des exports suivants. Les copies déjà téléchargées par des tiers ne peuvent pas être retirées.",
    dialog:
      "Votre texte sera effacé immédiatement et ne figurera plus dans les prochains exports. Cette action ne peut pas être annulée.",
    done: "Votre texte a été supprimé.",
  },
  withdraw: {
    intro:
      "La suppression retire immédiatement votre idée du site et des exports suivants, avec son analyse, ses contributions, ses signalements et ses droits de réponse. Elle reste visible de l’éditeur seul, marquée « retirée par son auteur », puis est effacée 12 mois après le retrait. Les copies déjà téléchargées par des tiers ne peuvent pas être retirées.",
    dialog:
      "Votre idée quittera immédiatement le site et les prochains exports, puis sera effacée 12 mois après son retrait. Le retrait est définitif : ni vous ni l’éditeur ne pourrez la republier.",
    done: "Votre idée a été retirée du site.",
  },
} as const;

function DeleteContribution({
  onDelete,
  withdraws,
}: {
  onDelete: () => Promise<void>;
  /** A published or hidden idea is withdrawn rather than erased. */
  withdraws: boolean;
}) {
  const texts = DELETION_TEXTS[withdraws ? "withdraw" : "erase"];
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  const confirm = async () => {
    setPending(true);
    setError(null);
    try {
      await onDelete();
      setDone(true);
      setOpen(false);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  if (done) {
    return (
      <p role="status" className="text-sm font-medium">
        {texts.done}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      <p className="text-sm text-muted-foreground">
        {texts.intro}{" "}
        {withdraws ? (
          <Link to={`${paths.privacy}#conservation`} className={linkClass}>
            Durées de conservation
          </Link>
        ) : null}
      </p>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!pending) setOpen(next);
          if (!next) setError(null);
        }}
      >
        <DialogTrigger asChild>
          <Button type="button" variant="outline">
            <Trash2Icon aria-hidden="true" />
            Supprimer ma contribution
          </Button>
        </DialogTrigger>
        <DialogContent showCloseButton={!pending}>
          <DialogHeader>
            <DialogTitle>
              {withdraws ? "Retirer définitivement votre idée ?" : "Supprimer définitivement ?"}
            </DialogTitle>
            <DialogDescription>{texts.dialog}</DialogDescription>
          </DialogHeader>
          {error ? (
            <p role="alert" className="text-sm font-medium text-destructive">
              {error}
            </p>
          ) : null}
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline" disabled={pending}>
                Annuler
              </Button>
            </DialogClose>
            <Button
              type="button"
              variant="destructive"
              disabled={pending}
              onClick={() => {
                void confirm();
              }}
            >
              {pending ? (
                <Loader2Icon className="animate-spin" aria-hidden="true" />
              ) : (
                <Trash2Icon aria-hidden="true" />
              )}
              Supprimer définitivement
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function ReviewRequest({
  onRequestReview,
}: {
  onRequestReview: (message: string | null) => Promise<void>;
}) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const length = ideaTextLength(message);
  const tooLong = length > REVIEW_MESSAGE_MAX_LENGTH;

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (pending) return;
    if (tooLong) {
      setError(
        `Votre message ne doit pas dépasser ${formatInteger(REVIEW_MESSAGE_MAX_LENGTH)} caractères.`,
      );
      return;
    }
    setPending(true);
    setError(null);
    try {
      const trimmed = message.trim();
      await onRequestReview(trimmed === "" ? null : trimmed);
      setDone(true);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  if (done) {
    return (
      <p role="status" className="text-sm font-medium">
        Votre demande de réexamen humain est enregistrée. Sa suite apparaîtra sur cette page.
      </p>
    );
  }

  if (!open) {
    return (
      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">
          Vous contestez cette décision automatique ? Une personne peut la réexaminer.
        </p>
        <Button
          type="button"
          onClick={() => {
            setOpen(true);
          }}
        >
          <MessageSquareWarningIcon aria-hidden="true" />
          Demander un réexamen humain
        </Button>
      </div>
    );
  }

  return (
    <form
      onSubmit={(event) => {
        void submit(event);
      }}
      noValidate
      className="space-y-3"
      aria-label="Demande de réexamen humain"
    >
      <div className="space-y-2">
        <Label htmlFor={`${id}-message`}>Votre message (facultatif)</Label>
        <p id={`${id}-help`} className="text-sm text-muted-foreground">
          Expliquez pourquoi la décision vous semble erronée. Ne donnez aucune information
          personnelle.
        </p>
        <Textarea
          id={`${id}-message`}
          value={message}
          onChange={(event) => {
            setMessage(event.target.value);
          }}
          rows={4}
          aria-invalid={tooLong ? true : undefined}
          aria-describedby={`${id}-help ${id}-counter`}
        />
        <p
          id={`${id}-counter`}
          className={tooLong ? "text-sm text-destructive" : "text-sm text-muted-foreground"}
        >
          {formatCharacterCount(length, REVIEW_MESSAGE_MAX_LENGTH)}
        </p>
      </div>
      {error ? (
        <p role="alert" className="text-sm font-medium text-destructive">
          {error}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={pending}>
          {pending ? <Loader2Icon className="animate-spin" aria-hidden="true" /> : null}
          Envoyer la demande
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={pending}
          onClick={() => {
            setOpen(false);
            setError(null);
          }}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
