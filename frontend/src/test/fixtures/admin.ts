import type {
  DashboardStats,
  EscalationRow,
  HiddenIdeaRow,
  ReportRow,
  ReviewRequestRow,
  ScreeningDecisionRow,
  ThresholdSettings,
} from "@/lib/admin-model";

/** Fictitious administration data, for component tests and for wiring the admin pages. */

export const dashboardStatsFixture: DashboardStats = {
  jobs: { queued: 12, running: 2, failed: 1 },
  screening: { window_days: 7, accepted: 30, rejected: 10, gray_zone: 4 },
  ai_costs: { today_eur: 2.5, month_eur: 41.237, daily_budget_eur: 5 },
  escalations_pending: 1,
  review_requests_pending: 0,
};

export const hiddenIdeasFixture: HiddenIdeaRow[] = [
  {
    slug: "taxe-exemple",
    title: "Créer une taxe sur un exemple",
    hidden_on: "2026-10-17",
    origin: "report_review",
    reason: "Chiffrage fondé sur une source inexistante.",
  },
];

export const reportsFixture: ReportRow[] = [
  {
    id: "r-1",
    idea_slug: "taxe-exemple",
    idea_title: "Créer une taxe sur un exemple",
    reason: "source_erronee",
    details: "La source [S2] ne mentionne pas ce chiffre.",
    received_on: "2026-10-16",
    status: "reviewed",
    outcome: "hide",
    escalate: false,
  },
  {
    id: "r-2",
    idea_slug: "mutualiser-achats",
    idea_title: "Mutualiser les achats des communes",
    reason: "contenu_problematique",
    details: null,
    received_on: "2026-10-18",
    status: "accepted",
    outcome: null,
    escalate: true,
  },
];

export const escalationsFixture: EscalationRow[] = [
  {
    id: "e-1",
    idea_slug: "mutualiser-achats",
    idea_title: "Mutualiser les achats des communes",
    escalated_at: "2026-10-18T08:00:00Z",
    explanation: "Le texte original contient une menace.",
    status: "pending",
  },
  {
    id: "e-2",
    idea_slug: "taxe-exemple",
    idea_title: "Créer une taxe sur un exemple",
    escalated_at: "2026-10-10T08:00:00Z",
    explanation: "Propos diffamatoires.",
    status: "transmitted",
  },
];

export const reviewRequestsFixture: ReviewRequestRow[] = [
  {
    id: "rr-1",
    kind: "idea",
    text: "Réduire les dépenses de communication des ministères.",
    reason_code: "off_topic",
    message: "Mon idée réduit bien une dépense.",
    requested_on: "2026-10-18",
    status: "pending",
  },
];

export const screeningDecisionsFixture: ScreeningDecisionRow[] = [
  {
    id: "d-1",
    target: "idea",
    decided_at: "2026-10-18T09:15:00Z",
    outcome: "rejected",
    excerpt: "Supprimer toutes les dépenses inutiles.",
    grid_version: "1.1.0",
    label: null,
    criteria: [
      {
        key: "respectful",
        probability: 0.97,
        reject_below: 0.3,
        accept_above: 0.6,
        verdict: "pass",
      },
      { key: "realism", probability: 0.12, reject_below: 0.3, accept_above: 0.5, verdict: "fail" },
    ],
  },
  {
    id: "d-2",
    target: "contribution",
    decided_at: "2026-10-17T18:00:00Z",
    outcome: "gray_zone",
    excerpt: null,
    grid_version: "1.1.0",
    label: "correct",
    criteria: [
      { key: "relevant", probability: 0.45, reject_below: 0.3, accept_above: 0.6, verdict: "gray" },
    ],
  },
];

export const thresholdSettingsFixture: ThresholdSettings = {
  idea: [
    { key: "respectful", reject_below: 0.3, accept_above: 0.6 },
    { key: "realism", reject_below: 0.3, accept_above: 0.5 },
  ],
  contribution: [{ key: "relevant", reject_below: 0.3, accept_above: 0.6 }],
  report: [{ key: "relevant", reject_below: 0.25, accept_above: 0.55 }],
  dedup_similarity_threshold: 0.95,
};
