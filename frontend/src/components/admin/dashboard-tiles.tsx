import {
  CoinsIcon,
  FilterIcon,
  ListTodoIcon,
  type LucideIcon,
  MessageSquareWarningIcon,
  SirenIcon,
  WalletIcon,
} from "lucide-react";
import type { ReactNode } from "react";

import { formatEurosPrecise, formatInteger, formatPercent } from "@/lib/format";
import { acceptanceRate, type DashboardStats } from "@/lib/admin-model";
import { cn } from "@/lib/utils";

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

/** Dashboard of the administration (AGENTS.md §8): job queue, screening, AI costs, queues. */
export function DashboardTiles({ stats }: { stats: DashboardStats }) {
  const rate = acceptanceRate(stats.screening);
  const budgetUsed =
    stats.ai_costs.daily_budget_eur > 0
      ? stats.ai_costs.today_eur / stats.ai_costs.daily_budget_eur
      : null;

  return (
    <ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
      <Tile
        label="Jobs en file"
        icon={ListTodoIcon}
        value={formatInteger(stats.jobs.queued)}
        detail={
          <>
            {formatInteger(stats.jobs.running)} en cours · {formatInteger(stats.jobs.failed)} en
            échec
          </>
        }
        alert={stats.jobs.failed > 0}
      />
      <Tile
        label={`Taux d’acceptation (${stats.screening.window_days} j)`}
        icon={FilterIcon}
        value={rate === null ? "—" : formatPercent(rate)}
        detail={
          <>
            {formatInteger(stats.screening.accepted)} acceptées ·{" "}
            {formatInteger(stats.screening.rejected)} refusées ·{" "}
            {formatInteger(stats.screening.gray_zone)} en zone grise
          </>
        }
      />
      <Tile
        label="Coût IA du jour"
        icon={WalletIcon}
        value={formatEurosPrecise(stats.ai_costs.today_eur)}
        alert={budgetUsed !== null && budgetUsed >= 1}
        detail={
          budgetUsed === null ? (
            "Aucun plafond journalier"
          ) : (
            <div className="space-y-1">
              <p>
                {formatPercent(Math.min(budgetUsed, 9.99))} du plafond de{" "}
                {formatEurosPrecise(stats.ai_costs.daily_budget_eur)}
                {budgetUsed >= 1 ? " : les jobs attendent minuit" : ""}
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
      <Tile
        label="Coût IA du mois"
        icon={CoinsIcon}
        value={formatEurosPrecise(stats.ai_costs.month_eur)}
      />
      <Tile
        label="Escalades en attente"
        icon={SirenIcon}
        value={formatInteger(stats.escalations_pending)}
        detail="Transmission à PHAROS visée sous 24 h"
        alert={stats.escalations_pending > 0}
      />
      <Tile
        label="Réexamens demandés"
        icon={MessageSquareWarningIcon}
        value={formatInteger(stats.review_requests_pending)}
        alert={stats.review_requests_pending > 0}
      />
    </ul>
  );
}
