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

const integer = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });

/** "1,2 Md €", "350 M €", "12 k €" — for headlines and cards. */
export function formatEurosCompact(amount: number): string {
  return compactEuros.format(amount);
}

/** "12 345 678 €" — for tables and details. */
export function formatEuros(amount: number): string {
  return fullEuros.format(amount);
}

export function formatInteger(value: number): string {
  return integer.format(value);
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
