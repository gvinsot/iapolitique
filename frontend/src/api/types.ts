import type { components } from "./schema";

/**
 * Named aliases of the API types generated from the backend OpenAPI spec (schema.d.ts, never
 * edited by hand). Pages and components import their transport types from here.
 */
type Schemas = components["schemas"];

// --- Public: ranking and idea page ------------------------------------------------------------
export type IdeaList = Schemas["IdeaList"];
export type IdeaListItem = Schemas["IdeaListItem"];
export type IdeaDetail = Schemas["IdeaDetail"];
export type IdeaAnalysis = Schemas["IdeaAnalysis"];
export type IdeaRedirect = Schemas["IdeaRedirect"];
export type IdeaSort = Schemas["IdeaSort"];
export type SearchMatch = Schemas["SearchMatch"];
export type AdminLevel = Schemas["AdminLevel"];
export type IdeaKind = Schemas["IdeaKind"];
export type Confidence = Schemas["Confidence"];
export type LegalVehicle = Schemas["LegalVehicle"];
export type ImpactDirection = Schemas["ImpactDirection"];
export type ContributionKind = Schemas["ContributionKind"];
export type ReportReason = Schemas["ReportReason"];
export type CategoryRef = Schemas["CategoryRef"];
export type TerritoryRef = Schemas["TerritoryRef"];
export type EuroRange = Schemas["EuroRange"];
export type AmountEstimate = Schemas["AmountEstimate"];
export type FeasibilityAssessment = Schemas["FeasibilityAssessment"];
export type ImpactAssessment = Schemas["ImpactAssessment"];
export type SourceReference = Schemas["SourceReference"];
export type ScoreComponents = Schemas["ScoreComponents"];
export type ContributionSummary = Schemas["ContributionSummary"];
export type RightOfReply = Schemas["RightOfReply"];
export type AiMarking = Schemas["AiMarking"];
export type OriginalText = Schemas["OriginalText"];

// --- Public: submissions and tracking ---------------------------------------------------------
export type SubmissionResponse = Schemas["SubmissionResponse"];
export type StatementOfReasons = Schemas["StatementOfReasons"];
export type RuleReference = Schemas["RuleReference"];
export type Measure = Schemas["Measure"];
export type TargetKind = Schemas["TargetKind"];
export type TrackedItem = Schemas["TrackedItem"];
export type TrackedIdea = Schemas["TrackedIdea"];
export type TrackedReviewRequest = Schemas["TrackedReviewRequest"];
export type ReviewOutcome = Schemas["ReviewOutcome"];
export type ReviewRequestStatus = Schemas["ReviewRequestStatus"];

// --- Public: reference data -------------------------------------------------------------------
export type LegalInfo = Schemas["LegalInfo"];
export type LegalPages = Schemas["LegalPages"];
export type PublicStats = Schemas["PublicStats"];
export type KeyCount = Schemas["KeyCount"];
export type Taxonomy = Schemas["TaxonomyResponse"];
export type TaxonomyItem = Schemas["TaxonomyItem"];
export type CategoryItem = Schemas["CategoryItem"];
export type Methodology = Schemas["MethodologyResponse"];
export type MethodologyFile = Schemas["MethodologyFile"];
export type MethodologyModel = Schemas["MethodologyModel"];
export type ScreeningSection = Schemas["ScreeningSection"];
export type ScreeningCriterion = Schemas["ScreeningCriterion"];
export type ScoringMethod = Schemas["ScoringMethod"];

// --- Administration ---------------------------------------------------------------------------
export type AdminDashboard = Schemas["AdminDashboard"];
export type AdminAlert = Schemas["AdminAlert"];
export type AdminBulkReanalysis = Schemas["AdminBulkReanalysis"];
export type AdminScreeningRates = Schemas["AdminScreeningRates"];
export type AdminAiCosts = Schemas["AdminAiCosts"];
export type AdminQueues = Schemas["AdminQueues"];
export type AdminJob = Schemas["AdminJob"];
export type AdminJobPage = Schemas["AdminJobPage"];
export type JobKind = Schemas["JobKind"];
export type JobStatus = Schemas["JobStatus"];
export type AdminIdea = Schemas["AdminIdea"];
export type AdminIdeaRef = Schemas["AdminIdeaRef"];
export type IdeaStatus = Schemas["IdeaStatus"];
export type AdminReport = Schemas["AdminReport"];
export type ReportStatus = Schemas["ReportStatus"];
export type AdminEscalation = Schemas["AdminEscalation"];
export type AdminEscalationSeal = Schemas["AdminEscalationSeal"];
export type AdminEscalationSealSummary = Schemas["AdminEscalationSealSummary"];
export type EscalationOutcome = Schemas["EscalationOutcome"];
export type AdminReviewRequest = Schemas["AdminReviewRequest"];
export type ReviewResolution = Schemas["ReviewResolution"];
export type AdminScreening = Schemas["AdminScreening"];
export type AdminCriterionScore = Schemas["AdminCriterionScore"];
export type AdminScreeningLabelRequest = Schemas["AdminScreeningLabelRequest"];
export type ScreeningOutcome = Schemas["ScreeningOutcome"];
export type CriterionVerdict = Schemas["CriterionVerdict"];
export type Decision = Schemas["Decision"];
export type LabelVerdict = Schemas["LabelVerdict"];
export type AdminSettings = Schemas["AdminSettings"];
export type AdminThresholdSection = Schemas["AdminThresholdSection"];
export type AdminCriterionThresholds = Schemas["AdminCriterionThresholds"];
export type AdminThresholdsUpdate = Schemas["AdminThresholdsUpdate"];
export type AdminReply = Schemas["AdminReply"];
export type AdminReplyContent = Schemas["AdminReplyContent"];
export type AdminAction = Schemas["AdminAction"];
