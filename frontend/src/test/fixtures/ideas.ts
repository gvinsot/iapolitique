import type {
  ContributionConsidered,
  IdeaAnalysis,
  IdeaDetail,
  IdeaSummary,
  Impact,
  RightOfReplyData,
  ScoreBreakdownData,
} from "@/lib/idea-model";

/**
 * Fictitious data shaped like the API answers, for component tests and for wiring the pages.
 * Amounts are invented and carry no meaning.
 */

export const ideaSummaryFixture: IdeaSummary = {
  slug: "mutualiser-achats-fournitures-communes",
  title: "Mutualiser les achats de fournitures des communes d’un même département",
  summary:
    "Regrouper les commandes de fournitures courantes des communes au sein d’une centrale d’achat départementale afin d’obtenir de meilleurs prix et de réduire les coûts de procédure.",
  admin_level: "commune",
  territory: { name: "Gironde", insee_code: "33" },
  category: { key: "achats_publics", label: "Achats publics" },
  kind: "savings",
  annual_gain_eur: { low: 2_000_000, median: 5_000_000, high: 9_000_000 },
  feasibility: 4,
  time_to_effect_months: 12,
  confidence: "medium",
  score: 1_920_000,
  points: 410,
  proposal_count: 3,
  contribution_count: 2,
  updated_at: "2026-10-12T08:30:00Z",
};

export const scoreBreakdownFixture: ScoreBreakdownData = {
  label: "score calculé par algorithme",
  scoring_version: "1",
  net_annual_eur: 4_800_000,
  feasibility_factor: 0.8,
  confidence_factor: 0.75,
  delay_factor: 0.6667,
  score: 1_920_000,
  points: 410,
  max_points: 1000,
};

export const impactPublics = [
  ["menages_modestes", "Ménages modestes"],
  ["classes_moyennes", "Classes moyennes"],
  ["menages_aises", "Ménages aisés"],
  ["entreprises", "Entreprises"],
  ["agents_publics", "Agents publics"],
  ["usagers_services_publics", "Usagers des services publics"],
  ["territoires_ruraux", "Territoires ruraux"],
  ["territoires_urbains", "Territoires urbains"],
  ["environnement", "Environnement"],
] as const;

const effects = ["neutral", "neutral", "neutral", "negative", "uncertain", "positive"] as const;

export const impactsFixture: Impact[] = impactPublics.map(([key, label], index) => ({
  public: key,
  label,
  effect: effects[index % effects.length] ?? "neutral",
  explanation: `Effet estimé pour ce public [S${(index % 2) + 1}].`,
}));

export const analysisFixture: IdeaAnalysis = {
  improved_proposal:
    "Créer une centrale d’achat départementale facultative pour les fournitures courantes, ouverte aux communes volontaires, avec un catalogue commun [S1].",
  existing_situation:
    "Des groupements de commandes existent déjà dans certains départements [S2] ; le gain chiffré est supplémentaire.",
  legal_vehicle: "deliberation",
  annual_gain_eur: {
    low: 2_000_000,
    median: 5_000_000,
    high: 9_000_000,
    calculation: "Dépenses de fournitures des communes × 10 % d’économie moyenne [S1].",
  },
  implementation_cost_eur: {
    low: 500_000,
    median: 1_000_000,
    high: 2_000_000,
    calculation: "Mise en place de la centrale et du catalogue [S2].",
  },
  time_to_effect_months: 12,
  feasibility: { score: 4, obstacles: ["Adhésion volontaire des communes"] },
  confidence: "medium",
  confidence_explanation: "Données publiques partielles [S1].",
  assumptions: ["Taux d’adhésion de 60 % [S1]", "Économie moyenne de 10 % sur les prix [S2]"],
  pros: ["Prix unitaires plus bas [S1]", "Procédures allégées pour les petites communes"],
  cons: ["Moins de choix pour les achats locaux [S2]"],
  risks: ["Dépendance à un nombre réduit de fournisseurs"],
  impacts: impactsFixture,
  sources: [
    {
      id: "S1",
      title: "Rapport sur la commande publique locale",
      publisher: "Cour des comptes",
      url: "https://www.ccomptes.fr/fr/publications/exemple",
      used_for: "Montant des dépenses de fournitures",
    },
    {
      id: "S2",
      title: "Les groupements de commandes",
      publisher: "Direction des affaires juridiques",
      url: "javascript:alert(1)",
      used_for: "Situation existante",
    },
  ],
  changes_since_previous: "",
};

export const contributionsFixture: ContributionConsidered[] = [
  {
    kind: "amelioration",
    summary: "Ouvrir la centrale aux établissements publics locaux.",
    taken_into_account: true,
    response: "Intégré dans la proposition améliorée.",
    integrated_on: "2026-10-12",
  },
  {
    kind: "objection",
    summary: "Risque pour les commerces de proximité.",
    taken_into_account: false,
    response: "",
    integrated_on: "2026-10-12",
  },
];

export const rightOfReplyFixture: RightOfReplyData = {
  author_label: "l’association des maires du département",
  body: "Nous rappelons que des groupements de commandes existent déjà.\nIls couvrent 40 % des communes.",
  published_at: "2026-10-15T09:00:00Z",
  updated_at: "2026-10-15T09:00:00Z",
};

/** Answer of GET /api/ideas/{slug} for the card above. */
export const ideaDetailFixture: IdeaDetail = {
  slug: ideaSummaryFixture.slug,
  title: ideaSummaryFixture.title,
  summary: ideaSummaryFixture.summary,
  admin_level: ideaSummaryFixture.admin_level,
  territory: ideaSummaryFixture.territory,
  category: ideaSummaryFixture.category,
  kind: ideaSummaryFixture.kind,
  original: {
    text: "Les communes devraient acheter ensemble leurs fournitures. Contact : [donnée masquée]",
    personal_data_masked: true,
    attribution: "Contribution anonyme – iapolitique.fr",
    notice: "Contribution d’un internaute (texte original, données personnelles masquées)",
  },
  analysis: analysisFixture,
  score: scoreBreakdownFixture,
  contributions: contributionsFixture,
  rights_of_reply: [rightOfReplyFixture],
  ai: {
    ai_generated: true,
    provider: "Mistral AI",
    model: "mistral-large-latest",
    generated_at: "2026-10-12T08:30:00Z",
    analysis_version: 3,
    prompt_version: "1.1.0",
    research_prompt_version: "1.1.0",
    schema_version: "sha256:0123456789ab",
    scoring_version: "1",
    web_search_count: 4,
    human_reviewed: false,
    notice:
      "Analyse générée automatiquement par intelligence artificielle (Mistral AI), sans relecture humaine. Elle peut contenir des erreurs ; vérifiez les sources citées. Ce n’est pas une position officielle.",
  },
  proposal_count: ideaSummaryFixture.proposal_count,
  contribution_count: ideaSummaryFixture.contribution_count,
  published_on: "2026-10-01",
  updated_at: "2026-10-12T08:30:00Z",
  canonical_url: `https://iapolitique.fr/idees/${ideaSummaryFixture.slug}`,
};
