import {
  BrainIcon,
  CoinsIcon,
  FilterIcon,
  KeyRoundIcon,
  ListTodoIcon,
  Loader2Icon,
  type LucideIcon,
  RotateCcwIcon,
  WalletIcon,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { Link } from "react-router";

import type { AdminAlert, AdminDashboard, AdminQueues } from "@/api/types";
import { paths } from "@/app/paths";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { labelFor, targetSectionLabels } from "@/lib/admin-model";
import { formatDateTime, formatEurosPrecise, formatInteger, formatPercent } from "@/lib/format";
import { cn } from "@/lib/utils";

import { adminLinkClass } from "./admin-styles";
import { ConfirmDialog } from "./admin-ui";

interface TileProps {
  label: string;
  value: string;
  icon: LucideIcon;
  detail?: ReactNode;
  /** Draws attention to something needing action. */
  alert?: boolean;
}

function Tile({ label, value, icon: Icon, detail, alert = false }: TileProps) {
  return (
    <li
      className={cn(
        "space-y-2 rounded-lg border bg-card p-4",
        alert && "border-warning/60 bg-warning-surface",
      )}
    >
      <p
        className={cn(
          "flex items-center gap-2 text-sm font-medium",
          alert ? "text-warning" : "text-muted-foreground",
        )}
      >
        <Icon className="size-4" aria-hidden="true" />
        {label}
      </p>
      <p className={cn("text-2xl font-semibold tabular-nums", alert && "text-warning")}>{value}</p>
      {detail ? (
        <div className={cn("text-xs", alert ? "text-warning" : "text-muted-foreground")}>
          {detail}
        </div>
      ) : null}
    </li>
  );
}

function rate(value: number | null): string {
  return value === null ? "—" : formatPercent(value);
}

/** Headline figures of the administration (AGENTS.md §8): jobs, screening, AI costs. */
export function DashboardTiles({ dashboard }: { dashboard: AdminDashboard }) {
  const { jobs, ai_costs: costs } = dashboard;
  const ideas = dashboard.screening.find((rates) => rates.kind === "idea");
  const budgetUsed = costs.daily_budget_eur > 0 ? costs.today_eur / costs.daily_budget_eur : null;

  return (
    <ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <Tile
        label="Jobs en file"
        icon={ListTodoIcon}
        value={formatInteger(jobs.queued)}
        detail={
          <>
            {formatInteger(jobs.running)} en cours · {formatInteger(jobs.dead)} abandonné
            {jobs.dead > 1 ? "s" : ""}
          </>
        }
        alert={jobs.dead > 0}
      />
      <Tile
        label={`Idées acceptées (${dashboard.screening_window_days} j)`}
        icon={FilterIcon}
        value={rate(ideas?.acceptance_rate ?? null)}
        detail={
          ideas ? (
            <>
              {formatInteger(ideas.total)} filtrées · {rate(ideas.rejection_rate)} refusées ·{" "}
              {rate(ideas.gray_zone_rate)} en zone grise
            </>
          ) : (
            "Aucune idée filtrée sur la période"
          )
        }
      />
      <Tile
        label="Coût IA du jour"
        icon={WalletIcon}
        value={formatEurosPrecise(costs.today_eur)}
        alert={costs.budget_reached}
        detail={
          budgetUsed === null ? (
            "Aucun plafond journalier"
          ) : (
            <div className="space-y-1">
              <p>
                {formatPercent(Math.min(budgetUsed, 9.99))} du plafond de{" "}
                {formatEurosPrecise(costs.daily_budget_eur)}
                {costs.budget_reached
                  ? ` : les jobs IA attendent le ${formatDateTime(costs.next_reset)}`
                  : ` · reste ${formatEurosPrecise(costs.remaining_today_eur)}`}
              </p>
              <div className="h-1.5 rounded-full bg-muted" aria-hidden="true">
                <div
                  className="h-full rounded-full bg-primary"
                  style={{ width: `${Math.min(100, budgetUsed * 100)}%` }}
                />
              </div>
            </div>
          )
        }
      />
      <Tile label="Coût IA du mois" icon={CoinsIcon} value={formatEurosPrecise(costs.month_eur)} />
    </ul>
  );
}

interface QueueItem {
  key: keyof AdminQueues;
  label: string;
  to: string;
}

const QUEUE_ITEMS: readonly QueueItem[] = [
  {
    key: "escalations_pending",
    label: "Escalades à traiter (cible 24 h)",
    to: paths.admin.escalations,
  },
  {
    key: "review_requests_pending",
    label: "Réexamens humains demandés",
    to: paths.admin.reviewRequests,
  },
  { key: "jobs_dead", label: "Jobs abandonnés", to: `${paths.admin.jobs}?status=dead` },
  {
    key: "ideas_analysis_failed",
    label: "Analyses en échec",
    to: `${paths.admin.ideas}?status=analysis_failed`,
  },
  {
    key: "texts_second_opinion",
    label: "Textes en attente de second avis",
    to: `${paths.admin.ideas}?status=second_opinion`,
  },
  {
    key: "texts_submitted_stale",
    label: "Textes non filtrés depuis plus d’une heure",
    to: `${paths.admin.ideas}?status=submitted`,
  },
  {
    key: "reports_awaiting_review",
    label: "Signalements en attente de revue",
    to: `${paths.admin.reports}?status=accepted`,
  },
  { key: "ideas_hidden", label: "Idées masquées", to: `${paths.admin.ideas}?status=hidden` },
];

/** What waits for the administrator, with links to each queue. */
export function DashboardQueues({ queues }: { queues: AdminQueues }) {
  return (
    <ul className="divide-y rounded-lg border bg-card">
      {QUEUE_ITEMS.map((item) => {
        const count = queues[item.key];
        return (
          <li key={item.key} className="flex items-center justify-between gap-3 px-4 py-3 text-sm">
            <Link to={item.to} className={adminLinkClass}>
              {item.label}
            </Link>
            <span
              className={cn(
                "rounded-full px-2.5 py-0.5 font-semibold tabular-nums",
                count > 0 ? "bg-warning-surface text-warning" : "text-muted-foreground",
              )}
            >
              {formatInteger(count)}
            </span>
          </li>
        );
      })}
    </ul>
  );
}

/** Screening outcomes per kind of text over the window. */
export function ScreeningRatesTable({ dashboard }: { dashboard: AdminDashboard }) {
  const headers = [
    "Textes",
    "Filtrés",
    "Acceptés",
    "Refusés",
    "Zone grise",
    "Second avis (accepte / refuse)",
    "Erreurs",
    "Étiquetés",
  ];
  return (
    <div className="overflow-x-auto rounded-lg border bg-card">
      <table className="w-full min-w-[44rem] border-collapse text-sm">
        <caption className="border-b bg-muted/40 px-4 py-3 text-left font-semibold">
          Filtrage sur {dashboard.screening_window_days} jours
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
          {dashboard.screening.length === 0 ? (
            <tr>
              <td colSpan={headers.length} className="px-4 py-6 text-center text-muted-foreground">
                Aucun texte filtré sur la période.
              </td>
            </tr>
          ) : null}
          {dashboard.screening.map((rates) => (
            <tr key={rates.kind} className="border-b tabular-nums last:border-b-0">
              <th scope="row" className="px-4 py-2 text-left font-medium">
                {labelFor(targetSectionLabels, rates.kind)}
              </th>
              <td className="px-4 py-2">{formatInteger(rates.total)}</td>
              <td className="px-4 py-2">
                {formatInteger(rates.accepted)} ({rate(rates.acceptance_rate)})
              </td>
              <td className="px-4 py-2">
                {formatInteger(rates.rejected)} ({rate(rates.rejection_rate)})
              </td>
              <td className="px-4 py-2">
                {formatInteger(rates.gray_zone)} ({rate(rates.gray_zone_rate)})
              </td>
              <td className="px-4 py-2">
                {formatInteger(rates.second_opinion_accept)} /{" "}
                {formatInteger(rates.second_opinion_reject)}
              </td>
              <td className="px-4 py-2">{formatInteger(rates.errors)}</td>
              <td className="px-4 py-2">{formatInteger(rates.labeled)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const ALERT_TITLES: Record<string, string> = {
  mistral_auth: "Clé Mistral refusée",
  mistral_reasoning_unsupported: "Effort de raisonnement refusé par un modèle Mistral",
};

function plural(count: number, word: string): string {
  return `${formatInteger(count)} ${word}${count > 1 ? "s" : ""}`;
}

/**
 * « Vérifier la clé Mistral maintenant » (POST /api/admin/provider-alerts/mistral/check): one
 * call that bills nothing, instead of waiting up to an hour for the next job allowed to check
 * the key. `onCheck` shows the result (toast); the button is disabled while it runs.
 */
export function CheckMistralKeyButton({ onCheck }: { onCheck: () => Promise<void> }) {
  const [checking, setChecking] = useState(false);
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={checking}
      onClick={() => {
        setChecking(true);
        void onCheck().finally(() => {
          setChecking(false);
        });
      }}
    >
      {checking ? (
        <Loader2Icon aria-hidden="true" className="animate-spin" />
      ) : (
        <KeyRoundIcon aria-hidden="true" />
      )}
      Vérifier la clé Mistral maintenant
    </Button>
  );
}

interface DashboardAlertsProps {
  alerts: readonly AdminAlert[];
  /** Checks the Mistral key now (button of the refused-key alert); no button without it. */
  onCheckKey?: () => Promise<void>;
}

/**
 * Configuration alerts in progress (AGENTS.md §6.1, §6.4): a refused Mistral key makes every AI
 * job wait without failing until the key is fixed; a model that refuses `reasoning_effort` is
 * called without it. The message comes from the API, in French.
 */
export function DashboardAlerts({ alerts, onCheckKey }: DashboardAlertsProps) {
  if (alerts.length === 0) return null;
  return (
    <section aria-label="Alertes de configuration" className="space-y-3">
      {alerts.map((alert) => {
        const auth = alert.code === "mistral_auth";
        return (
          <Alert key={`${alert.code}-${alert.since}`} variant={auth ? "destructive" : "warning"}>
            {auth ? <KeyRoundIcon aria-hidden="true" /> : <BrainIcon aria-hidden="true" />}
            <AlertTitle>{labelFor(ALERT_TITLES, alert.code)}</AlertTitle>
            <AlertDescription>
              <p>{alert.message}</p>
              {auth ? (
                <>
                  <p>
                    {plural(alert.failures, "appel")} refusé{alert.failures > 1 ? "s" : ""}, le
                    dernier le {formatDateTime(alert.last_seen_at)}. Les jobs IA attendent sans
                    échouer et reprennent seuls au premier appel accepté.
                  </p>
                  <p>
                    Corrigez <code>MISTRAL_API_KEY</code> dans l’environnement PulsarCD puis
                    redéployez (procédure : <code>docs/ops.md</code>, rotation de la clé Mistral) ;
                    relancez ensuite les analyses passées en échec.
                  </p>
                  {onCheckKey ? (
                    <div className="pt-1">
                      <CheckMistralKeyButton onCheck={onCheckKey} />
                    </div>
                  ) : null}
                </>
              ) : (
                <p>
                  {plural(alert.failures, "refus")} constaté{alert.failures > 1 ? "s" : ""}, le
                  dernier le {formatDateTime(alert.last_seen_at)}. Rien n’attend : les appels
                  partent sans le paramètre. L’alerte disparaît quand le modèle change ou l’accepte
                  de nouveau.
                </p>
              )}
            </AlertDescription>
          </Alert>
        );
      })}
    </section>
  );
}

interface ReanalyzeFailedActionProps {
  /** Ideas currently in `analysis_failed` (dashboard queue). */
  failedCount: number;
  /** A refused Mistral key is still reported: the analyses would only wait. */
  keyRefused: boolean;
  onConfirm: () => Promise<void>;
}

/**
 * « Relancer toutes les analyses échouées » (POST /api/admin/ideas/reanalyze-failed), confirmed
 * in a dialog, never with window.confirm.
 */
export function ReanalyzeFailedAction({
  failedCount,
  keyRefused,
  onConfirm,
}: ReanalyzeFailedActionProps) {
  return (
    <ConfirmDialog
      trigger={
        <>
          <RotateCcwIcon aria-hidden="true" />
          Relancer toutes les analyses échouées
        </>
      }
      triggerLabel={`Relancer toutes les analyses échouées (${formatInteger(failedCount)})`}
      title="Relancer toutes les analyses échouées"
      description={
        <>
          {failedCount === 0
            ? "Aucune idée n’est en échec d’analyse pour l’instant."
            : `${formatInteger(failedCount)} idée${failedCount > 1 ? "s" : ""} en échec d’analyse ${failedCount > 1 ? "repartent" : "repart"} en file, plus anciennes d’abord.`}{" "}
          Chaque analyse relancée consomme du budget Mistral. L’action est consignée au journal.
          {keyRefused
            ? " Attention : la clé Mistral est encore refusée, les analyses attendront qu’elle soit corrigée."
            : null}
        </>
      }
      confirm="Relancer les analyses"
      onConfirm={() => onConfirm()}
    />
  );
}
