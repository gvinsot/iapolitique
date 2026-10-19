import { ExternalLinkIcon, Loader2Icon } from "lucide-react";
import { type SubmitEvent, useId, useMemo, useState } from "react";
import { Link } from "react-router";

import type {
  AdminEscalation,
  AdminIdea,
  AdminReport,
  AdminReviewRequest,
  EscalationOutcome,
  ReviewResolution,
} from "@/api/types";
import { paths } from "@/app/paths";
import { errorMessage } from "@/api/errors";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Textarea } from "@/components/ui/textarea";
import { useTaxonomy, useTaxonomyLabels } from "@/hooks/use-taxonomy";
import {
  escalationOutcomeLabels,
  hiddenReasonLabels,
  hoursUntil,
  ideaStatusLabels,
  labelFor,
  reportStatusLabels,
  reviewOutcomeLabels,
  reviewResolutionLabels,
  targetKindLabels,
} from "@/lib/admin-model";
import { mailtoHref } from "@/lib/email";
import { formatDateTime, formatEuros, formatInteger } from "@/lib/format";
import { PHAROS_URL } from "@/lib/legal";
import { labelOf } from "@/lib/taxonomy-labels";

import { AsyncActionButton } from "./async-action-button";
import { adminLinkClass, cellClass } from "./admin-styles";
import { AdminIdeaLink, AdminTable, ConfirmDialog } from "./admin-ui";

const REASON_MAX_LENGTH = 1000;
const NOTE_MAX_LENGTH = 2000;

function Muted({ children = "—" }: { children?: string }) {
  return <span className="text-muted-foreground">{children}</span>;
}

/**
 * The reporter's address: a mailto link only when it is a plain address, so that an address
 * smuggling header fields (`?cc=…&body=…`, RFC 6068) never prefills the publisher's reply
 * (finding INJ-7). Otherwise it is shown as text, flagged, for the publisher to copy by hand.
 */
function ReporterEmail({ email }: { email: string }) {
  const href = mailtoHref(email);
  if (href) {
    return (
      <a href={href} className={adminLinkClass}>
        {email}
      </a>
    );
  }
  return (
    <p className="break-all">
      {email}{" "}
      <span className="text-xs text-muted-foreground">(adresse non conforme, pas de lien)</span>
    </p>
  );
}

// --- Ideas -----------------------------------------------------------------------------------

function statusBadgeVariant(status: string): "destructive" | "outline" | "secondary" {
  if (status === "hidden") return "destructive";
  // Withdrawn by its author (owner decision D2): read only, never republished.
  if (status === "withdrawn") return "outline";
  return "secondary";
}

/** Statuses from which the administrator may refuse an idea (AGENTS.md §8). */
const REJECTABLE_STATUSES = [
  "accepted",
  "dedup",
  "queued_analysis",
  "analyzing",
  "analysis_failed",
  "published",
  "hidden",
];

/**
 * Refusal of an idea by the administrator, with a reason of the screening grid: the author sees
 * it as a human decision. Errors of the API stay in the dialog.
 */
function RejectIdeaDialog({
  name,
  published,
  onReject,
}: {
  name: string;
  published: boolean;
  onReject: (reasonCode: string) => Promise<void>;
}) {
  const id = useId();
  const taxonomy = useTaxonomy();
  const reasons = useMemo(() => {
    const seen = new Set<string>();
    return (taxonomy.data?.rejection_reasons ?? []).filter((reason) => {
      if (seen.has(reason.code)) return false;
      seen.add(reason.code);
      return true;
    });
  }, [taxonomy.data]);
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (reason === "") {
      setError("Motif : choisissez un motif de refus.");
      return;
    }
    setPending(true);
    setError(null);
    try {
      await onReject(reason);
      setOpen(false);
      setReason("");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setError(null);
      }}
    >
      <DialogTrigger asChild>
        <Button type="button" size="sm" variant="outline" aria-label={`Refuser « ${name} »`}>
          Refuser
        </Button>
      </DialogTrigger>
      <DialogContent>
        <form
          onSubmit={(event) => {
            void submit(event);
          }}
          noValidate
          className="space-y-4"
        >
          <DialogHeader>
            <DialogTitle>Refuser cette idée</DialogTitle>
            <DialogDescription>
              {published
                ? "L’idée quitte le site et les exports aussitôt. "
                : "Son traitement s’arrête. "}
              Son auteur voit un refus décidé par l’éditeur du site, avec le motif choisi ; l’idée
              est effacée 30 jours plus tard. Elle pourra être acceptée de nouveau d’ici là.
            </DialogDescription>
          </DialogHeader>
          <fieldset className="space-y-2">
            <legend className="mb-2 text-sm font-medium">Motif (montré à l’auteur)</legend>
            {reasons.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {taxonomy.isError
                  ? "Les motifs de la grille sont indisponibles."
                  : "Chargement des motifs…"}
              </p>
            ) : (
              <RadioGroup value={reason} onValueChange={setReason} className="gap-2">
                {reasons.map((item) => (
                  <div key={item.code} className="flex items-start gap-3">
                    <RadioGroupItem
                      value={item.code}
                      id={`${id}-${item.code}`}
                      className="mt-0.5"
                    />
                    <Label htmlFor={`${id}-${item.code}`} className="block font-normal">
                      <span className="font-mono text-xs">{item.code}</span>
                      <span className="block text-sm">{item.message}</span>
                    </Label>
                  </div>
                ))}
              </RadioGroup>
            )}
          </fieldset>
          {error ? (
            <p role="alert" className="text-sm font-medium text-destructive">
              {error}
            </p>
          ) : null}
          <DialogFooter>
            <Button type="submit" variant="destructive" disabled={pending}>
              {pending ? <Loader2Icon className="animate-spin" aria-hidden="true" /> : null}
              Refuser l’idée
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** Bounds of the text of an idea, as for a submission (AGENTS.md §5.1). */
const IDEA_TEXT_MIN = 50;
const IDEA_TEXT_MAX = 4000;

/**
 * Acceptance of a refused idea by the administrator, with its text editable beforehand: a
 * corrected text is saved first (validated and masked like a submission), then the idea is
 * accepted and resumes the pipeline. Errors of the API stay in the dialog.
 */
function AcceptIdeaDialog({
  name,
  text,
  onEditText,
  onAccept,
}: {
  name: string;
  text: string | null;
  onEditText: (text: string) => Promise<void>;
  onAccept: () => Promise<void>;
}) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState(text ?? "");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const length = value.trim().length;
  const changed = text !== null && value.trim() !== text.trim();

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (changed && (length < IDEA_TEXT_MIN || length > IDEA_TEXT_MAX)) {
      setError(`Texte : entre ${IDEA_TEXT_MIN} et ${IDEA_TEXT_MAX} caractères.`);
      return;
    }
    setPending(true);
    setError(null);
    try {
      if (changed) await onEditText(value.trim());
      await onAccept();
      setOpen(false);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) setValue(text ?? "");
        else setError(null);
      }}
    >
      <DialogTrigger asChild>
        <Button type="button" size="sm" variant="outline" aria-label={`Accepter « ${name} »`}>
          Accepter
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <form
          onSubmit={(event) => {
            void submit(event);
          }}
          noValidate
          className="space-y-4"
        >
          <DialogHeader>
            <DialogTitle>Accepter cette idée</DialogTitle>
            <DialogDescription>
              Le refus est levé : l’idée reprend son traitement (dédoublonnage, puis analyse et
              chiffrage par IA, 15 minutes ou plus) et sera publiée à la fin de l’analyse.
            </DialogDescription>
          </DialogHeader>
          {text === null ? (
            <p className="text-sm text-muted-foreground">Le texte de cette idée a été effacé.</p>
          ) : (
            <div className="space-y-2">
              <Label htmlFor={`${id}-text`}>Texte de l’idée (modifiable avant d’accepter)</Label>
              <Textarea
                id={`${id}-text`}
                value={value}
                rows={10}
                maxLength={IDEA_TEXT_MAX}
                onChange={(event) => {
                  setValue(event.target.value);
                }}
                aria-describedby={`${id}-help`}
              />
              <p id={`${id}-help`} className="text-xs text-muted-foreground">
                {length} / {IDEA_TEXT_MAX} caractères. Un texte modifié est validé et masqué comme
                un dépôt, puis publié avec la mention « texte modifié par l’éditeur du site ».
              </p>
            </div>
          )}
          {error ? (
            <p role="alert" className="text-sm font-medium text-destructive">
              {error}
            </p>
          ) : null}
          <DialogFooter>
            <Button type="submit" disabled={pending}>
              {pending ? <Loader2Icon className="animate-spin" aria-hidden="true" /> : null}
              {changed ? "Enregistrer le texte et accepter" : "Accepter l’idée"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

interface IdeasTableProps {
  rows: AdminIdea[];
  onHide: (id: string, reason: string) => Promise<void>;
  onRepublish: (id: string, reason: string) => Promise<void>;
  onReanalyze: (id: string) => Promise<void>;
  onReject: (id: string, reasonCode: string) => Promise<void>;
  onAccept: (id: string) => Promise<void>;
  onEditText: (id: string, text: string) => Promise<void>;
}

/**
 * Ideas of every status (AGENTS.md §8): hide, republish, refuse an accepted idea (published or
 * not), accept a refused one, relaunch an analysis, replies. An idea
 * withdrawn by its author is shown read only: no action applies to it (§5.2).
 */
export function IdeasTable({
  rows,
  onHide,
  onRepublish,
  onReanalyze,
  onReject,
  onAccept,
  onEditText,
}: IdeasTableProps) {
  return (
    <AdminTable
      caption="Idées"
      headers={["Idée", "Statut", "Texte (masqué)", "En attente", "Actions"]}
      empty="Aucune idée ne correspond."
      rowCount={rows.length}
      minWidth="min-w-[56rem]"
    >
      {rows.map((row) => {
        const name = row.title ?? row.id;
        return (
          <tr key={row.id} className="border-b last:border-b-0">
            <td className={`${cellClass} max-w-xs`}>
              <AdminIdeaLink idea={row} markWithdrawn={false} />
              <p className="mt-1 text-xs text-muted-foreground">
                Reçue le {formatDateTime(row.created_at)} · proposée {row.proposal_count} fois
                {row.score_eur !== null ? ` · score ${formatEuros(row.score_eur)}` : null}
              </p>
            </td>
            <td className={cellClass}>
              <Badge variant={statusBadgeVariant(row.status)}>
                {labelFor(ideaStatusLabels, row.status)}
              </Badge>
              <p className="mt-1 text-xs text-muted-foreground">
                depuis le {formatDateTime(row.withdrawn_at ?? row.status_changed_at)}
              </p>
              {row.hidden_reason ? (
                <p className="mt-1 text-xs">{labelFor(hiddenReasonLabels, row.hidden_reason)}</p>
              ) : null}
              {row.rejection_reason_code ? (
                <p className="mt-1 font-mono text-xs">{row.rejection_reason_code}</p>
              ) : null}
            </td>
            <td className={`${cellClass} max-w-sm`}>
              <p className="line-clamp-4 text-pretty whitespace-pre-line">
                {row.excerpt ?? "Texte purgé"}
              </p>
            </td>
            <td className={cellClass}>
              <ul className="space-y-1 text-xs">
                {row.reports_pending > 0 ? (
                  <li>{formatInteger(row.reports_pending)} signalement(s) à revoir</li>
                ) : null}
                {row.review_request_pending ? <li>Réexamen demandé</li> : null}
                {row.escalation_pending ? (
                  <li>
                    <Badge variant="destructive">Escalade</Badge>
                  </li>
                ) : null}
                {row.reports_pending === 0 &&
                !row.review_request_pending &&
                !row.escalation_pending ? (
                  <li>
                    <Muted />
                  </li>
                ) : null}
              </ul>
            </td>
            <td className={cellClass}>
              {row.status === "withdrawn" ? (
                <p className="max-w-48 text-xs text-muted-foreground">
                  Lecture seule : retrait définitif décidé par l’auteur, effacement 12 mois après le
                  retrait.
                </p>
              ) : null}
              <div className="flex flex-wrap gap-2">
                {row.status === "published" ? (
                  <ConfirmDialog
                    trigger="Masquer"
                    triggerLabel={`Masquer « ${name} »`}
                    title="Masquer cette idée"
                    description="L’idée quitte le site et les exports suivants. Elle pourra être republiée."
                    confirm="Masquer l’idée"
                    field={{
                      label: "Motif",
                      required: true,
                      maxLength: REASON_MAX_LENGTH,
                      help: "Consigné dans le journal d’administration, jamais montré à l’auteur.",
                    }}
                    onConfirm={(reason) => onHide(row.id, reason ?? "")}
                  />
                ) : null}
                {row.status === "hidden" ? (
                  <ConfirmDialog
                    trigger="Republier"
                    triggerLabel={`Republier « ${name} »`}
                    title="Republier cette idée"
                    description="L’idée revient sur le site et dans les exports suivants."
                    confirm="Republier l’idée"
                    field={{ label: "Motif", required: true, maxLength: REASON_MAX_LENGTH }}
                    onConfirm={(reason) => onRepublish(row.id, reason ?? "")}
                  />
                ) : null}
                {REJECTABLE_STATUSES.includes(row.status) ? (
                  <RejectIdeaDialog
                    name={name}
                    published={row.status === "published"}
                    onReject={(reasonCode) => onReject(row.id, reasonCode)}
                  />
                ) : null}
                {row.status === "rejected" ? (
                  <AcceptIdeaDialog
                    name={name}
                    text={row.text ?? null}
                    onEditText={(text) => onEditText(row.id, text)}
                    onAccept={() => onAccept(row.id)}
                  />
                ) : null}
                {["published", "hidden", "analysis_failed"].includes(row.status) ? (
                  <AsyncActionButton
                    variant="ghost"
                    onAction={() => onReanalyze(row.id)}
                    aria-label={`Relancer l’analyse de « ${name} »`}
                  >
                    Relancer l’analyse
                  </AsyncActionButton>
                ) : null}
                {row.status === "published" || row.status === "hidden" ? (
                  <Link
                    to={paths.admin.replies(row.id)}
                    className="inline-flex h-8 items-center px-2 text-sm font-medium text-primary underline underline-offset-4"
                    aria-label={`Droits de réponse de « ${name} »`}
                  >
                    Droits de réponse
                  </Link>
                ) : null}
              </div>
            </td>
          </tr>
        );
      })}
    </AdminTable>
  );
}

// --- Reports ---------------------------------------------------------------------------------

/** Reports with the reporter's optional contact (never published) and their AI review. */
export function ReportsTable({ rows }: { rows: AdminReport[] }) {
  const labels = useTaxonomyLabels();
  return (
    <AdminTable
      caption="Signalements"
      headers={["Idée", "Motif et précision", "Signalant", "Statut", "Revue IA"]}
      empty="Aucun signalement ne correspond."
      rowCount={rows.length}
      minWidth="min-w-[56rem]"
    >
      {rows.map((row) => (
        <tr key={row.id} className="border-b last:border-b-0">
          <td className={`${cellClass} max-w-xs`}>
            <AdminIdeaLink idea={row.idea} />
            <p className="mt-1 text-xs text-muted-foreground">
              Reçu le {formatDateTime(row.created_at)}
            </p>
          </td>
          <td className={`${cellClass} max-w-sm`}>
            <p className="font-medium">{labelOf(labels.reportReasons, row.reason)}</p>
            <p className="text-pretty whitespace-pre-line">
              {row.details ?? <Muted>Sans précision</Muted>}
            </p>
          </td>
          <td className={cellClass}>
            {row.reporter_name || row.reporter_email ? (
              <>
                {row.reporter_name ? <p>{row.reporter_name}</p> : null}
                {row.reporter_email ? <ReporterEmail email={row.reporter_email} /> : null}
              </>
            ) : (
              <Muted>Anonyme</Muted>
            )}
            <p className="mt-1 text-xs text-muted-foreground">
              {row.good_faith ? "Bonne foi déclarée" : "Sans déclaration de bonne foi"}
            </p>
          </td>
          <td className={cellClass}>
            <Badge variant="secondary">{labelFor(reportStatusLabels, row.status)}</Badge>
            {row.rejection_reason_code ? (
              <p className="mt-1 font-mono text-xs">{row.rejection_reason_code}</p>
            ) : null}
          </td>
          <td className={`${cellClass} max-w-sm`}>
            {row.review ? (
              <div className="space-y-1">
                <div className="flex flex-wrap gap-1">
                  <Badge variant="secondary">
                    {labelFor(reviewOutcomeLabels, row.review.outcome)}
                  </Badge>
                  {row.review.escalate ? <Badge variant="destructive">Escaladé</Badge> : null}
                </div>
                <p className="text-pretty">{row.review.explanation}</p>
                {row.review.corrections ? (
                  <p className="text-xs text-muted-foreground">
                    Corrections : {row.review.corrections}
                  </p>
                ) : null}
                <p className="text-xs text-muted-foreground">
                  {formatDateTime(row.review.reviewed_at)}
                </p>
              </div>
            ) : (
              <Muted>Pas encore examiné</Muted>
            )}
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}

// --- Escalations -----------------------------------------------------------------------------

interface EscalationsTableProps {
  rows: AdminEscalation[];
  onHandle: (id: string, outcome: EscalationOutcome, note: string | null) => Promise<void>;
  /** Injected for tests; defaults to the current time. */
  now?: Date;
}

/**
 * Serious content hidden at once by the report review (AGENTS.md §7.5): to be examined by a
 * human and, if needed, reported by hand to PHAROS within 24 hours.
 */
export function EscalationsTable({ rows, onHandle, now = new Date() }: EscalationsTableProps) {
  return (
    <div className="space-y-2">
      <p className="text-sm text-muted-foreground">
        Signalement manuel :{" "}
        <a href={PHAROS_URL} target="_blank" rel="noopener noreferrer" className={adminLinkClass}>
          plateforme PHAROS
          <ExternalLinkIcon className="ml-1 inline size-3.5" aria-hidden="true" />
          <span className="sr-only"> (nouvelle fenêtre)</span>
        </a>
        . L’idée reste masquée quelle que soit l’issue ; la republier se fait depuis la liste des
        idées. Une copie scellée du contenu est conservée un an après l’escalade, même si l’auteur
        retire son idée (consultation journalisée).
      </p>
      <AdminTable
        caption="File d’escalade"
        headers={["Idée", "Explication de la revue", "Délai", "Issue", "Actions"]}
        empty="Aucune escalade."
        rowCount={rows.length}
        minWidth="min-w-[52rem]"
      >
        {rows.map((row) => {
          const pending = row.status === "pending";
          const hoursLeft = hoursUntil(row.due_at, now);
          const name = row.idea.title ?? row.idea.id;
          return (
            <tr key={row.id} className="border-b last:border-b-0">
              <td className={`${cellClass} max-w-xs`}>
                <AdminIdeaLink idea={row.idea} />
                <p className="mt-1 text-xs text-muted-foreground">
                  Escaladée le {formatDateTime(row.created_at)}
                </p>
              </td>
              <td className={`${cellClass} max-w-sm text-pretty`}>
                {row.review_explanation ?? <Muted />}
              </td>
              <td className={cellClass}>
                {!pending ? (
                  <Muted />
                ) : hoursLeft < 0 ? (
                  <Badge variant="destructive">Hors délai de 24 h</Badge>
                ) : (
                  `${Math.floor(hoursLeft)} h restantes`
                )}
              </td>
              <td className={cellClass}>
                {pending ? (
                  <Badge variant="outline">À traiter</Badge>
                ) : (
                  <>
                    <p>{labelFor(escalationOutcomeLabels, row.outcome)}</p>
                    <p className="text-xs text-muted-foreground">
                      {formatDateTime(row.handled_at)}
                    </p>
                    {row.note ? <p className="text-xs">{row.note}</p> : null}
                  </>
                )}
              </td>
              <td className={cellClass}>
                <p className="mb-2">
                  <Link
                    to={paths.admin.seal(row.id)}
                    className={adminLinkClass}
                    aria-label={`Copie scellée de « ${name} »`}
                  >
                    Copie scellée
                  </Link>
                </p>
                {pending ? (
                  <div className="flex flex-wrap gap-2">
                    <ConfirmDialog
                      trigger="Transmis à PHAROS"
                      triggerVariant="default"
                      triggerLabel={`Marquer « ${name} » comme transmis à PHAROS`}
                      title="Transmission à PHAROS"
                      description="Confirmez que le contenu a été signalé sur la plateforme PHAROS."
                      confirm="Enregistrer la transmission"
                      field={{
                        label: "Note (date, référence du signalement)",
                        required: false,
                        maxLength: NOTE_MAX_LENGTH,
                      }}
                      onConfirm={(note) => onHandle(row.id, "transmitted", note)}
                    />
                    <ConfirmDialog
                      trigger="Classer sans suite"
                      triggerLabel={`Classer sans suite « ${name} »`}
                      title="Classer sans suite"
                      description="Le contenu ne justifie pas de transmission. L’idée reste masquée."
                      confirm="Classer sans suite"
                      field={{ label: "Note", required: false, maxLength: NOTE_MAX_LENGTH }}
                      onConfirm={(note) => onHandle(row.id, "dismissed", note)}
                    />
                  </div>
                ) : null}
              </td>
            </tr>
          );
        })}
      </AdminTable>
    </div>
  );
}

// --- Review requests -------------------------------------------------------------------------

interface ReviewRequestsTableProps {
  rows: AdminReviewRequest[];
  onResolve: (id: string, resolution: ReviewResolution, note: string | null) => Promise<void>;
}

const measureLabels: Record<string, string> = {
  rejected: "Refus par le filtrage",
  hidden: "Masquage",
};

/** Human review requests sent from tracking links (AGENTS.md §7.1). */
export function ReviewRequestsTable({ rows, onResolve }: ReviewRequestsTableProps) {
  return (
    <AdminTable
      caption="Demandes de réexamen humain"
      headers={["Texte", "Décision contestée", "Message de l’auteur", "Demandée le", "Actions"]}
      empty="Aucune demande de réexamen."
      rowCount={rows.length}
      minWidth="min-w-[56rem]"
    >
      {rows.map((row) => (
        <tr key={row.id} className="border-b last:border-b-0">
          <td className={`${cellClass} max-w-sm`}>
            <p className="font-medium">{labelFor(targetKindLabels, row.target_kind)}</p>
            {row.idea ? (
              <p className="text-xs">
                Idée : <AdminIdeaLink idea={row.idea} />
              </p>
            ) : null}
            <p className="mt-1 line-clamp-6 text-pretty whitespace-pre-line text-muted-foreground">
              {row.text ?? "Texte supprimé"}
            </p>
          </td>
          <td className={cellClass}>
            {row.decision ? (
              <>
                <p>{labelFor(measureLabels, row.decision.measure)}</p>
                {row.decision.reason_code ? (
                  <p className="font-mono text-xs">{row.decision.reason_code}</p>
                ) : null}
              </>
            ) : (
              <Muted>Plus de décision en vigueur</Muted>
            )}
            {row.target_status ? (
              <p className="mt-1 text-xs text-muted-foreground">Statut : {row.target_status}</p>
            ) : null}
          </td>
          <td className={`${cellClass} max-w-xs text-pretty whitespace-pre-line`}>
            {row.message ?? <Muted />}
          </td>
          <td className={cellClass}>{formatDateTime(row.created_at)}</td>
          <td className={cellClass}>
            {row.status === "pending" ? (
              <div className="flex flex-wrap gap-2">
                <ConfirmDialog
                  trigger="Infirmer"
                  triggerVariant="default"
                  triggerLabel={`Infirmer la décision (demande du ${formatDateTime(row.created_at)})`}
                  title="Infirmer la décision automatique"
                  description="Un texte refusé est accepté et reprend son parcours ; une idée masquée est republiée."
                  confirm="Infirmer la décision"
                  field={{
                    label: "Note interne",
                    required: false,
                    maxLength: NOTE_MAX_LENGTH,
                    help: "Jamais montrée à l’auteur.",
                  }}
                  onConfirm={(note) => onResolve(row.id, "overturned", note)}
                />
                <ConfirmDialog
                  trigger="Confirmer"
                  triggerLabel={`Confirmer la décision (demande du ${formatDateTime(row.created_at)})`}
                  title="Confirmer la décision automatique"
                  description="La décision reste en vigueur ; la purge du texte reprend."
                  confirm="Confirmer la décision"
                  field={{
                    label: "Note interne",
                    required: false,
                    maxLength: NOTE_MAX_LENGTH,
                    help: "Jamais montrée à l’auteur.",
                  }}
                  onConfirm={(note) => onResolve(row.id, "confirmed", note)}
                />
              </div>
            ) : (
              <>
                <Badge variant="secondary">
                  {labelFor(reviewResolutionLabels, row.resolution)}
                </Badge>
                <p className="mt-1 text-xs text-muted-foreground">
                  {formatDateTime(row.handled_at)}
                </p>
                {row.note ? <p className="text-xs">{row.note}</p> : null}
              </>
            )}
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}
