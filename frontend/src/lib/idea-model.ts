import type {
  AmountEstimate,
  CategoryRef,
  ContributionSummary,
  FeasibilityAssessment,
  IdeaAnalysis,
  IdeaDetail,
  IdeaListItem,
  ImpactAssessment,
  RightOfReply,
  ScoreComponents,
  SourceReference,
  TerritoryRef,
} from "@/api/types";

/**
 * Display models of the idea components: the API types themselves (GET /api/ideas and
 * GET /api/ideas/{slug}, AGENTS.md §13.2), under the names the components use.
 */

export type { AmountEstimate, CategoryRef, IdeaAnalysis, IdeaDetail };

/** An idea in the ranking (card). */
export type IdeaSummary = IdeaListItem;
/** Territory targeted by an idea; `insee_code` is empty when unknown. */
export type Territory = TerritoryRef;
/** Components of the composite score (backend/src/domain/scoring.rs). */
export type ScoreBreakdownData = ScoreComponents;
/** Expected effect for one public of the taxonomy. */
export type Impact = ImpactAssessment;
/** A source cited by the analysis ("S1"…); its URL comes from the AI (see lib/citations.ts). */
export type Source = SourceReference;
export type Feasibility = FeasibilityAssessment;
/** A contribution integrated by an analysis: neutral summary only, never the raw text. */
export type ContributionConsidered = ContributionSummary;
/** Right of reply published by the administrator under an idea (LCEN art. 1-1 III). */
export type RightOfReplyData = RightOfReply;
