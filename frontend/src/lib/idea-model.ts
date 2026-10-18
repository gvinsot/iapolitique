import type { EuroRange } from "./format";
import type {
  AdminLevel,
  Confidence,
  ContributionKind,
  IdeaKind,
  ImpactEffect,
  LegalVehicle,
} from "./labels";

/**
 * Display models of the idea components. Field names follow the analysis contract (AGENTS.md
 * §6.2) and the public API (§13.2) in snake_case, so the generated API types map onto them
 * field for field; components never depend on the transport types directly.
 */

export interface AmountEstimate extends EuroRange {
  /** Detailed calculation with [Sx] references. */
  calculation: string;
}

export interface Territory {
  name: string;
  /** INSEE code, empty when unknown. */
  insee_code: string;
}

export interface CategoryRef {
  key: string;
  label: string;
}

/** An idea in the ranking (card). */
export interface IdeaSummary {
  slug: string;
  title: string;
  summary: string;
  admin_level: AdminLevel;
  territory: Territory | null;
  category: CategoryRef;
  kind: IdeaKind;
  annual_gain_eur: EuroRange;
  feasibility: number;
  time_to_effect_months: number;
  confidence: Confidence;
  score: number;
  /** How many times the idea was proposed (merged duplicates included). */
  proposal_count: number;
  contribution_count: number;
  /** Date of the current analysis, `YYYY-MM-DD` or ISO date-time. */
  updated_at: string;
}

/** Components of the composite score (backend/src/domain/scoring.rs ScoreBreakdown). */
export interface ScoreBreakdownData {
  scoring_version: string;
  net_annual_eur: number;
  feasibility_factor: number;
  confidence_factor: number;
  delay_factor: number;
  score: number;
}

export interface Impact {
  /** Key of the public in taxonomy.yaml `impact_publics`. */
  public: string;
  /** French label of the public (taxonomy). */
  label: string;
  effect: ImpactEffect;
  explanation: string;
}

export interface Source {
  /** "S1", "S2"… as cited in the texts. */
  id: string;
  title: string;
  publisher: string;
  url: string;
  used_for: string;
  /** Publication date when known. */
  date?: string | null;
}

export interface Feasibility {
  score: number;
  obstacles: string[];
}

/** A contribution integrated by an analysis: neutral summary only, never the raw text. */
export interface ContributionConsidered {
  contribution_id: string;
  kind: ContributionKind;
  summary: string;
  taken_into_account: boolean;
  response: string;
}

/** Right of reply published by the administrator under an idea (LCEN art. 1-1 III). */
export interface RightOfReplyData {
  /** How the person replying is designated, as they asked (e.g. "La direction de …"). */
  author_label: string;
  body: string;
  published_at: string;
}

/** Full analysis of a published idea (AGENTS.md §6.2). */
export interface IdeaAnalysis {
  title: string;
  summary: string;
  improved_proposal: string;
  existing_situation: string;
  kind: IdeaKind;
  legal_vehicle: LegalVehicle;
  annual_gain_eur: AmountEstimate;
  implementation_cost_eur: AmountEstimate;
  time_to_effect_months: number;
  feasibility: Feasibility;
  confidence: Confidence;
  confidence_explanation: string;
  assumptions: string[];
  pros: string[];
  cons: string[];
  risks: string[];
  impacts: Impact[];
  sources: Source[];
  contributions_considered: ContributionConsidered[];
  changes_since_previous: string;
  /** Provenance, for the AI marking (AGENTS.md §7.5). */
  ai_model: string;
  generated_at: string;
  analysis_version: string;
}
