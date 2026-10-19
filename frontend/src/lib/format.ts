/** French number formatting for amounts (euros are integers, see AGENTS.md §4.3). */

const compactEuros = new Intl.NumberFormat("fr-FR", {
  style: "currency",
  currency: "EUR",
  notation: "compact",
  compactDisplay: "short",
  maximumSignificantDigits: 3,
});

const fullEuros = new Intl.NumberFormat("fr-FR", {
  style: "currency",
  currency: "EUR",
  maximumFractionDigits: 0,
});

const compactNumber = new Intl.NumberFormat("fr-FR", {
  notation: "compact",
  compactDisplay: "short",
  maximumSignificantDigits: 3,
});

const decimal = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 2 });

const preciseEuros = new Intl.NumberFormat("fr-FR", {
  style: "currency",
  currency: "EUR",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

const percent = new Intl.NumberFormat("fr-FR", { style: "percent", maximumFractionDigits: 0 });

const integer = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });

/** "1,2 Md €", "350 M €", "12 k €" — for headlines and cards. */
export function formatEurosCompact(amount: number): string {
  return compactEuros.format(amount);
}

/** "12 345 678 €" — for tables and details. */
export function formatEuros(amount: number): string {
  return fullEuros.format(amount);
}

/** "1,23 €" — estimated AI costs, which have cents. */
export function formatEurosPrecise(amount: number): string {
  return preciseEuros.format(amount);
}

/** "42 %" from a ratio between 0 and 1. */
export function formatPercent(ratio: number): string {
  return percent.format(ratio);
}

export function formatInteger(value: number): string {
  return integer.format(value);
}

/** Composite ranking score (AGENTS.md §6.3), a weighted amount without unit: "12,3 M". */
export function formatScore(score: number): string {
  return compactNumber.format(score);
}

/** Score factor between 0 and 1: "0,75". */
export function formatFactor(value: number): string {
  return decimal.format(value);
}

export interface EuroRange {
  low: number;
  median: number;
  high: number;
}

/** "350 M € (entre 120 M € et 600 M €)" */
export function formatEuroRange({ low, median, high }: EuroRange): string {
  if (low === high) return formatEurosCompact(median);
  return `${formatEurosCompact(median)} (entre ${formatEurosCompact(low)} et ${formatEurosCompact(high)})`;
}

/** "6 mois", "1 an", "2 ans et 6 mois" */
export function formatMonths(months: number): string {
  const total = Math.max(0, Math.round(months));
  const years = Math.floor(total / 12);
  const rest = total % 12;
  const yearPart = years === 0 ? "" : years === 1 ? "1 an" : `${years} ans`;
  const monthPart = rest === 0 ? "" : `${rest} mois`;
  if (yearPart && monthPart) return `${yearPart} et ${monthPart}`;
  return yearPart || monthPart || "0 mois";
}

const longDate = new Intl.DateTimeFormat("fr-FR", {
  day: "numeric",
  month: "long",
  year: "numeric",
  timeZone: "Europe/Paris",
});

/** "18 octobre 2026". Accepts `YYYY-MM-DD` (read as a calendar day) or a full date-time. */
export function formatDate(value: string): string {
  const dayOnly = /^\d{4}-\d{2}-\d{2}$/.test(value);
  // A bare day is parsed as UTC midnight: read it at noon UTC so it never shifts by one day.
  const date = new Date(dayOnly ? `${value}T12:00:00Z` : value);
  if (Number.isNaN(date.getTime())) return value;
  return longDate.format(date);
}

const shortDateTime = new Intl.DateTimeFormat("fr-FR", {
  dateStyle: "short",
  timeStyle: "short",
  timeZone: "Europe/Paris",
});

/** "19/10/2026 14:05" (Paris time), for administration timestamps; "—" when absent. */
export function formatDateTime(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : shortDateTime.format(date);
}
