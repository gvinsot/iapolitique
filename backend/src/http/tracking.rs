//! Tracking page of an author (AGENTS.md §7.1, §13.2): state of the text, deletion, human
//! review request.
//!
//! The token travels only in the JSON body of a `POST` (never in a URL, hence never in an
//! access log), is checked for format before hashing, and is looked up by its SHA-256: an
//! unknown, expired or malformed token gets the same `404 tracking_not_found`. Answers expose
//! what the author needs (status, days, reasons, link to the published idea), never an internal
//! id. Every request counts against the tracking rate limit and is never cached.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::client_ip::ClientIp;
use super::error::{ApiError, ErrorBody};
use super::extract::ApiJson;
use super::security::security_event;
use super::submissions::{masked, no_store};
use crate::antispam::rate_limit::RateAction;
use crate::db::tracked;
use crate::db::tracking::{self, TokenHash, TokenTarget};
use crate::domain::types::{ReviewOutcome, ReviewRequestStatus, TargetKind};
use crate::domain::validation::{REVIEW_MESSAGE_LIMITS, validate_optional_text};
use crate::moderation::Target;
use crate::moderation::statement::{self, StatementOfReasons};

/// Status reported for a text deleted by its author.
pub const DELETED: &str = "deleted";

/// Body of the tracking requests.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackingRequest {
    /// Tracking token (43 base64url characters), from the fragment of the tracking link.
    pub token: String,
}

/// Body of a human review request.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequestBody {
    pub token: String,
    /// Optional message for the administrator, 1 000 characters at most (personal data
    /// masked).
    #[serde(default)]
    pub message: Option<String>,
}

/// Published idea concerned by the tracked text.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrackedIdea {
    pub slug: String,
    pub title: String,
}

/// Human review requested by the author.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrackedReviewRequest {
    /// Day of the request (Europe/Paris).
    pub requested_on: NaiveDate,
    pub status: ReviewRequestStatus,
}

/// What the tracking page shows.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrackedItem {
    pub kind: TargetKind,
    /// Status of the text (`domain::status`: `submitted`, `second_opinion`, `accepted`,
    /// `published`, `rejected`…), or `deleted` once erased by its author.
    #[schema(example = "accepted")]
    pub status: String,
    /// Day of the submission (Europe/Paris).
    pub submitted_on: NaiveDate,
    /// Published idea: the idea itself, the idea a duplicate was merged into, or the idea
    /// concerned by a contribution or a report.
    #[schema(required = true)]
    pub idea: Option<TrackedIdea>,
    /// Statement of reasons of an automated decision against the text (refusal, masking).
    #[schema(required = true)]
    pub decision: Option<StatementOfReasons>,
    /// Outcome of the AI review of a report.
    #[schema(required = true)]
    pub report_outcome: Option<ReviewOutcome>,
    #[schema(required = true)]
    pub review_request: Option<TrackedReviewRequest>,
    /// The idea concerned by this contribution, report or merged duplicate was withdrawn by its
    /// author (§7.1): show « Idée retirée par son auteur » (`idea` is then `null`). Always sent;
    /// declared optional so that clients built before it keep compiling.
    #[schema(required = false)]
    pub idea_withdrawn: bool,
}

/// Confirmation of a deletion.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeletionResponse {
    /// Always `deleted`.
    #[schema(example = "deleted")]
    pub status: String,
}

fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "tracking_not_found",
        "Ce lien de suivi est inconnu ou a expiré. Vérifiez qu'il a été copié en entier.",
    )
}

/// Counts the request, then resolves the token (uniform 404 for anything unknown).
async fn resolve(
    state: &AppState,
    ip: ClientIp,
    token: &str,
) -> Result<(TokenHash, TokenTarget), ApiError> {
    state
        .rate_limiter
        .check(ip.ip(), RateAction::Tracking)
        .inspect_err(|_| security_event("rate_limited", RateAction::Tracking.as_str()))?;
    let hash = tracking::token_hash(token).ok_or_else(not_found)?;
    let target = tracking::find(&state.pool, &hash)
        .await?
        .ok_or_else(not_found)?;
    Ok((hash, target))
}

async fn tracked_item(state: &AppState, token: TokenTarget) -> Result<TrackedItem, ApiError> {
    if token.deleted {
        return Ok(TrackedItem {
            kind: token.kind,
            status: DELETED.to_owned(),
            submitted_on: token.created_on,
            idea: None,
            decision: None,
            report_outcome: None,
            review_request: None,
            idea_withdrawn: false,
        });
    }
    let target = Target::new(token.kind, token.id);
    // A token whose text vanished (deleted with the idea it was about) is unknown.
    let view = tracked::view(&state.pool, target)
        .await?
        .ok_or_else(not_found)?;
    let decision = statement::load(&state.pool, &state.prompts, target).await?;
    let review_request =
        tracked::review_request(&state.pool, target)
            .await?
            .map(|(requested_on, status)| TrackedReviewRequest {
                requested_on,
                status,
            });
    Ok(TrackedItem {
        kind: token.kind,
        report_outcome: view.report_outcome(),
        idea: view
            .idea_slug
            .zip(view.idea_title)
            .map(|(slug, title)| TrackedIdea { slug, title }),
        status: view.status,
        submitted_on: view.submitted_on,
        decision,
        review_request,
        idea_withdrawn: view.idea_withdrawn,
    })
}

/// State of the idea, contribution or report behind a tracking token.
#[utoipa::path(
    post,
    path = "/api/track",
    tag = "tracking",
    operation_id = "track",
    request_body = TrackingRequest,
    responses(
        (status = 200, description = "État du texte suivi.", body = TrackedItem),
        (status = 404, description = "Jeton inconnu, expiré ou mal formé (`tracking_not_found`).",
         body = ErrorBody),
        (status = 422, description = "Corps invalide.", body = ErrorBody),
        (status = 429, description = "Limite de requêtes atteinte (`rate_limited`).", body = ErrorBody)
    )
)]
pub async fn track(
    State(state): State<AppState>,
    ip: ClientIp,
    ApiJson(body): ApiJson<TrackingRequest>,
) -> Result<Response, ApiError> {
    let (_, token) = resolve(&state, ip, &body.token).await?;
    Ok(no_store(StatusCode::OK, tracked_item(&state, token).await?))
}

/// Deletes the text at its author's request (see `db::tracked`): a published or hidden idea is
/// withdrawn (gone from the public site, the API and the exports, kept for the admin, purged 12
/// months later), any other text is erased at once with what refers to it. Idempotent: a
/// second call confirms the deletion.
#[utoipa::path(
    post,
    path = "/api/track/delete",
    tag = "tracking",
    operation_id = "deleteTracked",
    request_body = TrackingRequest,
    responses(
        (status = 200, description = "Texte supprimé.", body = DeletionResponse),
        (status = 404, description = "Jeton inconnu, expiré ou mal formé (`tracking_not_found`).",
         body = ErrorBody),
        (status = 422, description = "Corps invalide.", body = ErrorBody),
        (status = 429, description = "Limite de requêtes atteinte (`rate_limited`).", body = ErrorBody)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    ip: ClientIp,
    ApiJson(body): ApiJson<TrackingRequest>,
) -> Result<Response, ApiError> {
    let (hash, token) = resolve(&state, ip, &body.token).await?;
    if !token.deleted {
        tracked::delete(&state.pool, Target::new(token.kind, token.id), &hash).await?;
    }
    Ok(no_store(
        StatusCode::OK,
        DeletionResponse {
            status: DELETED.to_owned(),
        },
    ))
}

/// Asks for a human review of an automated decision against the text (refusal or masking),
/// queued for the administrator. One request per text.
#[utoipa::path(
    post,
    path = "/api/track/review-request",
    tag = "tracking",
    operation_id = "requestHumanReview",
    request_body = ReviewRequestBody,
    responses(
        (status = 201, description = "Demande enregistrée.", body = TrackedReviewRequest),
        (status = 404, description = "Jeton inconnu, expiré ou mal formé (`tracking_not_found`).",
         body = ErrorBody),
        (status = 409, description = "Aucune décision automatique à réexaminer \
            (`review_not_available`) ou demande déjà faite (`review_already_requested`).",
         body = ErrorBody),
        (status = 422, description = "Message invalide.", body = ErrorBody),
        (status = 429, description = "Limite de requêtes atteinte (`rate_limited`).", body = ErrorBody)
    )
)]
pub async fn request_review(
    State(state): State<AppState>,
    ip: ClientIp,
    ApiJson(body): ApiJson<ReviewRequestBody>,
) -> Result<Response, ApiError> {
    let message = validate_optional_text(body.message.as_deref(), REVIEW_MESSAGE_LIMITS)?
        .map(|message| masked(&message, REVIEW_MESSAGE_LIMITS))
        .transpose()?;
    let (_, token) = resolve(&state, ip, &body.token).await?;
    let not_available = || {
        ApiError::new(
            StatusCode::CONFLICT,
            "review_not_available",
            "Un réexamen humain ne peut être demandé que pour un refus ou un masquage décidé \
             automatiquement.",
        )
    };
    if token.deleted {
        return Err(not_available());
    }
    let target = Target::new(token.kind, token.id);
    // Nothing left to review once the idea concerned was withdrawn by its author.
    if tracked::view(&state.pool, target)
        .await?
        .is_none_or(|view| view.idea_withdrawn)
    {
        return Err(not_available());
    }
    if statement::load(&state.pool, &state.prompts, target)
        .await?
        .is_none()
    {
        return Err(not_available());
    }
    let requested_on = tracked::insert_review_request(&state.pool, target, message.as_deref())
        .await?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::CONFLICT,
                "review_already_requested",
                "Un réexamen humain a déjà été demandé pour ce texte.",
            )
        })?;
    tracing::info!(
        target_kind = target.kind.as_str(),
        target_id = %target.id,
        "human review requested"
    );
    Ok(no_store(
        StatusCode::CREATED,
        TrackedReviewRequest {
            requested_on,
            status: ReviewRequestStatus::Pending,
        },
    ))
}
