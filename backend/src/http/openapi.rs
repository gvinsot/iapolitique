//! OpenAPI specification (utoipa). The full spec (public + admin) is committed in
//! `frontend/src/api/openapi.json`, from which the frontend generates its types (§13.3); a
//! test fails when the committed file is outdated. Regenerate with
//! `cargo run -- openapi --output ../frontend/src/api/openapi.json`.

use std::collections::BTreeSet;

use axum::Json;
use axum::response::IntoResponse;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use super::admin::actions::{self as admin_actions, AdminAction, AdminActionPage};
use super::admin::alerts::{self as admin_alerts, AdminMistralKeyCheck, MistralKeyCheckResult};
use super::admin::dashboard::{
    self as admin_dashboard, AdminAiCosts, AdminAlert, AdminDashboard, AdminJobCount,
    AdminJobSummary, AdminPurposeCost, AdminQueues, AdminScreeningRates,
};
use super::admin::ideas::{
    self as admin_ideas, AdminBulkReanalysis, AdminIdea, AdminIdeaPage, AdminReanalysis,
    AdminReason,
};
use super::admin::jobs::{self as admin_jobs, AdminJob, AdminJobLog, AdminJobLogs, AdminJobPage};
use super::admin::moderation::{
    self as admin_moderation, AdminEscalation, AdminEscalationHandling, AdminEscalationPage,
    AdminEscalationSeal, AdminEscalationSealPage, AdminEscalationSealSummary, AdminReport,
    AdminReportPage, AdminReportReview, AdminReviewRequest, AdminReviewRequestPage,
    AdminReviewResolution, AdminReviewedDecision,
};
use super::admin::replies::{self as admin_replies, AdminReply, AdminReplyContent};
use super::admin::screenings::{
    self as admin_screenings, AdminCriterionScore, AdminScreening, AdminScreeningLabel,
    AdminScreeningLabelRequest, AdminScreeningPage, AdminSecondOpinion, CalibrationPart,
};
use super::admin::settings::{
    self as admin_settings, AdminCriterionThresholds, AdminDedupThreshold, AdminDedupUpdate,
    AdminSettings, AdminThresholdOverride, AdminThresholdSection, AdminThresholdsUpdate,
};
use super::admin::{self, AdminIdeaRef, AdminSession};
use super::antispam;
use super::error::{ErrorBody, ErrorDetail};
use super::exports;
use super::health::{self, HealthResponse};
use super::ideas::{
    self, AiMarking, AmountEstimate, CategoryRef, ContributionSummary, EuroRange,
    FeasibilityAssessment, IdeaAnalysis, IdeaDetail, IdeaList, IdeaListItem, IdeaRedirect,
    ImpactAssessment, OriginalText, RightOfReply, ScoreComponents, SearchMatch, SourceReference,
    TerritoryRef,
};
use super::legal::{self, LegalInfo, LegalPages, LegalPublisher};
use super::reference::{
    self, AdminLevelItem, CategoryItem, ConfidenceFactor, KeyCount, MethodologyModel,
    MethodologyModels, MethodologyResponse, PublicStats, RejectionReason, ScoringMethod,
    ScreeningCriterion, ScreeningMethod, ScreeningSection, TaxonomyItem, TaxonomyResponse,
    YesNoLabels,
};
use super::submissions::{
    self, ContributionSubmission, ContributionSubmissionKind, IdeaSubmission, ReportSubmission,
    SubmissionResponse,
};
use super::tracking::{
    self, DeletionResponse, ReviewRequestBody, TrackedIdea, TrackedItem, TrackedReviewRequest,
    TrackingRequest,
};
use crate::ai::prompts::{MethodologyFile, MethodologyFileKind};
use crate::antispam::altcha::AltchaChallenge;
use crate::db::portal::IdeaSort;
use crate::domain::rules::RuleReference;
use crate::domain::status::{IdeaStatus, ReportStatus};
use crate::domain::types::{
    AdminLevel, Confidence, ContributionKind, CriterionVerdict, Decision, EscalationOutcome,
    IdeaKind, ImpactDirection, LabelVerdict, LegalVehicle, ReasoningEffort, ReportReason,
    ReviewOutcome, ReviewRequestStatus, ReviewResolution, ScreeningOutcome, TargetKind,
};
use crate::jobs::{JobKind, JobStatus};
use crate::moderation::statement::{Measure, StatementOfReasons};

/// Prefix of the admin routes, removed from the spec served on the public port.
pub const ADMIN_PREFIX: &str = "/api/admin";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "iapolitique API",
        description = "API de la plateforme iapolitique : dépôt, filtrage, analyse et classement \
                       d'idées pour réduire les dépenses ou augmenter les recettes publiques.",
        license(name = "MIT")
    ),
    paths(
        health::health,
        health::ready,
        openapi_json,
        antispam::altcha_challenge,
        ideas::list,
        ideas::detail,
        legal::legal,
        exports::ideas_json,
        exports::ideas_csv,
        exports::license,
        exports::metadata,
        reference::methodology,
        reference::taxonomy,
        reference::stats,
        submissions::submit_idea,
        submissions::submit_contribution,
        submissions::submit_report,
        tracking::track,
        tracking::delete,
        tracking::request_review,
        admin::session,
        admin_dashboard::dashboard,
        admin_alerts::check_mistral_key,
        admin_jobs::list,
        admin_jobs::retry,
        admin_jobs::logs,
        admin_ideas::list,
        admin_ideas::hide,
        admin_ideas::republish,
        admin_ideas::reanalyze,
        admin_ideas::reanalyze_failed,
        admin_moderation::reports,
        admin_moderation::escalations,
        admin_moderation::handle_escalation,
        admin_moderation::escalation_seal,
        admin_moderation::escalation_seals,
        admin_moderation::review_requests,
        admin_moderation::resolve_review_request,
        admin_screenings::list,
        admin_screenings::label,
        admin_screenings::unlabel,
        admin_screenings::export,
        admin_settings::get,
        admin_settings::update_thresholds,
        admin_settings::update_dedup,
        admin_replies::list,
        admin_replies::create,
        admin_replies::update,
        admin_replies::delete,
        admin_actions::list
    ),
    components(schemas(
        ErrorBody,
        ErrorDetail,
        HealthResponse,
        AdminSession,
        AltchaChallenge,
        AdminLevel,
        IdeaSubmission,
        ContributionSubmission,
        ContributionSubmissionKind,
        ReportSubmission,
        ReportReason,
        SubmissionResponse,
        StatementOfReasons,
        Measure,
        RuleReference,
        TrackingRequest,
        ReviewRequestBody,
        TrackedItem,
        TrackedIdea,
        TrackedReviewRequest,
        DeletionResponse,
        TargetKind,
        ReviewOutcome,
        ReviewRequestStatus,
        IdeaSort,
        IdeaKind,
        Confidence,
        ReasoningEffort,
        LegalVehicle,
        ImpactDirection,
        ContributionKind,
        SearchMatch,
        IdeaList,
        IdeaListItem,
        TerritoryRef,
        CategoryRef,
        EuroRange,
        IdeaDetail,
        IdeaRedirect,
        OriginalText,
        IdeaAnalysis,
        AmountEstimate,
        FeasibilityAssessment,
        ImpactAssessment,
        SourceReference,
        ScoreComponents,
        AiMarking,
        ContributionSummary,
        RightOfReply,
        MethodologyResponse,
        MethodologyModels,
        MethodologyModel,
        MethodologyFile,
        MethodologyFileKind,
        ScoringMethod,
        ConfidenceFactor,
        ScreeningMethod,
        ScreeningSection,
        ScreeningCriterion,
        YesNoLabels,
        TaxonomyResponse,
        TaxonomyItem,
        AdminLevelItem,
        CategoryItem,
        RejectionReason,
        PublicStats,
        KeyCount,
        LegalInfo,
        LegalPublisher,
        LegalPages,
        AdminDashboard,
        AdminAlert,
        AdminMistralKeyCheck,
        MistralKeyCheckResult,
        AdminBulkReanalysis,
        AdminEscalationSeal,
        AdminEscalationSealSummary,
        AdminEscalationSealPage,
        AdminJobSummary,
        AdminJobCount,
        AdminScreeningRates,
        AdminAiCosts,
        AdminPurposeCost,
        AdminQueues,
        AdminJob,
        AdminJobPage,
        AdminJobLog,
        AdminJobLogs,
        JobKind,
        JobStatus,
        AdminIdeaRef,
        AdminIdea,
        AdminIdeaPage,
        AdminReason,
        AdminReanalysis,
        IdeaStatus,
        ReportStatus,
        AdminReport,
        AdminReportPage,
        AdminReportReview,
        AdminEscalation,
        AdminEscalationPage,
        AdminEscalationHandling,
        EscalationOutcome,
        AdminReviewRequest,
        AdminReviewRequestPage,
        AdminReviewedDecision,
        AdminReviewResolution,
        ReviewResolution,
        AdminScreening,
        AdminScreeningPage,
        AdminCriterionScore,
        AdminSecondOpinion,
        AdminScreeningLabel,
        AdminScreeningLabelRequest,
        CalibrationPart,
        LabelVerdict,
        Decision,
        CriterionVerdict,
        ScreeningOutcome,
        AdminSettings,
        AdminThresholdSection,
        AdminCriterionThresholds,
        AdminDedupThreshold,
        AdminThresholdsUpdate,
        AdminThresholdOverride,
        AdminDedupUpdate,
        AdminReply,
        AdminReplyContent,
        AdminAction,
        AdminActionPage
    )),
    modifiers(&AdminSecurity),
    tags(
        (name = "system", description = "Santé du service et spécification"),
        (name = "antispam", description = "Protection anti-abus des formulaires (ALTCHA)"),
        (name = "portal", description = "Classement et pages des idées publiées"),
        (name = "reference", description = "Méthodologie, taxonomie et compteurs publics"),
        (name = "exports", description = "Exports open data (Licence Ouverte 2.0)"),
        (name = "submissions", description = "Dépôt d'idées, de contributions et de signalements"),
        (name = "tracking", description = "Suivi anonyme d'un envoi par son jeton"),
        (name = "admin", description = "Administration (réseau local uniquement, jeton requis)")
    )
)]
pub struct ApiDoc;

/// Declares the bearer scheme of the admin API.
struct AdminSecurity;

impl Modify for AdminSecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "admin_token",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("ADMIN_API_TOKEN"))
                    .build(),
            ),
        );
    }
}

/// The complete specification (public and admin routes).
pub fn full_spec() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

/// Adds to `out` the schema names referenced (`$ref`) anywhere in `value`.
fn collect_refs(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(name) = map
                .get("$ref")
                .and_then(serde_json::Value::as_str)
                .and_then(|r| r.strip_prefix("#/components/schemas/"))
            {
                out.insert(name.to_owned());
            }
            map.values().for_each(|v| collect_refs(v, out));
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_refs(v, out)),
        _ => {}
    }
}

/// Schemas reachable from the paths of `spec`.
fn reachable_schemas(spec: &utoipa::openapi::OpenApi) -> BTreeSet<String> {
    let value = serde_json::to_value(spec).expect("the OpenAPI spec serializes");
    let schemas = &value["components"]["schemas"];
    let mut reachable = BTreeSet::new();
    collect_refs(&value["paths"], &mut reachable);
    let mut pending: Vec<String> = reachable.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        let mut found = BTreeSet::new();
        collect_refs(&schemas[name.as_str()], &mut found);
        for name in found {
            if reachable.insert(name.clone()) {
                pending.push(name);
            }
        }
    }
    reachable
}

/// The specification served on the public port: admin routes, their schemas and the admin
/// scheme removed.
pub fn public_spec() -> utoipa::openapi::OpenApi {
    let mut spec = full_spec();
    spec.paths
        .paths
        .retain(|path, _| !path.starts_with(ADMIN_PREFIX));
    let reachable = reachable_schemas(&spec);
    if let Some(components) = spec.components.as_mut() {
        components.security_schemes.clear();
        components
            .schemas
            .retain(|name, _| reachable.contains(name));
    }
    spec.tags = spec
        .tags
        .map(|tags| tags.into_iter().filter(|t| t.name != "admin").collect());
    spec
}

/// Pretty JSON with a trailing newline, as committed in the frontend.
pub fn to_json(spec: &utoipa::openapi::OpenApi) -> String {
    let mut json = serde_json::to_string_pretty(spec).expect("the OpenAPI spec serializes");
    json.push('\n');
    json
}

/// The OpenAPI specification of this API.
#[utoipa::path(
    get,
    path = "/api/openapi.json",
    tag = "system",
    operation_id = "getOpenApi",
    responses((status = 200, description = "Spécification OpenAPI.", body = serde_json::Value))
)]
pub async fn openapi_json() -> impl IntoResponse {
    Json(public_spec())
}

/// Same route on the admin port, with the admin routes.
pub async fn openapi_json_full() -> impl IntoResponse {
    Json(full_spec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn committed_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/src/api/openapi.json")
    }

    /// Fails when `frontend/src/api/openapi.json` differs from the generated spec. Set
    /// `UPDATE_OPENAPI=1` to rewrite the file instead.
    #[test]
    fn committed_spec_is_up_to_date() {
        let generated = to_json(&full_spec());
        let path = committed_path();
        if std::env::var_os("UPDATE_OPENAPI").is_some() {
            std::fs::write(&path, &generated).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!(
                "cannot read {}: {err}. Generate it with \
                 `cargo run -- openapi --output ../frontend/src/api/openapi.json`",
                path.display()
            )
        });
        assert!(
            committed.replace("\r\n", "\n") == generated,
            "{} is outdated: run `cargo run -- openapi --output ../frontend/src/api/openapi.json` \
             (or UPDATE_OPENAPI=1 cargo test) and commit it with `npm run gen:api` output",
            path.display()
        );
    }

    #[test]
    fn full_spec_documents_every_route() {
        let spec = full_spec();
        let paths: Vec<&str> = spec.paths.paths.keys().map(String::as_str).collect();
        for path in [
            "/api/health",
            "/api/ready",
            "/api/openapi.json",
            "/api/altcha/challenge",
            "/api/ideas",
            "/api/ideas/{slug}",
            "/api/ideas/{slug}/contributions",
            "/api/ideas/{slug}/reports",
            "/api/legal",
            "/api/exports/ideas.json",
            "/api/exports/ideas.csv",
            "/api/exports/LICENSE",
            "/api/exports/metadata.json",
            "/api/methodology",
            "/api/taxonomy",
            "/api/stats",
            "/api/track",
            "/api/track/delete",
            "/api/track/review-request",
            "/api/admin/session",
            "/api/admin/dashboard",
            "/api/admin/jobs",
            "/api/admin/jobs/{id}/retry",
            "/api/admin/ideas",
            "/api/admin/ideas/{id}/hide",
            "/api/admin/ideas/{id}/republish",
            "/api/admin/ideas/{id}/reanalyze",
            "/api/admin/ideas/reanalyze-failed",
            "/api/admin/reports",
            "/api/admin/escalations",
            "/api/admin/escalations/{id}/handle",
            "/api/admin/escalations/{id}/seal",
            "/api/admin/escalation-seals",
            "/api/admin/review-requests",
            "/api/admin/review-requests/{id}/resolve",
            "/api/admin/screenings",
            "/api/admin/screenings/{id}/label",
            "/api/admin/screenings/export.jsonl",
            "/api/admin/settings",
            "/api/admin/settings/screening-thresholds",
            "/api/admin/settings/dedup-threshold",
            "/api/admin/ideas/{id}/replies",
            "/api/admin/replies/{id}",
            "/api/admin/actions",
        ] {
            assert!(paths.contains(&path), "{path} missing");
        }
        assert!(paths.iter().all(|p| p.starts_with("/api/")));
        let schemes = &spec.components.as_ref().unwrap().security_schemes;
        assert!(schemes.contains_key("admin_token"));
    }

    #[test]
    fn public_spec_hides_admin_routes() {
        let spec = public_spec();
        assert!(
            spec.paths
                .paths
                .keys()
                .all(|p| !p.starts_with(ADMIN_PREFIX))
        );
        assert!(spec.paths.paths.contains_key("/api/health"));
        let components = spec.components.as_ref().unwrap();
        assert!(components.security_schemes.is_empty());
        assert!(components.schemas.contains_key("ErrorBody"));
        let json = to_json(&spec);
        assert!(!json.contains("admin_token"));
        // Admin-only schemas are not published on the public port.
        for name in [
            "AdminSession",
            "AdminDashboard",
            "AdminJob",
            "AdminJobPage",
            "AdminJobLogs",
            "JobStatus",
        ] {
            assert!(!components.schemas.contains_key(name), "{name}");
        }
        assert!(components.schemas.contains_key("AdminLevel"));
        assert!(components.schemas.contains_key("IdeaDetail"));
    }
}
