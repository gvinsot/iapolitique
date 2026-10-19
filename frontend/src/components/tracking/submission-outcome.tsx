import { CheckCircle2Icon, HourglassIcon, type LucideIcon, XCircleIcon } from "lucide-react";
import { useEffect, useId, useRef } from "react";
import type { SubmissionResult } from "@/lib/submission-model";
import { type TrackedKind, trackingPhase } from "@/lib/tracking-model";
import { cn } from "@/lib/utils";

import { StatementOfReasons } from "./statement-of-reasons";
import { TrackingLinkReveal } from "./tracking-link-reveal";

interface Outcome {
  title: string;
  description: string;
  icon: LucideIcon;
  tone: "success" | "progress" | "warning";
}

const ACCEPTED: Record<TrackedKind, string> = {
  idea: "Votre idée est conservée, elle va être améliorée et chiffrée par IA. Elle rejoindra le classement dès la fin de son analyse.",
  contribution:
    "Votre contribution est acceptée : elle sera prise en compte lors de la prochaine analyse de l’idée.",
  report:
    "Accusé de réception : votre signalement est enregistré et sera examiné par IA. La suite donnée apparaîtra sur votre lien de suivi.",
};

/** An analysis takes 15 minutes or more, longer behind a queue (AGENTS.md §6.1). */
const PROCESSING_DELAY: Partial<Record<TrackedKind, string>> = {
  idea: "Le traitement de votre idée peut facilement prendre plus de 30 minutes, davantage s’il y a une file d’attente : revenez plus tard, grâce à votre lien de suivi ci-dessous.",
  contribution:
    "La nouvelle analyse de l’idée peut facilement prendre plus de 30 minutes, davantage s’il y a une file d’attente : revenez plus tard, grâce à votre lien de suivi ci-dessous.",
};

const REJECTED_TITLES: Record<TrackedKind, string> = {
  idea: "Votre idée n’a pas été retenue",
  contribution: "Votre contribution n’a pas été retenue",
  report: "Votre signalement n’a pas été retenu",
};

function outcomeOf(kind: TrackedKind, result: SubmissionResult): Outcome {
  const phase = trackingPhase({ kind, status: result.status });
  if (phase === "rejected") {
    return {
      title: REJECTED_TITLES[kind],
      // The statement of reasons below gives the reason; without one, say it here.
      description: result.statement
        ? "Le filtrage automatique n’a pas retenu ce texte. Le motif et la règle en cause sont détaillés ci-dessous."
        : (result.reason_message ??
          "Le filtrage automatique n’a pas retenu ce texte. Le motif est indiqué sur votre lien de suivi."),
      icon: XCircleIcon,
      tone: "warning",
    };
  }
  if (phase === "pending" || phase === "second_opinion") {
    return {
      title: "En cours de vérification",
      description:
        phase === "second_opinion"
          ? "Le premier filtrage n’a pas permis de trancher : un second avis est en cours. Consultez votre lien de suivi dans quelques minutes."
          : "Le filtrage est momentanément indisponible : votre texte est bien enregistré et sera examiné automatiquement. Consultez votre lien de suivi un peu plus tard.",
      icon: HourglassIcon,
      tone: "progress",
    };
  }
  return {
    title: kind === "report" ? "Signalement reçu" : "Merci, c’est enregistré",
    description: ACCEPTED[kind],
    icon: CheckCircle2Icon,
    tone: "success",
  };
}

const toneClasses: Record<Outcome["tone"], string> = {
  success: "border-primary/30 bg-accent text-accent-foreground",
  progress: "border-warning/40 bg-warning-surface text-warning",
  warning: "border-warning/40 bg-warning-surface text-warning",
};

interface SubmissionOutcomeProps {
  kind: TrackedKind;
  result: SubmissionResult;
  /** Reminder stored with the link if the visitor asks to remember it. */
  linkLabel: string;
}

/**
 * Immediate answer after a submission (AGENTS.md §5.2): the screening outcome, the refusal
 * reason with an invitation to rephrase, and the tracking link shown this once. Focus moves to
 * the heading so screen reader users hear the outcome.
 */
export function SubmissionOutcome({ kind, result, linkLabel }: SubmissionOutcomeProps) {
  const headingId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const outcome = outcomeOf(kind, result);
  const phase = trackingPhase({ kind, status: result.status });
  const rejected = phase === "rejected";
  const delay = outcome.tone === "success" ? PROCESSING_DELAY[kind] : undefined;

  useEffect(() => {
    headingRef.current?.focus();
  }, []);

  return (
    <div className="space-y-6">
      <section
        aria-labelledby={headingId}
        className={cn("space-y-3 rounded-lg border p-4 sm:p-5", toneClasses[outcome.tone])}
      >
        <h2
          id={headingId}
          ref={headingRef}
          tabIndex={-1}
          className="flex items-center gap-2 text-xl font-semibold outline-none"
        >
          <outcome.icon className="size-5 shrink-0" aria-hidden="true" />
          {outcome.title}
        </h2>
        <p className="text-pretty">{outcome.description}</p>
        {delay ? (
          <p className="flex items-start gap-2 text-sm font-medium text-pretty">
            <HourglassIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            {delay}
          </p>
        ) : null}
        {rejected ? (
          <p className="text-sm">
            {kind === "idea"
              ? "Vous pouvez reformuler votre idée et l’envoyer à nouveau, ou demander un réexamen humain depuis votre lien de suivi."
              : "Vous pouvez demander un réexamen humain depuis votre lien de suivi."}
          </p>
        ) : null}
      </section>
      {rejected && result.statement ? (
        <StatementOfReasons kind={kind} decision={result.statement} />
      ) : null}
      <TrackingLinkReveal token={result.tracking_token} label={linkLabel} />
    </div>
  );
}
