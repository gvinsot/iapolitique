import { describe, expect, it } from "vitest";

import { attributionText, formatLegalDate, parseLegalInfo } from "./legal";
import { ruleForReasonCode, screeningRules } from "./rules";

const validInfo = {
  publisher: {
    name: "Camille Exemple",
    address: "1 rue de l’Exemple, 75000 Paris",
    phone: "01 02",
  },
  contact_email: "contact@example.org",
  privacy_email: "confidentialite@example.org",
  cgu_version: "1.0",
  pages: { mentions_legales: "2026-10-18", regles: "2026-10-19T08:00:00Z" },
};

describe("parseLegalInfo", () => {
  it("accepts the documented shape and keeps only known, valid page dates", () => {
    const info = parseLegalInfo({
      ...validInfo,
      pages: { ...validInfo.pages, licence: "pas une date", inconnue: "2026-01-01" },
    });
    expect(info).toEqual({ ...validInfo, pages: validInfo.pages });
  });

  it("trims values and tolerates a missing pages map", () => {
    const { pages: _pages, ...withoutPages } = validInfo;
    const info = parseLegalInfo({ ...withoutPages, cgu_version: " 2.1 " });
    expect(info?.cgu_version).toBe("2.1");
    expect(info?.pages).toEqual({});
  });

  it.each([
    null,
    "texte",
    [],
    { ...validInfo, publisher: null },
    { ...validInfo, publisher: { ...validInfo.publisher, phone: "  " } },
    { ...validInfo, contact_email: 42 },
    { ...validInfo, cgu_version: undefined },
  ])("rejects an incomplete answer (%#)", (value) => {
    expect(parseLegalInfo(value)).toBeNull();
  });
});

describe("formatLegalDate", () => {
  it("formats a calendar day without shifting it", () => {
    expect(formatLegalDate("2026-10-18")).toBe("18 octobre 2026");
    expect(formatLegalDate("2026-01-01")).toBe("1 janvier 2026");
  });

  it("formats a date-time in the Paris time zone and keeps unparsable values", () => {
    expect(formatLegalDate("2026-10-18T23:30:00Z")).toBe("19 octobre 2026");
    expect(formatLegalDate("bientôt")).toBe("bientôt");
  });
});

describe("attributionText", () => {
  it("uses the placeholder until the export date is known", () => {
    expect(attributionText()).toBe(
      "Source : iapolitique.fr – Propositions citoyennes et analyses générées par IA (Mistral AI), mise à jour du AAAA-MM-JJ – Licence Ouverte 2.0",
    );
    expect(attributionText("2026-11-02T03:00:00Z")).toContain("mise à jour du 2026-11-02 –");
    expect(attributionText("02/11/2026")).toContain("AAAA-MM-JJ");
  });
});

describe("screening rules", () => {
  it("numbers the criteria in the order applied by the decision model", () => {
    expect(screeningRules.map((rule) => rule.number)).toEqual([
      "4.1",
      "4.2",
      "4.3",
      "4.4",
      "4.5",
      "4.6",
      "4.7",
      "4.8",
    ]);
    expect(
      screeningRules.filter((rule) => rule.appliesTo.includes("idea")).map((r) => r.key),
    ).toEqual([
      "injection",
      "respectful",
      "targeted",
      "personal_data",
      "on_topic",
      "lawful",
      "realism",
    ]);
    expect(new Set(screeningRules.map((rule) => rule.anchor)).size).toBe(screeningRules.length);
  });

  it("finds the rule of a refusal reason code", () => {
    expect(ruleForReasonCode("off_topic")?.number).toBe("4.5");
    expect(ruleForReasonCode("inconnu")).toBeNull();
    expect(ruleForReasonCode(null)).toBeNull();
  });
});
