import { FileLockIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { AdminEscalationSeal, AdminEscalationSealSummary } from "@/api/types";
import { paths } from "@/app/paths";
import { Badge } from "@/components/ui/badge";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import {
  hiddenReasonLabels,
  ideaStatusLabels,
  labelFor,
  reviewOutcomeLabels,
} from "@/lib/admin-model";
import { analysisText, readSealContent } from "@/lib/admin-seal";
import { formatDateTime } from "@/lib/format";
import { labelOf } from "@/lib/taxonomy-labels";

import { adminLinkClass, cellClass } from "./admin-styles";
import { AdminTable } from "./admin-ui";

const escalationStatusLabels: Record<string, string> = {
  pending: "En attente",
  done: "Traitée",
};

function Muted({ children = "—" }: { children?: string }) {
  return <span className="text-muted-foreground">{children}</span>;
}

/** Sealed copies still kept (owner decision D3), without their content: nothing is logged. */
export function SealsTable({ rows }: { rows: AdminEscalationSealSummary[] }) {
  return (
    <AdminTable
      caption="Copies scellées conservées"
      headers={["Escalade", "Idée", "Statut de l’escalade", "Supprimée après", "Actions"]}
      empty="Aucune copie scellée n’est conservée."
      rowCount={rows.length}
      minWidth="min-w-[52rem]"
    >
      {rows.map((row) => (
        <tr key={row.escalation_id} className="border-b last:border-b-0">
          <td className={cellClass}>
            <p>Escaladée le {formatDateTime(row.escalated_at)}</p>
            <p className="font-mono text-xs text-muted-foreground">{row.escalation_id}</p>
          </td>
          <td className={cellClass}>
            <p className="font-mono text-xs">{row.idea_id}</p>
            <p className="mt-1 text-xs">
              {row.idea_status ? labelFor(ideaStatusLabels, row.idea_status) : "Idée effacée"}
            </p>
          </td>
          <td className={cellClass}>
            {row.escalation_status ? (
              <Badge variant={row.escalation_status === "pending" ? "outline" : "secondary"}>
                {labelFor(escalationStatusLabels, row.escalation_status)}
              </Badge>
            ) : (
              <Muted>Escalade effacée</Muted>
            )}
          </td>
          <td className={cellClass}>{formatDateTime(row.purge_after)}</td>
          <td className={cellClass}>
            <Link
              to={paths.admin.seal(row.escalation_id)}
              className={adminLinkClass}
              aria-label={`Consulter la copie scellée de l’escalade du ${formatDateTime(row.escalated_at)}`}
            >
              Consulter
            </Link>
          </td>
        </tr>
      ))}
    </AdminTable>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[12rem_1fr] sm:gap-4">
      <dt className="text-sm font-medium text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-pretty break-words whitespace-pre-line">{children}</dd>
    </div>
  );
}

function OptionalText({ value }: { value: string | null }) {
  return value === null || value === "" ? <Muted /> : <>{value}</>;
}

function OptionalDate({ value }: { value: string | null }) {
  return value === null ? <Muted /> : <>{formatDateTime(value)}</>;
}

/**
 * Content of a sealed copy, as plain text only (React escapes everything; the analysis is
 * indented JSON in a `<pre>`). It may hold unlawful content: shown to the administrator only,
 * every read is logged by the API (`escalation.seal_read`).
 */
export function SealedCopyView({ seal }: { seal: AdminEscalationSeal }) {
  const labels = useTaxonomyLabels();
  const content = readSealContent(seal.content);
  const { idea, review, reports } = content;
  const analysis = analysisText(content.analysis);

  return (
    <article aria-label="Copie scellée" className="space-y-8">
      <section aria-labelledby="scellement" className="space-y-3 rounded-lg border bg-card p-4">
        <h2 id="scellement" className="flex items-center gap-2 text-lg font-semibold">
          <FileLockIcon className="size-5" aria-hidden="true" />
          Scellement
        </h2>
        <dl className="space-y-2">
          <Field label="Escalade">
            {formatDateTime(seal.escalated_at)}
            <span className="block font-mono text-xs text-muted-foreground">
              {seal.escalation_id}
            </span>
          </Field>
          <Field label="Idée">
            <span className="font-mono text-xs">{seal.idea_id}</span>
          </Field>
          <Field label="Scellée le">{formatDateTime(seal.sealed_at)}</Field>
          <Field label="Supprimée après">{formatDateTime(seal.purge_after)}</Field>
        </dl>
        {content.backfilled ? (
          <p className="text-sm text-muted-foreground">
            Copie constituée après coup par la migration, à partir de ce qui restait de l’escalade :
            certains champs peuvent manquer.
          </p>
        ) : null}
      </section>

      <section aria-labelledby="idee-scellee" className="space-y-3">
        <h2 id="idee-scellee" className="text-lg font-semibold">
          Idée au moment de l’escalade
        </h2>
        {idea ? (
          <dl className="space-y-2 rounded-lg border bg-card p-4">
            <Field label="Statut">
              {idea.status ? labelFor(ideaStatusLabels, idea.status) : <Muted />}
            </Field>
            <Field label="Titre">
              <OptionalText value={idea.title} />
            </Field>
            <Field label="Résumé">
              <OptionalText value={idea.summary} />
            </Field>
            <Field label="Texte original (masqué)">
              <OptionalText value={idea.originalText} />
            </Field>
            <Field label="Territoire indiqué">
              <OptionalText value={idea.submittedTerritory} />
            </Field>
            <Field label="Niveau indiqué">
              <OptionalText value={idea.submittedAdminLevel} />
            </Field>
            <Field label="Adresse (slug)">
              <OptionalText value={idea.slug} />
            </Field>
            <Field label="Déposée le">
              <OptionalDate value={idea.submittedAt} />
            </Field>
            <Field label="Publiée le">
              <OptionalDate value={idea.publishedAt} />
            </Field>
            <Field label="Masquée le">
              <OptionalDate value={idea.hiddenAt} />
            </Field>
            <Field label="Motif du masquage">
              {idea.hiddenReason ? labelFor(hiddenReasonLabels, idea.hiddenReason) : <Muted />}
            </Field>
          </dl>
        ) : (
          <p className="text-muted-foreground">Aucune donnée de l’idée dans la copie.</p>
        )}
      </section>

      <section aria-labelledby="revue-scellee" className="space-y-3">
        <h2 id="revue-scellee" className="text-lg font-semibold">
          Revue qui a déclenché l’escalade
        </h2>
        {review ? (
          <dl className="space-y-2 rounded-lg border bg-card p-4">
            <Field label="Issue">
              {review.outcome ? labelFor(reviewOutcomeLabels, review.outcome) : <Muted />}
              {review.escalate ? (
                <Badge variant="destructive" className="ml-2">
                  Escalade
                </Badge>
              ) : null}
            </Field>
            <Field label="Explication">
              <OptionalText value={review.explanation} />
            </Field>
            <Field label="Modèle et prompt">
              <OptionalText
                value={
                  [review.model, review.promptVersion && `prompt ${review.promptVersion}`]
                    .filter(Boolean)
                    .join(" · ") || null
                }
              />
            </Field>
            <Field label="Examinée le">
              <OptionalDate value={review.reviewedAt} />
            </Field>
          </dl>
        ) : (
          <p className="text-muted-foreground">Aucune revue dans la copie.</p>
        )}
      </section>

      <section aria-labelledby="signalements-scelles" className="space-y-3">
        <h2 id="signalements-scelles" className="text-lg font-semibold">
          Signalements à l’origine de l’escalade
        </h2>
        <p className="text-sm text-muted-foreground">
          Motif et précision masquée seulement : les coordonnées des signalants ne sont jamais
          scellées.
        </p>
        {reports.length === 0 ? (
          <p className="text-muted-foreground">Aucun signalement dans la copie.</p>
        ) : (
          <ul className="space-y-2">
            {reports.map((report, index) => (
              // The sealed reports carry no identifier; their order is fixed by the copy.
              <li key={index} className="rounded-lg border bg-card p-4">
                <p className="font-medium">
                  {report.reason ? labelOf(labels.reportReasons, report.reason) : "Motif inconnu"}
                  {report.reportedAt ? (
                    <span className="font-normal text-muted-foreground">
                      {" "}
                      · {formatDateTime(report.reportedAt)}
                    </span>
                  ) : null}
                </p>
                <p className="text-pretty whitespace-pre-line">
                  {report.details ?? <Muted>Sans précision</Muted>}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="analyse-scellee" className="space-y-3">
        <h2 id="analyse-scellee" className="text-lg font-semibold">
          Analyse en vigueur à l’escalade
        </h2>
        {analysis ? (
          <pre className="max-h-[32rem] overflow-auto rounded-lg border bg-muted/40 p-4 text-xs whitespace-pre-wrap">
            {analysis}
          </pre>
        ) : (
          <p className="text-muted-foreground">L’idée n’avait pas d’analyse.</p>
        )}
      </section>
    </article>
  );
}
