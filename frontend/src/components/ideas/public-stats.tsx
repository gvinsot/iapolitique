import { useId } from "react";

import { Skeleton } from "@/components/ui/skeleton";
import { usePublicStats } from "@/hooks/use-public-stats";
import { formatDate, formatInteger } from "@/lib/format";
import { cn } from "@/lib/utils";

interface Counter {
  value: number;
  /** Singular and plural labels: "1 idée publiée", "42 idées publiées". */
  one: string;
  many: string;
}

function counterLabel({ value, one, many }: Counter): string {
  return value <= 1 ? one : many;
}

interface PublicStatsProps {
  className?: string;
  /** Heading level of the block title, to keep the outline valid where it is placed. */
  headingLevel?: 2 | 3;
}

/**
 * Aggregate counters of the platform (GET /api/stats): no text, identifier nor fine timestamp.
 * Hidden while unavailable: these figures are informative, never required to use the site.
 */
export function PublicStats({ className, headingLevel = 2 }: PublicStatsProps) {
  const { data } = usePublicStats();
  const headingId = useId();
  const Heading = headingLevel === 2 ? "h2" : "h3";

  if (!data) {
    return (
      <div className={cn("grid grid-cols-2 gap-3 sm:grid-cols-4", className)} aria-hidden="true">
        {[0, 1, 2, 3].map((index) => (
          <Skeleton key={index} className="h-20 rounded-lg" />
        ))}
      </div>
    );
  }

  const counters: Counter[] = [
    { value: data.ideas_published, one: "idée publiée", many: "idées publiées" },
    { value: data.ideas_received, one: "idée reçue", many: "idées reçues" },
    {
      value: data.contributions_integrated,
      one: "contribution intégrée",
      many: "contributions intégrées",
    },
    { value: data.duplicates_merged, one: "doublon regroupé", many: "doublons regroupés" },
  ];

  return (
    <section aria-labelledby={headingId} className={cn("space-y-3", className)}>
      <Heading id={headingId} className="sr-only">
        La plateforme en chiffres
      </Heading>
      <dl className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {counters.map((counter) => (
          <div key={counter.many} className="flex flex-col rounded-lg border bg-card p-4">
            <dt className="text-sm text-muted-foreground">{counterLabel(counter)}</dt>
            <dd className="order-first text-2xl font-semibold tabular-nums">
              {formatInteger(counter.value)}
            </dd>
          </div>
        ))}
      </dl>
      <p className="text-xs text-muted-foreground">
        {data.ideas_in_progress > 0
          ? `${formatInteger(data.ideas_in_progress)} ${data.ideas_in_progress === 1 ? "idée est" : "idées sont"} en cours d’examen. `
          : null}
        {data.last_published_on
          ? `Dernière publication le ${formatDate(data.last_published_on)}. `
          : null}
        Chiffres du {formatDate(data.computed_on)}.
      </p>
    </section>
  );
}
