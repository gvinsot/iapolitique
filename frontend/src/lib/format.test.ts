import { describe, expect, it } from "vitest";

import { formatEuroRange, formatEuros, formatEurosCompact, formatMonths } from "./format";

// Intl uses (narrow) no-break spaces in French: normalise them for readable assertions.
const plain = (value: string) => value.replace(/\s/g, " ");

describe("format", () => {
  it("formats compact euro amounts", () => {
    expect(plain(formatEurosCompact(1_250_000_000))).toBe("1,25 Md €");
    expect(plain(formatEurosCompact(350_000_000))).toBe("350 M €");
    expect(plain(formatEurosCompact(12_000))).toBe("12 k €");
  });

  it("formats full euro amounts without decimals", () => {
    expect(plain(formatEuros(12_345_678))).toBe("12 345 678 €");
  });

  it("formats a range around its median", () => {
    expect(plain(formatEuroRange({ low: 1e8, median: 3e8, high: 6e8 }))).toBe(
      "300 M € (entre 100 M € et 600 M €)",
    );
    expect(plain(formatEuroRange({ low: 5e6, median: 5e6, high: 5e6 }))).toBe("5 M €");
  });

  it("formats durations in months and years", () => {
    expect(formatMonths(0)).toBe("0 mois");
    expect(formatMonths(6)).toBe("6 mois");
    expect(formatMonths(12)).toBe("1 an");
    expect(formatMonths(30)).toBe("2 ans et 6 mois");
  });
});
