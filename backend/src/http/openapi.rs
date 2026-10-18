//! OpenAPI specification (utoipa). The full spec (public + admin) is committed in
//! `frontend/src/api/openapi.json`, from which the frontend generates its types (§13.3); a
//! test fails when the committed file is outdated. Regenerate with
//! `cargo run -- openapi --output ../frontend/src/api/openapi.json`.

use axum::Json;
use axum::response::IntoResponse;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use super::admin::{self, AdminSession};
use super::antispam;
use super::error::{ErrorBody, ErrorDetail};
use super::health::{self, HealthResponse};
use super::ideas::{
    self, AiMarking, AmountEstimate, CategoryRef, ContributionSummary, EuroRange,
    FeasibilityAssessment, IdeaAnalysis, IdeaDetail, IdeaList, IdeaListItem, IdeaRedirect,
    ImpactAssessment, OriginalText, RightOfReply, ScoreComponents, SearchMatch, SourceReference,
    TerritoryRef,
};
use super::submissions::{
    self, ContributionSubmission, ContributionSubmissionKind, IdeaSubmission, ReportSubmission,
    SubmissionResponse,
};
use super::tracking::{
    self, DeletionResponse, ReviewRequestBody, TrackedIdea, TrackedItem, TrackedReviewRequest,
    TrackingRequest,
};
use crate::antispam::altcha::AltchaChallenge;
use crate::db::portal::IdeaSort;
use crate::domain::rules::RuleReference;
use crate::domain::types::{
    AdminLevel, Confidence, ContributionKind, IdeaKind, ImpactDirection, LegalVehicle,
    ReportReason, ReviewOutcome, ReviewRequestStatus, TargetKind,
};
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
        submissions::submit_idea,
        submissions::submit_contribution,
        submissions::submit_report,
        tracking::track,
        tracking::delete,
        tracking::request_review,
        admin::session
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
        RightOfReply
    )),
    modifiers(&AdminSecurity),
    tags(
        (name = "system", description = "Santé du service et spécification"),
        (name = "antispam", description = "Protection anti-abus des formulaires (ALTCHA)"),
        (name = "portal", description = "Classement et pages des idées publiées"),
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

/// The specification served on the public port: admin routes and scheme removed.
pub fn public_spec() -> utoipa::openapi::OpenApi {
    let mut spec = full_spec();
    spec.paths
        .paths
        .retain(|path, _| !path.starts_with(ADMIN_PREFIX));
    if let Some(components) = spec.components.as_mut() {
        components.security_schemes.clear();
        components.schemas.remove("AdminSession");
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
            "/api/track",
            "/api/track/delete",
            "/api/track/review-request",
            "/api/admin/session",
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
    }
}
