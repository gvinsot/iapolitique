import { PencilIcon } from "lucide-react";
import { useState } from "react";

import type {
  AdminAction,
  AdminAiCosts,
  AdminJob,
  AdminReply,
  AdminReplyContent,
} from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  aiPurposeLabels,
  jobKindLabels,
  jobStatusLabels,
  labelFor,
  reasoningEffortLabels,
  rightOfReplyDeadline,
} from "@/lib/admin-model";
import { formatDate, formatDateTime, formatEurosPrecise, formatInteger } from "@/lib/format";

import { cellClass } from "./admin-styles";
import { AdminTable, ConfirmDialog } from "./admin-ui";
import { AsyncActionButton } from "./async-action-button";
import { JobLogsDialog } from "./job-logs-dialog";
import { RightOfReplyEditor } from "./right-of-reply-editor";

/** Payloads and details hold internal identifiers only (AGENTS.md §7.1): shown as JSON. */
function JsonValue({ value }: { value: unknown }) {
  if (value === null || value === undefined)
    return <span className="text-muted-foreground">—</span>;
  const text = JSON.stringify(value);
  return text === "{}" ? (
    <span className="text-muted-foreground">—</span>
  ) : (
    <code className="block max-w-xs text-xs break-all whitespace-pre-wrap">{text}</code>
  );
}

// --- Jobs ------------------------------------------------------------------------------------

/** The job queue (AGENTS.md §8): the logs of each job; a dead job can be queued again. */
export function JobsTable({
  rows,
  onRetry,
}: {
  rows: AdminJob[];
  onRetry: (id: number) => Promise<void>;
}) {
  return (
    <AdminTable
      caption="Jobs"
      headers={["Job", "Statut", "Tentatives", "Charge utile", "Dernière erreur", "Actions"]}
      empty="Aucun job ne correspond."
      rowCount={rows.length}
      minWidth="min-w-[60rem]"
    >
      {rows.map((job) => (
        <tr key={job.id} className="border-b last:border-b-0">
          <td className={cellClass}>
            <p className="font-medium">{labelFor(jobKindLabels, job.kind)}</p>
            <p className="font-mono text-xs text-muted-foreground">
              n° {job.id} · priorité {job.priority}
            </p>
            <p className="text-xs text-muted-foreground">
              Créé le {formatDateTime(job.created_at)}
            </p>
          </td>
          <td className={cellClass}>
            <Badge variant={job.status === "dead" ? "destructive" : "secondary"}>
              {labelFor(jobStatusLabels, job.status)}
            </Badge>
            <p className="mt-1 text-xs text-muted-foreground">
              {job.status === "queued"
                ? `prévu le ${formatDateTime(job.run_after)}`
                : job.finished_at
                  ? `fini le ${formatDateTime(job.finished_at)}`
                  : `mis à jour le ${formatDateTime(job.updated_at)}`}
            </p>
          </td>
          <td className={`${cellClass} tabular-nums`}>
            {job.attempts} / {job.max_attempts}
          </td>
          <td className={cellClass}>
            <JsonValue value={job.payload} />
          </td>
          <td className={`${cellClass} max-w-xs text-xs break-words`}>
            {job.last_error ?? <span className="text-muted-foreground">—</span>}
          </td>
          <td className={cellClass}>
            <div className="flex flex-wrap gap-2">
              <JobLogsDialog job={job} />
              {job.status === "dead" ? (
                <AsyncActionButton
                  onAction={() => onRetry(job.id)}
                  aria-label={`Relancer le job n° ${job.id}`}
                >
                  Relancer
                </AsyncActionButton>
              ) : null}
            </div>
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}

// --- AI costs --------------------------------------------------------------------------------

/** Calls and estimated costs per purpose, for the Paris day and month (AGENTS.md §6.4). */
export function CostsTable({ costs }: { costs: AdminAiCosts }) {
  return (
    <AdminTable
      caption="Appels et coûts estimés par usage"
      headers={[
        "Usage",
        "Fournisseur",
        "Raisonnement",
        "Appels (jour)",
        "Coût (jour)",
        "Appels (mois)",
        "Coût (mois)",
      ]}
      empty="Aucun appel IA ce mois-ci."
      rowCount={costs.by_purpose.length}
    >
      {costs.by_purpose.map((row) => (
        <tr
          key={`${row.provider}-${row.purpose}-${row.reasoning_effort ?? ""}`}
          className="border-b tabular-nums last:border-b-0"
        >
          <th scope="row" className="px-4 py-2 text-left font-medium">
            {labelFor(aiPurposeLabels, row.purpose)}
          </th>
          <td className="px-4 py-2">
            {row.provider === "mistral" ? "Mistral AI" : "Modèle de décision (local)"}
          </td>
          <td className="px-4 py-2">{labelFor(reasoningEffortLabels, row.reasoning_effort)}</td>
          <td className="px-4 py-2">{formatInteger(row.calls_today)}</td>
          <td className="px-4 py-2">{formatEurosPrecise(row.today_eur)}</td>
          <td className="px-4 py-2">{formatInteger(row.calls_month)}</td>
          <td className="px-4 py-2">{formatEurosPrecise(row.month_eur)}</td>
        </tr>
      ))}
    </AdminTable>
  );
}

// --- Journal ---------------------------------------------------------------------------------

/** Every administrative action, most recent first (AGENTS.md §8: `admin_actions`). */
export function ActionsTable({ rows }: { rows: AdminAction[] }) {
  return (
    <AdminTable
      caption="Journal des actions"
      headers={["Date", "Action", "Cible", "Détails"]}
      empty="Aucune action ne correspond."
      rowCount={rows.length}
    >
      {rows.map((row) => (
        <tr key={row.id} className="border-b last:border-b-0">
          <td className={cellClass}>{formatDateTime(row.created_at)}</td>
          <td className={`${cellClass} font-mono text-xs`}>{row.action}</td>
          <td className={cellClass}>
            {row.target_kind ? <p>{row.target_kind}</p> : null}
            {row.target_id ? (
              <p className="font-mono text-xs break-all text-muted-foreground">{row.target_id}</p>
            ) : null}
          </td>
          <td className={cellClass}>
            <JsonValue value={row.details} />
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}

// --- Rights of reply -------------------------------------------------------------------------

interface RepliesListProps {
  ideaTitle: string;
  replies: AdminReply[];
  onUpdate: (id: string, content: AdminReplyContent) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

function ReplyItem({
  reply,
  ideaTitle,
  onUpdate,
  onDelete,
}: { reply: AdminReply } & Omit<RepliesListProps, "replies">) {
  const [editing, setEditing] = useState(false);
  return (
    <li className="space-y-3 rounded-lg border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <p className="font-medium">Auteur : {reply.author_label}</p>
          <p className="text-xs text-muted-foreground">
            Demande reçue le {formatDate(reply.requested_on)} · à publier avant le{" "}
            {formatDate(reply.due_on || rightOfReplyDeadline(reply.requested_on))} · publiée le{" "}
            {formatDateTime(reply.published_at)}
            {reply.updated_at !== reply.published_at
              ? ` · modifiée le ${formatDateTime(reply.updated_at)}`
              : null}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            aria-expanded={editing}
            onClick={() => {
              setEditing(!editing);
            }}
          >
            <PencilIcon aria-hidden="true" />
            {editing ? "Fermer" : "Modifier"}
          </Button>
          <ConfirmDialog
            trigger="Retirer"
            triggerVariant="ghost"
            triggerLabel={`Retirer la réponse (auteur : ${reply.author_label})`}
            title="Retirer ce droit de réponse"
            description="La réponse disparaît de la page de l’idée. L’action est consignée au journal."
            confirm="Retirer la réponse"
            onConfirm={() => onDelete(reply.id)}
          />
        </div>
      </div>
      {editing ? (
        <RightOfReplyEditor
          ideaTitle={ideaTitle}
          initial={{
            author_label: reply.author_label,
            body: reply.body,
            requested_on: reply.requested_on,
          }}
          submitLabel="Enregistrer les modifications"
          successMessage="Réponse modifiée."
          onSubmit={(content) => onUpdate(reply.id, content)}
        />
      ) : (
        <blockquote className="border-l-2 pl-4 text-sm text-pretty whitespace-pre-line">
          {reply.body}
        </blockquote>
      )}
    </li>
  );
}

/** Rights of reply already published under an idea, editable and removable. */
export function RepliesList({ replies, ...props }: RepliesListProps) {
  if (replies.length === 0) {
    return <p className="text-muted-foreground">Aucun droit de réponse publié sous cette idée.</p>;
  }
  return (
    <ul className="space-y-4" aria-label="Droits de réponse publiés">
      {replies.map((reply) => (
        <ReplyItem key={reply.id} reply={reply} {...props} />
      ))}
    </ul>
  );
}
