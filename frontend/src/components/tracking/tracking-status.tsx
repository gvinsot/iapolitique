import {
  ArchiveXIcon,
  CheckCircle2Icon,
  CircleDashedIcon,
  EyeOffIcon,
  HourglassIcon,
  type LucideIcon,
  MergeIcon,
  SearchCheckIcon,
  Trash2Icon,
  XCircleIcon,
} from "lucide-react";
import { useId } from "react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { formatDate } from "@/lib/format";
import {
  canRequestReview,
  ideaWithdrawn,
  type ReviewOutcome,
  type TrackedItem,
  type TrackedKind,
  type TrackingPhase,
  trackingPhase,
} from "@/lib/tracking-model";
import { cn } from "@/lib/utils";

import { StatementOfReasons } from "./statement-of-reasons";

interface PhaseText {
  title: string;
  description: string;
  icon: LucideIcon;
  tone: "progress" | "success" | "neutral" | "warning";
}

const REJECTED_TITLES: Record<TrackedKind, string> = {
  idea: "Idée non retenue",
  contribution: "Contribution non retenue",
  report: "Signalement non retenu",
};

const SUBMITTED_LABELS: Record<TrackedKind, string> = {
  idea: "Idée envoyée",
  contribution: "Contribution envoyée",
  report: "Signalement envoyé",
};

const REPORT_OUTCOMES: Record<ReviewOutcome, string> = {
  maintain:
    "L’idée est maintenue en l’état : l’examen n’a pas relevé de problème justifiant une modification.",
  update: "L’analyse de l’idée va être refaite en tenant compte de votre signalement.",
  hide: "L’idée a été masquée du site et des exports suivants.",
};

function phaseText(item: TrackedItem, phase: TrackingPhase): PhaseText {
  const { kind } = item;
  switch (phase) {
    case "pending":
      return {
        title: "En cours de filtrage",
        description:
          kind === "report"
            ? "Accusé de réception : votre signalement est enregistré et en cours de filtrage automatique."
            : "Votre texte est en cours d’examen par le modèle de filtrage. Si le service est momentanément indisponible, l’examen est relancé automatiquement : revenez un peu plus tard.",
        icon: HourglassIcon,
        tone: "progress",
      };
    case "second_opinion":
      return {
        title: "En cours de vérification",
        description:
          "Le premier filtrage n’a pas permis de trancher : un second avis est demandé à un modèle de Mistral AI avec la même grille. La réponse apparaîtra ici.",
        icon: HourglassIcon,
        tone: "progress",
      };
    case "accepted":
      return kind === "idea"
        ? {
            title: "Idée conservée",
            description:
              "Votre idée est conservée, elle va être améliorée et chiffrée par IA. Elle rejoindra le classement dès la fin de son analyse.",
            icon: CheckCircle2Icon,
            tone: "success",
          }
        : {
            title: "Contribution acceptée",
            description:
              "Votre contribution sera prise en compte lors de la prochaine analyse de l’idée, qui sera refaite automatiquement.",
            icon: CheckCircle2Icon,
            tone: "success",
          };
    case "published":
      return {
        title: "Idée publiée",
        description: "Votre idée a été analysée par IA et figure dans le classement public.",
        icon: CheckCircle2Icon,
        tone: "success",
      };
    case "merged":
      return {
        title: "Rattachée à une idée existante",
        description:
          "Une idée très proche était déjà publiée : la vôtre lui a été rattachée comme contribution et son compteur « proposée N fois » a augmenté. L’idée sera analysée à nouveau en tenant compte de votre texte.",
        icon: MergeIcon,
        tone: "success",
      };
    case "integrated":
      return {
        title: "Contribution prise en compte",
        description: "La dernière analyse de l’idée a examiné votre contribution.",
        icon: CheckCircle2Icon,
        tone: "success",
      };
    case "received":
      return {
        title: "Signalement reçu",
        description:
          "Accusé de réception : votre signalement a passé le filtrage. Il sera examiné par IA avec les autres signalements de cette idée, au plus une fois par 24 heures ; un contenu problématique est examiné sans délai.",
        icon: SearchCheckIcon,
        tone: "progress",
      };
    case "reviewed":
      return {
        title: "Signalement examiné",
        description: item.report_outcome
          ? REPORT_OUTCOMES[item.report_outcome]
          : "Votre signalement a été examiné.",
        icon: SearchCheckIcon,
        tone: "success",
      };
    case "rejected":
      return {
        title: REJECTED_TITLES[kind],
        description:
          item.status === "purged"
            ? "Le texte a été supprimé à l’issue du délai de conservation de 30 jours."
            : "Le filtrage automatique n’a pas retenu ce texte. Le motif et les voies de recours sont détaillés ci-dessous.",
        icon: XCircleIcon,
        tone: "warning",
      };
    case "hidden":
      return {
        title: "Idée masquée",
        description:
          "À la suite d’un signalement ou d’une décision de l’éditeur, l’idée n’est plus visible sur le site.",
        icon: EyeOffIcon,
        tone: "warning",
      };
    case "deleted":
      return {
        title: "Supprimé à votre demande",
        description:
          "Votre suppression est faite : le texte n’est plus visible sur le site et n’apparaîtra plus dans les exports suivants. Un texte non publié est effacé aussitôt ; une idée publiée est retirée du site, n’est plus visible que de l’éditeur, puis est effacée 12 mois après son retrait. Ce lien de suivi ne sert plus à rien.",
        icon: Trash2Icon,
        tone: "neutral",
      };
    case "unknown":
      return {
        title: "Statut momentanément illisible",
        description:
          "Le statut de votre envoi n’a pas pu être interprété par cette version du site. Rechargez la page dans quelques instants.",
        icon: CircleDashedIcon,
        tone: "neutral",
      };
  }
}

const toneClasses: Record<PhaseText["tone"], string> = {
  progress: "border-warning/40 bg-warning-surface text-warning",
  success: "border-primary/30 bg-accent text-accent-foreground",
  neutral: "bg-card",
  warning: "border-warning/40 bg-warning-surface text-warning",
};

const linkClass = "font-medium underline underline-offset-4 hover:no-underline";

const WITHDRAWN_TEXTS: Record<TrackedKind, string> = {
  idea: "L’idée à laquelle la vôtre avait été rattachée a été supprimée par son auteur : elle n’est plus visible sur le site ni dans les exports, avec tout ce qui s’y rattache.",
  contribution:
    "L’idée sur laquelle portait votre contribution a été supprimée par son auteur : elle n’est plus visible sur le site ni dans les exports, avec ses contributions.",
  report:
    "L’idée que vous avez signalée a été supprimée par son auteur : elle n’est plus visible sur le site ni dans les exports.",
};

/** The idea concerned was withdrawn by its author (owner decision D2, AGENTS.md §7.1). */
function IdeaWithdrawnNotice({ kind }: { kind: TrackedKind }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-2 rounded-lg border bg-card p-4 sm:p-5">
      <h2 id={headingId} className="flex items-center gap-2 text-lg font-semibold">
        <ArchiveXIcon className="size-5 shrink-0" aria-hidden="true" />
        Idée retirée par son auteur
      </h2>
      <p className="text-pretty">{WITHDRAWN_TEXTS[kind]}</p>
      <p className="text-sm text-pretty text-muted-foreground">
        Votre texte garde son statut ci-dessus. Il est effacé avec l’idée 12 mois après son retrait
        ; vous pouvez le supprimer dès maintenant. Un réexamen humain n’est plus possible.
      </p>
    </section>
  );
}

/** Current state of a submission seen through its tracking link. */
export function TrackingStatus({ item }: { item: TrackedItem }) {
  const headingId = useId();
  const phase = trackingPhase(item);
  const text = phaseText(item, phase);
  const ideaLinkLabel =
    phase === "published"
      ? "Voir l’idée publiée"
      : phase === "merged"
        ? "Voir l’idée à laquelle elle est rattachée"
        : "Voir l’idée concernée";
  const showIdeaLink = item.idea !== null && phase !== "deleted" && phase !== "hidden";

  return (
    <div className="space-y-4">
      <section
        aria-labelledby={headingId}
        className={cn("space-y-2 rounded-lg border p-4 sm:p-5", toneClasses[text.tone])}
      >
        <p className="text-sm font-medium">
          {SUBMITTED_LABELS[item.kind]} le {formatDate(item.submitted_on)}
        </p>
        <h2 id={headingId} className="flex items-center gap-2 text-xl font-semibold">
          <text.icon className="size-5 shrink-0" aria-hidden="true" />
          {text.title}
        </h2>
        <p className="text-pretty">{text.description}</p>
        {showIdeaLink && item.idea ? (
          <p>
            <Link to={paths.idea(item.idea.slug)} className={linkClass}>
              {ideaLinkLabel} : « {item.idea.title} »
            </Link>
          </p>
        ) : null}
      </section>

      {ideaWithdrawn(item) ? <IdeaWithdrawnNotice kind={item.kind} /> : null}

      {(phase === "rejected" || phase === "hidden") && item.decision ? (
        <StatementOfReasons
          kind={item.kind}
          decision={item.decision}
          purged={item.status === "purged"}
          reviewAvailable={canRequestReview(item)}
        />
      ) : null}

      {item.review_request ? (
        <p className="rounded-lg border bg-card p-4 text-sm" role="status">
          {item.review_request.status === "pending"
            ? `Réexamen humain demandé le ${formatDate(item.review_request.requested_on)} : il est en attente de traitement par l’éditeur.`
            : `Le réexamen humain demandé le ${formatDate(item.review_request.requested_on)} a été traité ; le statut ci-dessus en tient compte.`}
        </p>
      ) : null}
    </div>
  );
}
