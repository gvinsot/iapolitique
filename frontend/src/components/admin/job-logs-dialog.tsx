import { RefreshCwIcon, ScrollTextIcon } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { apiClient } from "@/api/client";
import { errorMessage } from "@/api/errors";
import type { AdminJob, AdminJobLog } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { jobKindLabels, jobStatusLabels, labelFor } from "@/lib/admin-model";
import { formatDateTime } from "@/lib/format";

/** Refresh period while the job is queued or running. */
export const JOB_LOGS_REFRESH_MS = 3_000;

const levelVariant: Record<string, "destructive" | "secondary" | "outline"> = {
  error: "destructive",
  warn: "destructive",
  info: "secondary",
};

function isActive(job: AdminJob): boolean {
  return job.status === "queued" || job.status === "running";
}

interface JobLogsState {
  job: AdminJob;
  lines: AdminJobLog[];
  error: string | null;
  loading: boolean;
}

/**
 * The log lines of a job, fetched when `open`, then only the new ones (`after` the last id)
 * on « Actualiser » and every few seconds while the job is queued or running.
 */
function useJobLogs(initial: AdminJob, open: boolean) {
  const [state, setState] = useState<JobLogsState>({
    job: initial,
    lines: [],
    error: null,
    loading: false,
  });
  const lastId = useRef<number | undefined>(undefined);
  const busy = useRef(false);

  const load = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setState((current) => ({ ...current, loading: true }));
    try {
      let more = true;
      while (more) {
        const { data } = await apiClient.GET("/api/admin/jobs/{id}/logs", {
          params: { path: { id: initial.id }, query: { after: lastId.current } },
        });
        if (!data) break;
        const last = data.items.at(-1);
        if (last) lastId.current = last.id;
        setState((current) => ({
          job: data.job,
          lines: [...current.lines, ...data.items],
          error: null,
          loading: current.loading,
        }));
        more = data.has_more;
      }
      setState((current) => ({ ...current, loading: false }));
    } catch (cause) {
      setState((current) => ({ ...current, error: errorMessage(cause), loading: false }));
    } finally {
      busy.current = false;
    }
  }, [initial.id]);

  /** Called when the dialog opens: the lines are read again from the start. */
  const reset = useCallback((job: AdminJob) => {
    lastId.current = undefined;
    setState({ job, lines: [], error: null, loading: false });
  }, []);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const active = isActive(state.job);
  useEffect(() => {
    if (!open || !active) return;
    const timer = window.setInterval(() => void load(), JOB_LOGS_REFRESH_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [open, active, load]);

  return { ...state, active, refresh: load, reset };
}

/** Fields of a line, `name=value`, as plain text. */
function fieldsText(fields: unknown): string {
  if (fields === null || typeof fields !== "object") return "";
  return Object.entries(fields as Record<string, unknown>)
    .map(([name, value]) => `${name}=${typeof value === "string" ? value : JSON.stringify(value)}`)
    .join("  ");
}

/** Button and dialog showing the log lines of a job (AGENTS.md §8). Plain text only. */
export function JobLogsDialog({ job }: { job: AdminJob }) {
  const [open, setOpen] = useState(false);
  const logs = useJobLogs(job, open);
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) logs.reset(job);
        setOpen(next);
      }}
    >
      <DialogTrigger asChild>
        <Button
          type="button"
          size="sm"
          variant="outline"
          aria-label={`Voir les logs du job n° ${job.id}`}
        >
          <ScrollTextIcon aria-hidden="true" />
          Logs
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-[90vh] sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>
            Logs du job n° {job.id} · {labelFor(jobKindLabels, job.kind)}
          </DialogTitle>
          <DialogDescription>
            Statut : {labelFor(jobStatusLabels, logs.job.status)}, tentative {logs.job.attempts} /{" "}
            {logs.job.max_attempts}.{" "}
            {logs.active
              ? "Actualisation automatique toutes les 3 secondes tant que le job est en file ou en cours."
              : "Le job est terminé."}
          </DialogDescription>
        </DialogHeader>
        <div className="flex items-center justify-between gap-2">
          <p className="text-sm text-muted-foreground" aria-live="polite">
            {logs.lines.length} ligne{logs.lines.length > 1 ? "s" : ""}
            {logs.loading ? " · chargement…" : ""}
          </p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void logs.refresh()}
            disabled={logs.loading}
          >
            <RefreshCwIcon aria-hidden="true" />
            Actualiser
          </Button>
        </div>
        {logs.error ? (
          <p role="alert" className="text-sm text-destructive">
            {logs.error}
          </p>
        ) : null}
        <div className="max-h-[60vh] overflow-auto rounded-md border bg-muted/40">
          {logs.lines.length === 0 && !logs.loading ? (
            <p className="p-3 text-sm text-muted-foreground">
              Aucune ligne de log pour ce job. Les lignes apparaissent pendant son exécution ; un
              appel à Mistral n’écrit sa ligne de fin qu’une fois terminé (15 minutes ou plus).
            </p>
          ) : (
            <ol className="divide-y font-mono text-xs" aria-label={`Logs du job n° ${job.id}`}>
              {logs.lines.map((line) => (
                <li key={line.id} className="space-y-1 p-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <time dateTime={line.logged_at} className="text-muted-foreground">
                      {formatDateTime(line.logged_at)}
                    </time>
                    <Badge variant={levelVariant[line.level] ?? "outline"}>{line.level}</Badge>
                    <span className="font-semibold break-words">{line.message}</span>
                  </div>
                  {fieldsText(line.fields) ? (
                    <p className="break-all whitespace-pre-wrap text-muted-foreground">
                      {fieldsText(line.fields)}
                    </p>
                  ) : null}
                </li>
              ))}
            </ol>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
