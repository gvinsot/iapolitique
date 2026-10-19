import { CoinsIcon, FilterIcon, ListTodoIcon, type LucideIcon, WalletIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { AdminDashboard, AdminQueues } from "@/api/types";
import { paths } from "@/app/paths";
import { labelFor, targetSectionLabels } from "@/lib/admin-model";
import { formatDateTime, formatEurosPrecise, formatInteger, formatPercent } from "@/lib/format";
import { cn } from "@/lib/utils";

import { adminLinkClass } from "./admin-styles";

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
