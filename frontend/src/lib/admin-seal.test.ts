import { describe, expect, it } from "vitest";

import { analysisText, readSealContent } from "./admin-seal";

describe("readSealContent", () => {
  it("reads the sealed idea, review, reports and the backfill marker", () => {
    const content = readSealContent({
      idea: {
        status: "hidden",
        slug: "taxe-exemple",
        title: "Créer une taxe",
        summary: "Résumé",
        original_text: "Texte [donnée masquée]",
        submitted_territory: null,
        submitted_admin_level: "etat",
        submitted_at: "2026-10-10T08:00:00Z",
        published_at: "2026-10-11T08:00:00Z",
        hidden_at: "2026-10-17T09:30:00Z",
        hidden_reason: "escalation",
      },
      analysis: { title: "Créer une taxe", pros: ["a"] },
      review: {
        outcome: "hide",
        escalate: true,
        explanation: "Menace",
        model: "mistral-large-latest",
        prompt_version: "1.2.0",
        reviewed_at: "2026-10-17T09:30:00Z",
      },
      reports: [{ reason: "contenu_problematique", details: null, reported_at: "2026-10-17" }],
      backfilled: true,
    });
    expect(content.idea).toMatchObject({
      status: "hidden",
      originalText: "Texte [donnée masquée]",
      submittedTerritory: null,
      hiddenReason: "escalation",
    });
    expect(content.review).toMatchObject({
      outcome: "hide",
      escalate: true,
      promptVersion: "1.2.0",
    });
    expect(content.reports).toEqual([
      { reason: "contenu_problematique", details: null, reportedAt: "2026-10-17" },
    ]);
    expect(content.backfilled).toBe(true);
    expect(analysisText(content.analysis)).toContain('"pros": [');
  });

  it("turns anything unexpected into empty values instead of failing", () => {
    expect(readSealContent(null)).toEqual({
      idea: null,
      analysis: null,
      review: null,
      reports: [],
      backfilled: false,
    });
    const odd = readSealContent({
      idea: { title: { nested: true }, status: 3 },
      review: [],
      reports: [null, "x", { reason: "autre" }],
      backfilled: "yes",
    });
    expect(odd.idea).toMatchObject({ title: null, status: "3" });
    expect(odd.review).toBeNull();
    expect(odd.reports).toEqual([{ reason: "autre", details: null, reportedAt: null }]);
    expect(odd.backfilled).toBe(false);
    expect(analysisText(null)).toBeNull();
  });
});
