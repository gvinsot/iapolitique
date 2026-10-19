import { useMutation } from "@tanstack/react-query";
import { FlagIcon, MessageSquarePlusIcon, TriangleAlertIcon, XIcon } from "lucide-react";
import { type ReactNode, useId, useState } from "react";

import { errorMessage } from "@/api/errors";
import { submitContribution, submitReport } from "@/api/submissions";
import type { SubmissionResponse } from "@/api/types";
import { SubmissionOutcome } from "@/components/tracking/submission-outcome";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { formatDate } from "@/lib/format";
import type { TrackedKind } from "@/lib/tracking-model";

import { ContributionForm, type ContributionFormValues } from "./contribution-form";
import { ReportForm, type ReportFormValues } from "./report-form";

type Panel = "contribution" | "report" | null;

interface IdeaParticipationProps {
  slug: string;
}

/**
 * The two ways a visitor acts on a published idea (AGENTS.md §5.5, §5.6): an improvement or an
 * objection, and a report. Forms open on demand (the anti-robot check loads with them); after a
 * submission, the immediate outcome and the tracking link replace the form.
 */
export function IdeaParticipation({ slug }: IdeaParticipationProps) {
  const [open, setOpen] = useState<Panel>(null);
  const contributionId = useId();
  const reportId = useId();

  const contribution = useMutation({
    mutationFn: (values: ContributionFormValues) => submitContribution(slug, values),
    meta: { errorToast: false },
  });
  const report = useMutation({
    mutationFn: (values: ReportFormValues) => submitReport(slug, values),
    meta: { errorToast: false },
  });

  const toggle = (panel: Exclude<Panel, null>) => {
    setOpen((current) => (current === panel ? null : panel));
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap gap-3">
        <Button
          type="button"
          variant={open === "contribution" ? "secondary" : "default"}
          aria-expanded={open === "contribution"}
          aria-controls={contributionId}
          onClick={() => {
            toggle("contribution");
          }}
        >
          <MessageSquarePlusIcon aria-hidden="true" />
          Proposer une amélioration ou une objection
        </Button>
        <Button
          type="button"
          variant={open === "report" ? "secondary" : "outline"}
          aria-expanded={open === "report"}
          aria-controls={reportId}
          onClick={() => {
            toggle("report");
          }}
        >
          <FlagIcon aria-hidden="true" />
          Signaler un problème
        </Button>
      </div>

      <div id={contributionId} hidden={open !== "contribution"}>
        {open === "contribution" ? (
          <Panel
            title="Améliorer ou contester cette idée"
            onClose={() => {
              setOpen(null);
            }}
          >
            {contribution.data ? (
              <Outcome
                kind="contribution"
                result={contribution.data}
                onAgain={() => {
                  contribution.reset();
                }}
              />
            ) : (
              <>
                <SubmissionError
                  error={contribution.isError ? contribution.error : null}
                  title="Votre contribution n’a pas pu être envoyée"
                />
                <p className="text-sm text-muted-foreground">
                  Votre texte n’est jamais publié tel quel : s’il est accepté, la prochaine analyse
                  de l’idée l’examine et en publie un résumé neutre avec sa réponse.
                </p>
                <ContributionForm
                  onSubmit={(values) => contribution.mutateAsync(values).then(() => undefined)}
                  submitting={contribution.isPending}
                />
              </>
            )}
          </Panel>
        ) : null}
      </div>

      <div id={reportId} hidden={open !== "report"}>
        {open === "report" ? (
          <Panel
            title="Signaler un problème sur cette idée"
            onClose={() => {
              setOpen(null);
            }}
          >
            {report.data ? (
              <Outcome
                kind="report"
                result={report.data}
                onAgain={() => {
                  report.reset();
                }}
              />
            ) : (
              <>
                <SubmissionError
                  error={report.isError ? report.error : null}
                  title="Votre signalement n’a pas pu être envoyé"
                />
                <ReportForm
                  onSubmit={(values) => report.mutateAsync(values).then(() => undefined)}
                  submitting={report.isPending}
                />
              </>
            )}
          </Panel>
        ) : null}
      </div>
    </div>
  );
}

function Panel({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-4 rounded-lg border bg-card p-4 sm:p-5">
      <div className="flex items-start justify-between gap-3">
        <h3 id={headingId} className="text-lg font-semibold">
          {title}
        </h3>
        <Button type="button" variant="ghost" size="sm" onClick={onClose}>
          <XIcon aria-hidden="true" />
          Fermer
        </Button>
      </div>
      {children}
    </section>
  );
}

function SubmissionError({ error, title }: { error: unknown; title: string }) {
  if (!error) return null;
  return (
    <Alert variant="destructive" role="alert">
      <TriangleAlertIcon aria-hidden="true" />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>
        <p>{errorMessage(error)}</p>
      </AlertDescription>
    </Alert>
  );
}

const OUTCOME_LABELS: Record<Exclude<TrackedKind, "idea">, string> = {
  contribution: "Contribution envoyée le",
  report: "Signalement envoyé le",
};

function Outcome({
  kind,
  result,
  onAgain,
}: {
  kind: Exclude<TrackedKind, "idea">;
  result: SubmissionResponse;
  onAgain: () => void;
}) {
  return (
    <div className="space-y-4">
      <SubmissionOutcome
        kind={kind}
        result={result}
        linkLabel={`${OUTCOME_LABELS[kind]} ${formatDate(new Date().toISOString())}`}
      />
      <Button type="button" variant="outline" onClick={onAgain}>
        J’ai conservé mon lien : envoyer un autre texte
      </Button>
    </div>
  );
}
