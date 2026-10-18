import { type EuroRange, formatEuroRange, formatEurosCompact } from "@/lib/format";
import { cn } from "@/lib/utils";

interface GainRangeProps {
  range: EuroRange;
  /** Visible label, e.g. "Gain annuel estimé". */
  label: string;
  /** Card layout: one line of text and a thin bar. */
  compact?: boolean;
  className?: string;
}

function percent(value: number, max: number): number {
  if (max <= 0) return 0;
  return Math.min(100, Math.max(0, (value / max) * 100));
}

/**
 * Low / median / high estimate. The text gives the three values; the bar (decorative, hidden
 * from assistive technologies) places the range on a scale from zero to the high value.
 */
export function GainRange({ range, label, compact = false, className }: GainRangeProps) {
  const low = Math.max(0, range.low);
  const high = Math.max(low, range.high);
  const median = Math.min(high, Math.max(low, range.median));
  const lowPct = percent(low, high);
  const medianPct = percent(median, high);

  const bar = (
    <div
      className={cn("relative h-2 rounded-full bg-muted", compact && "h-1.5")}
      aria-hidden="true"
    >
      <div
        className="absolute inset-y-0 rounded-full bg-chart-2"
        style={{ left: `${lowPct}%`, right: "0%" }}
      />
      <div
        className="absolute -inset-y-1 w-1 -translate-x-1/2 rounded-full bg-primary"
        style={{ left: `${medianPct}%` }}
      />
    </div>
  );

  if (compact) {
    return (
      <div className={cn("space-y-1.5", className)}>
        <p className="text-sm">
          <span className="text-muted-foreground">{label} : </span>
          <strong className="font-semibold tabular-nums">{formatEurosCompact(median)}</strong>
          {low !== high ? (
            <span className="text-muted-foreground tabular-nums">
              {" "}
              (entre {formatEurosCompact(low)} et {formatEurosCompact(high)})
            </span>
          ) : null}
        </p>
        {high > 0 ? bar : null}
      </div>
    );
  }

  return (
    <div className={cn("space-y-3", className)}>
      <p className="text-sm text-muted-foreground">{label}</p>
      <p className="text-2xl font-semibold tabular-nums">{formatEurosCompact(median)}</p>
      <p className="sr-only">Fourchette : {formatEuroRange({ low, median, high })}</p>
      {high > 0 ? bar : null}
      <dl className="grid grid-cols-3 gap-2 text-sm" aria-hidden="true">
        <div>
          <dt className="text-muted-foreground">Basse</dt>
          <dd className="font-medium tabular-nums">{formatEurosCompact(low)}</dd>
        </div>
        <div className="text-center">
          <dt className="text-muted-foreground">Médiane</dt>
          <dd className="font-medium tabular-nums">{formatEurosCompact(median)}</dd>
        </div>
        <div className="text-right">
          <dt className="text-muted-foreground">Haute</dt>
          <dd className="font-medium tabular-nums">{formatEurosCompact(high)}</dd>
        </div>
      </dl>
    </div>
  );
}
