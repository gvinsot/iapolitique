import type { Methodology, PublicStats, Taxonomy } from "@/api/types";

/** Fictitious answers of the reference endpoints (taxonomy, statistics, methodology). */

export const taxonomyFixture: Taxonomy = {
  version: "1.0.0",
  admin_levels: [
    { key: "etat", label: "État", description: "", category_groups: ["etat", "transverse"] },
    {
      key: "commune",
      label: "Commune (libellé de la taxonomie)",
      description: "",
      category_groups: ["collectivites", "transverse"],
    },
  ],
  categories: [
    {
      key: "achats_publics",
      label: "Achats publics",
      description: null,
      group: "transverse",
      admin_levels: ["etat", "commune"],
    },
    {
      key: "transports",
      label: "Transports",
      description: null,
      group: "collectivites",
      admin_levels: ["commune"],
    },
  ],
  impact_publics: [{ key: "entreprises", label: "Entreprises", description: null }],
  impact_effects: [{ key: "negative", label: "Défavorable", description: null }],
  idea_kinds: [{ key: "savings", label: "Économie de dépense", description: null }],
  confidence_levels: [{ key: "medium", label: "Moyenne", description: null }],
  legal_vehicles: [{ key: "deliberation", label: "Délibération locale", description: null }],
  report_reasons: [
    { key: "erreur_chiffrage", label: "Erreur dans le chiffrage", description: null },
  ],
  contribution_kinds: [{ key: "objection", label: "Objection argumentée", description: null }],
  rejection_reasons: [
    {
      code: "off_topic",
      message: "Votre texte ne propose pas de mesure pour les finances publiques.",
      rule: {
        article: "4.5",
        title: "Une mesure pour les finances publiques",
        url: "/regles#critere-sujet",
      },
    },
  ],
};

export const statsFixture: PublicStats = {
  ideas_received: 128,
  ideas_published: 42,
  ideas_in_progress: 5,
  ideas_rejected: 61,
  duplicates_merged: 17,
  contributions_integrated: 23,
  published_by_admin_level: [
    { key: "etat", count: 20 },
    { key: "commune", count: 22 },
  ],
  published_by_kind: [
    { key: "savings", count: 30 },
    { key: "revenue", count: 12 },
  ],
  last_published_on: "2026-10-18",
  computed_on: "2026-10-19",
};

export const methodologyFixture: Methodology = {
  screening_version: "1.1.0",
  taxonomy_version: "1.0.0",
  prompt_versions: { analysis: "1.1.0", analysis_research: "1.1.0", second_opinion: "1.1.0" },
  analysis_schema_version: "sha256:0123456789ab",
  export_schema_version: "1.0",
  cgu_version: "1.0",
  scoring: {
    version: "1",
    formula:
      "gain net annuel = gain médian − coût médian / 5\nscore = gain net annuel × faisabilité × confiance × délai",
    amortization_years: 5,
    max_feasibility: 5,
    delay_half_life_months: 24,
    confidence_factors: [
      { level: "low", factor: 0.5 },
      { level: "medium", factor: 0.75 },
      { level: "high", factor: 1 },
    ],
  },
  screening: {
    version: "1.1.0",
    sections: [
      {
        kind: "idea",
        label: "idée",
        criteria: [
          {
            key: "on_topic",
            question_type: "noul",
            instructions: "Le texte propose-t-il une mesure pour les finances publiques ?",
            answers: { yes: "oui", no: "non" },
            levels: [],
            pass_when: true,
            pass_from_level: null,
            reject_below: 0.3,
            accept_above: 0.7,
            thresholds_adjusted: true,
            reason_code: "off_topic",
            reason_message: "Votre texte ne propose pas de mesure pour les finances publiques.",
            rule: {
              article: "4.5",
              title: "Une mesure pour les finances publiques",
              url: "/regles#critere-sujet",
            },
          },
          {
            key: "realism",
            question_type: "score",
            instructions: "Quel est le réalisme de la mesure ?",
            answers: null,
            levels: ["irréaliste", "difficile", "faisable", "simple"],
            pass_when: null,
            pass_from_level: 1,
            reject_below: 0.2,
            accept_above: 0.6,
            thresholds_adjusted: false,
            reason_code: "unrealistic",
            reason_message: "La mesure paraît irréaliste.",
            rule: null,
          },
        ],
      },
    ],
  },
  files: [
    {
      file: "screening.yaml",
      kind: "screening",
      version: "1.1.0",
      content: 'version: "1.1.0"\nidea:\n  - key: on_topic',
    },
    {
      file: "analysis.md",
      kind: "prompt",
      version: "1.1.0",
      content: "---\nversion: 1.1.0\n---\nTu es un analyste <script>alert(1)</script>.",
    },
    {
      file: "analysis.schema.json",
      kind: "schema",
      version: null,
      content: '{ "type": "object" }',
    },
  ],
};
