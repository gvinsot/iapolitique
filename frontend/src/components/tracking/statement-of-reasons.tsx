import { ScaleIcon } from "lucide-react";
import { useId } from "react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { formatDate } from "@/lib/format";
import {
  RULES_ARTICLE_CRITERIA_ANCHOR,
  RULES_RECOURSE_ANCHOR,
  ruleForReasonCode,
} from "@/lib/rules";
import type { TrackedDecision, TrackedKind } from "@/lib/tracking-model";

const linkClass = "font-medium text-primary underline underline-offset-4 hover:no-underline";

interface StatementOfReasonsProps {
  kind: TrackedKind;
  /** Refusal by the screening, or masking of a published idea after a report review. */
  measure: "rejected" | "hidden";
  decision: TrackedDecision;
  /** The text was already erased (purge after the retention period). */
  purged?: boolean;
}

const measures = {
  rejected: {
    idea: "Votre idée n’est pas publiée.",
    contribution: "Votre contribution n’est pas prise en compte et n’est pas publiée.",
    report: "Votre signalement n’est pas examiné.",
  },
  hidden: {
    idea: "L’idée est masquée : retirée du site et des exports suivants.",
    contribution: "La contribution est masquée.",
    report: "Le contenu signalé est masqué.",
  },
} as const;

/**
 * Statement of reasons of an automated decision (DSA art. 17, AGENTS.md §7.1): measure taken,
 * rule at stake (article of /regles), automated decision notice, erasure date and recourse.
 */
export function StatementOfReasons({
  kind,
  measure,
  decision,
  purged = false,
}: StatementOfReasonsProps) {
  const headingId = useId();
  const rule = measure === "rejected" ? ruleForReasonCode(decision.reason_code) : null;
  const ruleHref = rule
    ? `${paths.terms}#${rule.anchor}`
    : `${paths.terms}#${measure === "rejected" ? RULES_ARTICLE_CRITERIA_ANCHOR : "mesures"}`;
  const ruleLabel = rule
    ? `Règle ${rule.number} : ${rule.title}`
    : measure === "rejected"
      ? "Article 4 des règles : critères d’acceptation et de rejet"
      : "Article 7 des règles : mesures et durées";

  return (
    <section aria-labelledby={headingId} className="space-y-4 rounded-lg border bg-card p-4 sm:p-5">
      <h3 id={headingId} className="flex items-center gap-2 font-semibold">
        <ScaleIcon className="size-4" aria-hidden="true" />
        Exposé des motifs
      </h3>
      <dl className="grid gap-x-4 gap-y-3 text-sm sm:grid-cols-[11rem_minmax(0,1fr)]">
        <dt className="font-medium text-muted-foreground">Mesure prise</dt>
        <dd>{measures[measure][kind]}</dd>

        <dt className="font-medium text-muted-foreground">Motif</dt>
        <dd className="text-pretty">{decision.reason_message}</dd>

        <dt className="font-medium text-muted-foreground">Règle en cause</dt>
        <dd>
          <Link to={ruleHref} className={linkClass}>
            {ruleLabel}
          </Link>
        </dd>

        <dt className="font-medium text-muted-foreground">Mode de décision</dt>
        <dd>
          <strong>Décision prise automatiquement par une IA</strong>
          {measure === "rejected"
            ? decision.second_opinion
              ? " : modèle de filtrage de l’éditeur, puis second avis d’un modèle de Mistral AI."
              : " : modèle de filtrage exécuté sur les serveurs de l’éditeur."
            : " : examen d’un signalement par un modèle de Mistral AI."}{" "}
          Aucune personne n’a relu cette décision.
        </dd>

        <dt className="font-medium text-muted-foreground">Date de la décision</dt>
        <dd>{formatDate(decision.decided_on)}</dd>

        <dt className="font-medium text-muted-foreground">Suppression du texte</dt>
        <dd>
          {purged
            ? "Le texte a déjà été supprimé à l’issue du délai de conservation."
            : decision.purge_on
              ? `Le texte sera supprimé le ${formatDate(decision.purge_on)}.`
              : "Aucune suppression automatique n’est programmée."}
        </dd>

        <dt className="font-medium text-muted-foreground">Voies de recours</dt>
        <dd>
          <ul className="list-disc space-y-1 pl-5">
            {purged ? null : <li>demander un réexamen humain avec le bouton ci-dessous ;</li>}
            <li>
              écrire à l’éditeur (
              <Link to={`${paths.legalNotice}#signalement`} className={linkClass}>
                point de contact
              </Link>
              ) ;
            </li>
            <li>saisir la juridiction française compétente.</li>
          </ul>
          <p className="mt-2">
            <Link to={`${paths.terms}#${RULES_RECOURSE_ANCHOR}`} className={linkClass}>
              Voies de recours détaillées
            </Link>
            {kind === "idea" && measure === "rejected" ? (
              <>
                {" · "}
                <Link to={paths.propose} className={linkClass}>
                  Reformuler et proposer à nouveau
                </Link>
              </>
            ) : null}
          </p>
        </dd>
      </dl>
    </section>
  );
}
