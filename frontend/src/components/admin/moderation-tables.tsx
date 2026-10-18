import { ExternalLinkIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { Badge } from "@/components/ui/badge";
import {
  escalationHoursLeft,
  type EscalationRow,
  type EscalationStatus,
  type HiddenIdeaRow,
  type HiddenOrigin,
  type ReportRow,
  type ReviewRequestRow,
} from "@/lib/admin-model";
import { formatDate } from "@/lib/format";
import { PHAROS_URL } from "@/lib/legal";
import { reportReasonLabels } from "@/lib/labels";
import { ruleForReasonCode } from "@/lib/rules";
import { trackedKindLabels } from "@/lib/tracking-model";

import { AsyncActionButton } from "./async-action-button";

const linkClass = "font-medium text-primary underline underline-offset-4 hover:no-underline";

interface AdminTableProps {
  caption: string;
  headers: string[];
  empty: string;
  rowCount: number;
  children: ReactNode;
}

/** Captioned table, scrollable on small screens, with an empty state. */
function AdminTable({ caption, headers, empty, rowCount, children }: AdminTableProps) {
  return (
    <div className="overflow-x-auto rounded-lg border">
      <table className="w-full min-w-[40rem] border-collapse text-sm">
        <caption className="border-b bg-muted/40 px-4 py-3 text-left font-semibold">
          {caption}
        </caption>
        <thead>
          <tr className="border-b text-left text-muted-foreground">
            {headers.map((header) => (
              <th key={header} scope="col" className="px-4 py-2 font-medium">
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rowCount === 0 ? (
            <tr>
              <td colSpan={headers.length} className="px-4 py-6 text-center text-muted-foreground">
                {empty}
              </td>
            </tr>
          ) : (
            children
          )}
        </tbody>
      </table>
    </div>
  );
}

const cellClass = "px-4 py-3 align-top";

function IdeaLink({ slug, title }: { slug: string; title: string }) {
  return (
    <Link to={paths.idea(slug)} className={linkClass}>
      {title}
    </Link>
  );
}

const hiddenOriginLabels: Record<HiddenOrigin, string> = {
  report_review: "Revue de signalement (IA)",
  escalation: "Escalade",
  admin: "Action admin",
};

interface HiddenIdeasTableProps {
  rows: HiddenIdeaRow[];
  onRepublish: (slug: string) => Promise<void>;
  onReanalyze: (slug: string) => Promise<void>;
}

/** Hidden ideas (AGENTS.md §8): republish or relaunch the analysis. */
export function HiddenIdeasTable({ rows, onRepublish, onReanalyze }: HiddenIdeasTableProps) {
  return (
    <AdminTable
      caption="Idées masquées"
      headers={["Idée", "Masquée le", "Origine", "Motif", "Actions"]}
      empty="Aucune idée masquée."
      rowCount={rows.length}
    >
      {rows.map((row) => (
        <tr key={row.slug} className="border-b last:border-b-0">
          <td className={cellClass}>
            <IdeaLink slug={row.slug} title={row.title} />
          </td>
          <td className={cellClass}>{formatDate(row.hidden_on)}</td>
          <td className={cellClass}>{hiddenOriginLabels[row.origin]}</td>
          <td className={cellClass}>{row.reason}</td>
          <td className={cellClass}>
            <div className="flex flex-wrap gap-2">
              <AsyncActionButton
                onAction={() => onRepublish(row.slug)}
                aria-label={`Republier « ${row.title} »`}
              >
                Republier
              </AsyncActionButton>
              <AsyncActionButton
                onAction={() => onReanalyze(row.slug)}
                variant="ghost"
                aria-label={`Relancer l’analyse de « ${row.title} »`}
              >
                Relancer l’analyse
              </AsyncActionButton>
            </div>
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}

const outcomeLabels = { maintain: "Maintenue", update: "Mise à jour", hide: "Masquée" } as const;

/** Reports and the outcome of their AI review. */
export function ReportsTable({ rows }: { rows: ReportRow[] }) {
  return (
    <AdminTable
      caption="Signalements"
      headers={["Idée", "Motif", "Précision", "Reçu le", "Revue IA"]}
      empty="Aucun signalement."
      rowCount={rows.length}
    >
      {rows.map((row) => (
        <tr key={row.id} className="border-b last:border-b-0">
          <td className={cellClass}>
            <IdeaLink slug={row.idea_slug} title={row.idea_title} />
          </td>
          <td className={cellClass}>{reportReasonLabels[row.reason]}</td>
          <td className={`${cellClass} max-w-xs text-pretty`}>
            {row.details ?? <span className="text-muted-foreground">—</span>}
          </td>
          <td className={cellClass}>{formatDate(row.received_on)}</td>
          <td className={cellClass}>
            <div className="flex flex-wrap gap-1">
              {row.outcome ? (
                <Badge variant="secondary">{outcomeLabels[row.outcome]}</Badge>
              ) : (
                <Badge variant="outline">En attente</Badge>
              )}
              {row.escalate ? <Badge variant="destructive">Escaladé</Badge> : null}
            </div>
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}

const escalationStatusLabels: Record<EscalationStatus, string> = {
  pending: "À traiter",
  transmitted: "Transmis à PHAROS",
  dismissed: "Classé sans suite",
};

interface EscalationsTableProps {
  rows: EscalationRow[];
  onMarkTransmitted: (id: string) => Promise<void>;
  onDismiss: (id: string) => Promise<void>;
  /** Injected for tests; defaults to the current time. */
  now?: Date;
}

/**
 * Serious content hidden at once by the report review (AGENTS.md §7.5): to be examined by a
 * human and, if needed, reported by hand to PHAROS within 24 hours.
 */
export function EscalationsTable({
  rows,
  onMarkTransmitted,
  onDismiss,
  now = new Date(),
}: EscalationsTableProps) {
  return (
    <div className="space-y-2">
      <p className="text-sm text-muted-foreground">
        Signalement manuel :{" "}
        <a href={PHAROS_URL} rel="noreferrer" className={linkClass}>
          plateforme PHAROS
          <ExternalLinkIcon className="ml-1 inline size-3.5" aria-hidden="true" />
        </a>
      </p>
      <AdminTable
        caption="File d’escalade"
        headers={["Idée", "Explication de la revue", "Délai", "Statut", "Actions"]}
        empty="Aucune escalade."
        rowCount={rows.length}
      >
        {rows.map((row) => {
          const hoursLeft = escalationHoursLeft(row.escalated_at, now);
          const pending = row.status === "pending";
          return (
            <tr key={row.id} className="border-b last:border-b-0">
              <td className={cellClass}>
                <IdeaLink slug={row.idea_slug} title={row.idea_title} />
              </td>
              <td className={`${cellClass} max-w-sm text-pretty`}>{row.explanation}</td>
              <td className={cellClass}>
                {!pending ? (
                  "—"
                ) : hoursLeft < 0 ? (
                  <Badge variant="destructive">Hors délai de 24 h</Badge>
                ) : (
                  `${Math.floor(hoursLeft)} h restantes`
                )}
              </td>
              <td className={cellClass}>{escalationStatusLabels[row.status]}</td>
              <td className={cellClass}>
                {pending ? (
                  <div className="flex flex-wrap gap-2">
                    <AsyncActionButton
                      onAction={() => onMarkTransmitted(row.id)}
                      variant="default"
                      aria-label={`Marquer « ${row.idea_title} » comme transmis à PHAROS`}
                    >
                      Marquer comme transmis
                    </AsyncActionButton>
                    <AsyncActionButton
                      onAction={() => onDismiss(row.id)}
                      variant="ghost"
                      aria-label={`Classer sans suite « ${row.idea_title} »`}
                    >
                      Classer sans suite
                    </AsyncActionButton>
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

interface ReviewRequestsTableProps {
  rows: ReviewRequestRow[];
  /** The automated decision is confirmed. */
  onUphold: (id: string) => Promise<void>;
  /** The automated decision is reversed (text accepted, or idea republished). */
  onOverturn: (id: string) => Promise<void>;
}

/** Human review requests sent from tracking links (AGENTS.md §7.1). */
export function ReviewRequestsTable({ rows, onUphold, onOverturn }: ReviewRequestsTableProps) {
  return (
    <AdminTable
      caption="Demandes de réexamen humain"
      headers={["Envoi", "Décision contestée", "Message", "Demandé le", "Actions"]}
      empty="Aucune demande de réexamen."
      rowCount={rows.length}
    >
      {rows.map((row) => {
        const rule = ruleForReasonCode(row.reason_code);
        return (
          <tr key={row.id} className="border-b last:border-b-0">
            <td className={`${cellClass} max-w-sm`}>
              <p className="font-medium">{trackedKindLabels[row.kind]}</p>
              <p className="line-clamp-4 text-pretty whitespace-pre-line text-muted-foreground">
                {row.text ?? "Texte supprimé"}
              </p>
            </td>
            <td className={cellClass}>
              {rule ? `Règle ${rule.number} : ${rule.title}` : row.reason_code}
            </td>
            <td className={`${cellClass} max-w-xs text-pretty`}>
              {row.message ?? <span className="text-muted-foreground">—</span>}
            </td>
            <td className={cellClass}>{formatDate(row.requested_on)}</td>
            <td className={cellClass}>
              {row.status === "pending" ? (
                <div className="flex flex-wrap gap-2">
                  <AsyncActionButton
                    onAction={() => onOverturn(row.id)}
                    variant="default"
                    aria-label={`Infirmer la décision (demande du ${formatDate(row.requested_on)})`}
                  >
                    Infirmer
                  </AsyncActionButton>
                  <AsyncActionButton
                    onAction={() => onUphold(row.id)}
                    aria-label={`Confirmer la décision (demande du ${formatDate(row.requested_on)})`}
                  >
                    Confirmer
                  </AsyncActionButton>
                </div>
              ) : (
                <Badge variant="secondary">Traitée</Badge>
              )}
            </td>
          </tr>
        );
      })}
    </AdminTable>
  );
}
