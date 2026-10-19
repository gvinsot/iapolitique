import type {
  AdminAction,
  AdminDashboard,
  AdminEscalation,
  AdminIdea,
  AdminJob,
  AdminReply,
  AdminReport,
  AdminReviewRequest,
  AdminScreening,
  AdminSettings,
} from "@/api/types";

/** Fictitious administration data shaped like the admin API answers (AGENTS.md §13.2). */

export const IDEA_ID = "01a15179-046d-7418-b8de-ce2167202085";

export const dashboardFixture: AdminDashboard = {
  jobs: {
    queued: 12,
    running: 2,
    succeeded: 340,
    dead: 1,
    by_kind: [{ kind: "analyze_idea", status: "dead", count: 1 }],
  },
  screening_window_days: 30,
  screening: [
    {
      kind: "idea",
      total: 44,
      accepted: 30,
      rejected: 10,
      gray_zone: 4,
      errors: 0,
      acceptance_rate: 0.6818,
      rejection_rate: 0.2273,
      gray_zone_rate: 0.0909,
      second_opinion_accept: 3,
      second_opinion_reject: 1,
      labeled: 12,
    },
  ],
  ai_costs: {
    today_eur: 2.5,
    month_eur: 41.237,
    daily_budget_eur: 5,
    remaining_today_eur: 2.5,
    budget_reached: false,
    next_reset: "2026-10-19T22:00:00Z",
    by_purpose: [
      {
        purpose: "analysis_research",
        provider: "mistral",
        calls_today: 4,
        calls_month: 60,
        today_eur: 1.2,
        month_eur: 20.5,
      },
      {
        purpose: "screening",
        provider: "decision_service",
        calls_today: 40,
        calls_month: 900,
        today_eur: 0,
        month_eur: 0,
      },
    ],
  },
  queues: {
    escalations_pending: 1,
    review_requests_pending: 0,
    ideas_analysis_failed: 2,
    texts_second_opinion: 0,
    texts_submitted_stale: 0,
    ideas_hidden: 3,
    reports_awaiting_review: 4,
    jobs_dead: 1,
  },
};

const ideaRef = {
  id: IDEA_ID,
  slug: "taxe-exemple",
  title: "Créer une taxe sur un exemple",
  status: "hidden",
};

export const ideasFixture: AdminIdea[] = [
  {
    id: IDEA_ID,
    status: "hidden",
    slug: "taxe-exemple",
    title: "Créer une taxe sur un exemple",
    excerpt: "Il faudrait taxer les exemples, contact : [donnée masquée]",
    submitted_admin_level: "etat",
    created_at: "2026-10-10T08:00:00Z",
    status_changed_at: "2026-10-17T09:30:00Z",
    rejection_reason_code: null,
    hidden_reason: "report_review",
    hidden_at: "2026-10-17T09:30:00Z",
    parent_idea_id: null,
    proposal_count: 2,
    contribution_count: 1,
    score_eur: 1_200_000,
    reports_pending: 1,
    review_request_pending: true,
    escalation_pending: false,
  },
  {
    id: "01a15179-0000-7000-8000-000000000002",
    status: "published",
    slug: "idee-publiee",
    title: "Une idée publiée",
    excerpt: "Texte d’une idée publiée.",
    submitted_admin_level: null,
    created_at: "2026-10-11T08:00:00Z",
    status_changed_at: "2026-10-12T08:00:00Z",
    rejection_reason_code: null,
    hidden_reason: null,
    hidden_at: null,
    parent_idea_id: null,
    proposal_count: 1,
    contribution_count: 0,
    score_eur: 500_000,
    reports_pending: 0,
    review_request_pending: false,
    escalation_pending: false,
  },
];

export const reportsFixture: AdminReport[] = [
  {
    id: "r-1",
    idea: ideaRef,
    reason: "source_erronee",
    details: "La source [S2] ne mentionne pas ce chiffre.",
    status: "reviewed",
    created_at: "2026-10-16T10:00:00Z",
    rejection_reason_code: null,
    reporter_name: "Dominique",
    reporter_email: "dominique@example.org",
    good_faith: true,
    review: {
      id: "rv-1",
      outcome: "hide",
      escalate: true,
      explanation: "Le chiffre cité n’apparaît dans aucune source.",
      corrections: "",
      reviewed_at: "2026-10-17T09:30:00Z",
    },
  },
];

export const escalationsFixture: AdminEscalation[] = [
  {
    id: "e-1",
    idea: ideaRef,
    status: "pending",
    outcome: null,
    note: null,
    created_at: "2026-10-18T08:00:00Z",
    due_at: "2026-10-19T08:00:00Z",
    handled_at: null,
    review_explanation: "Propos menaçants envers un groupe de personnes.",
  },
];

export const reviewRequestsFixture: AdminReviewRequest[] = [
  {
    id: "rr-1",
    target_kind: "idea",
    target_id: "01a15179-0000-7000-8000-000000000009",
    idea: null,
    target_status: "rejected",
    text: "Supprimer une niche fiscale peu utilisée.",
    message: "Mon idée vise bien une recette publique.",
    decision: { measure: "rejected", reason_code: "off_topic" },
    status: "pending",
    resolution: null,
    note: null,
    created_at: "2026-10-18T12:00:00Z",
    handled_at: null,
  },
];

export const screeningsFixture: AdminScreening[] = [
  {
    id: "s-1",
    target_kind: "idea",
    target_id: IDEA_ID,
    created_at: "2026-10-18T12:00:00Z",
    grid_version: "1.1.0",
    outcome: "rejected",
    reason_code: "off_topic",
    error_code: null,
    latency_ms: 1420,
    criteria: [
      {
        key: "injection",
        probability: 0.98,
        reject_below: 0.3,
        accept_above: 0.7,
        verdict: "pass",
      },
      { key: "on_topic", probability: 0.12, reject_below: 0.3, accept_above: 0.7, verdict: "fail" },
    ],
    second_opinion: null,
    final_decision: "reject",
    decided_at: "2026-10-18T12:00:01Z",
    label: null,
    excerpt: "Allonger les vacances d’été.",
  },
  {
    id: "s-2",
    target_kind: "contribution",
    target_id: "01a15179-0000-7000-8000-000000000003",
    created_at: "2026-10-18T13:00:00Z",
    grid_version: "1.1.0",
    outcome: null,
    reason_code: null,
    error_code: "timeout",
    latency_ms: null,
    criteria: [],
    second_opinion: null,
    final_decision: null,
    decided_at: null,
    label: null,
    excerpt: null,
  },
];

export const settingsFixture: AdminSettings = {
  grid_version: "1.1.0",
  screening: [
    {
      kind: "idea",
      criteria: [
        {
          key: "on_topic",
          reason_code: "off_topic",
          reject_below: 0.3,
          accept_above: 0.75,
          default_reject_below: 0.3,
          default_accept_above: 0.7,
          adjusted: true,
        },
        {
          key: "realism",
          reason_code: "unrealistic",
          reject_below: 0.2,
          accept_above: 0.6,
          default_reject_below: 0.2,
          default_accept_above: 0.6,
          adjusted: false,
        },
      ],
    },
  ],
  dedup_similarity_threshold: { value: 0.95, default: 0.95, adjusted: false, minimum: 0.5 },
};

export const jobsFixture: AdminJob[] = [
  {
    id: 42,
    kind: "analyze_idea",
    status: "dead",
    priority: 1,
    attempts: 5,
    max_attempts: 5,
    payload: { idea_id: IDEA_ID, trigger: "initial" },
    dedupe_key: `analyze_idea:${IDEA_ID}`,
    run_after: "2026-10-18T10:00:00Z",
    last_error: "mistral: 400 invalid_request",
    created_at: "2026-10-18T09:00:00Z",
    updated_at: "2026-10-18T10:00:00Z",
    finished_at: "2026-10-18T10:00:00Z",
  },
];

export const actionsFixture: AdminAction[] = [
  {
    id: 7,
    action: "idea.hide",
    target_kind: "idea",
    target_id: IDEA_ID,
    details: { reason_length: 32 },
    created_at: "2026-10-17T09:30:00Z",
  },
];

export const repliesFixture: AdminReply[] = [
  {
    id: "rep-1",
    idea_id: IDEA_ID,
    author_label: "l’association des maires du département",
    body: "Nous rappelons que des groupements de commandes existent déjà.",
    requested_on: "2026-10-15",
    due_on: "2026-10-18",
    published_at: "2026-10-16T09:00:00Z",
    updated_at: "2026-10-16T09:00:00Z",
  },
];

/** A page of an admin list, as the API answers. */
export function adminPage<T>(items: T[]) {
  return { items, page: 1, per_page: 25, total: items.length, total_pages: 1 };
}
