//! Public submissions (AGENTS.md §5.1, §7.1, §7.2, §7.5, §13.2).
//!
//! Every form goes through the same steps, in this order:
//!
//! 1. validation of the body (consents, lengths), so that a mistake does not consume the
//!    visitor's anti-robot solution nor count against the limit;
//! 2. rate limit of the client fingerprint ([`RateLimiter`](crate::antispam::rate_limit));
//! 3. ALTCHA proof of work (single use);
//! 4. masking of personal data ([`redact`]): only the masked text is ever stored, screened,
//!    sent to Mistral or published; the unmasked text is not kept anywhere;
//! 5. storage with a tracking token (returned once, only its hash is stored) and the consent to
//!    the current rules;
//! 6. synchronous screening by the decision model, then, in the gray zone, the Mistral second
//!    opinion within 10 s when possible.
//!
//! Texts, tokens and IP addresses are never logged.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::client_ip::ClientIp;
use super::error::{ApiError, ErrorBody};
use super::extract::ApiJson;
use crate::antispam::rate_limit::RateAction;
use crate::db::submissions::{self, NewIdea};
use crate::db::tracking::TrackingToken;
use crate::domain::redaction::redact;
use crate::domain::types::{Actor, AdminLevel, TargetKind};
use crate::domain::validation::{
    FieldError, IDEA_TEXT_LIMITS, TERRITORY_MAX_CHARS, TextError, TextLimits, validate_idea_text,
    validate_line,
};
use crate::moderation::second_opinion::{self, FALLBACK_DELAY};
use crate::moderation::statement::{self, StatementOfReasons};
use crate::moderation::{self, Step, Target};

/// Message of a form sent without its two consents (§7.5), as in the frontend.
pub const CONSENTS_REQUIRED_MESSAGE: &str =
    "Cochez les deux cases pour envoyer votre texte : elles conditionnent sa publication.";

/// A new idea (§5.1).
#[derive(Deserialize, ToSchema)]
pub struct IdeaSubmission {
    /// Text of the idea, 50 to 4 000 characters. Personal data (e-mails, phone numbers…) is
    /// masked before storage.
    pub text: String,
    /// Administrative level targeted, if the author knows it.
    #[serde(default)]
    pub admin_level: Option<AdminLevel>,
    /// Territory concerned (free text, 120 characters at most).
    #[serde(default)]
    #[schema(example = "Métropole de Lyon")]
    pub territory: Option<String>,
    /// Acceptance of the licence clause of the rules (box never pre-checked).
    #[serde(default)]
    #[schema(required = true)]
    pub accept_licence: bool,
    /// « Je comprends que mon texte sera publié s'il est accepté » (box never pre-checked).
    #[serde(default)]
    #[schema(required = true)]
    pub accept_publication: bool,
    /// Solution of the ALTCHA widget (base64), accepted once.
    #[serde(default)]
    #[schema(required = true)]
    pub altcha: String,
}

/// Immediate answer to a submission.
#[derive(Serialize, ToSchema)]
pub struct SubmissionResponse {
    /// Status right after the synchronous screening: `accepted`, `rejected`,
    /// `second_opinion` (« en cours de vérification ») or `submitted` (decision model
    /// unavailable: the screening is retried, follow it with the tracking link).
    #[schema(example = "accepted")]
    pub status: String,
    /// Tracking token (256 bits, base64url), returned only this once: the tracking link is
    /// `/suivi#<token>`.
    pub tracking_token: String,
    /// Refusal reason (`reason_code` of `prompts/screening.yaml`).
    #[schema(required = true)]
    pub reason_code: Option<String>,
    /// Refusal explanation in French, with an invitation to rephrase.
    #[schema(required = true)]
    pub reason_message: Option<String>,
    /// Statement of reasons of a refusal (DSA art. 17).
    #[schema(required = true)]
    pub statement: Option<StatementOfReasons>,
}

pub(crate) fn require_consents(licence: bool, publication: bool) -> Result<(), ApiError> {
    if licence && publication {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "consent_required",
            CONSENTS_REQUIRED_MESSAGE,
        ))
    }
}

impl From<FieldError> for ApiError {
    fn from(err: FieldError) -> Self {
        Self::validation(err.code(), err.to_string())
    }
}

/// Masks the personal data of a validated text. A mask can be longer than what it replaces:
/// the masked text must still fit.
pub(crate) fn masked(text: &str, limits: TextLimits) -> Result<String, TextError> {
    let masked = redact(text).text;
    let actual = masked.chars().count();
    if actual > limits.max {
        Err(TextError::TooLong {
            max: limits.max,
            actual,
        })
    } else {
        Ok(masked)
    }
}

/// Anti-abuse checks, once the body is valid: rate limit, then proof of work.
pub(crate) fn check_abuse(
    state: &AppState,
    ip: ClientIp,
    action: RateAction,
    altcha: &str,
) -> Result<(), ApiError> {
    state.rate_limiter.check(ip.ip(), action)?;
    state.altcha.verify(altcha)?;
    Ok(())
}

/// Screens a stored text and builds the answer. Failures after the storage never lose the
/// token: the text stays `submitted` and its job retries the screening.
pub(crate) async fn screen_and_respond(
    state: &AppState,
    target: Target,
    token: TrackingToken,
) -> Result<(StatusCode, Json<SubmissionResponse>), ApiError> {
    let inline = state.mistral.is_some();
    let second_opinion_at = if inline {
        Utc::now() + FALLBACK_DELAY
    } else {
        Utc::now()
    };
    let mut step = moderation::screen(
        &state.pool,
        &state.screener,
        &state.prompts,
        target,
        None,
        Actor::Api,
        second_opinion_at,
    )
    .await
    .unwrap_or_else(|err| {
        tracing::error!(
            target_kind = target.kind.as_str(),
            target_id = %target.id,
            error = %err,
            "screening could not be settled; left to the job"
        );
        Step::Pending
    });
    if step == Step::SecondOpinion
        && let Some(settled) = second_opinion::run_inline(
            &state.pool,
            &state.prompts,
            state.mistral.as_ref(),
            state.config.mistral.daily_budget,
            target,
        )
        .await
    {
        step = settled;
    }

    let status = match step.status() {
        Some(status) => status.to_owned(),
        None => current_status(state, target).await?,
    };
    let statement = if status == "rejected" {
        statement::load(&state.pool, &state.prompts, target).await?
    } else {
        None
    };
    Ok((
        StatusCode::CREATED,
        Json(SubmissionResponse {
            status,
            tracking_token: token.into_inner(),
            reason_code: statement.as_ref().map(|s| s.reason_code.clone()),
            reason_message: statement.as_ref().map(|s| s.reason_message.clone()),
            statement,
        }),
    ))
}

async fn current_status(state: &AppState, target: Target) -> Result<String, ApiError> {
    let sql = match target.kind {
        TargetKind::Idea => "SELECT status FROM ideas WHERE id = $1",
        TargetKind::Contribution => "SELECT status FROM contributions WHERE id = $1",
        TargetKind::Report => "SELECT status FROM reports WHERE id = $1",
    };
    let row: Option<(String,)> = sqlx::query_as(sql)
        .bind(target.id)
        .fetch_optional(&state.pool)
        .await?;
    // Deleted by its author between the screening and the answer.
    Ok(row.map_or_else(|| "deleted".to_owned(), |(status,)| status))
}

/// Submits an idea: screened at once by the decision model, answered with its status and a
/// tracking token.
#[utoipa::path(
    post,
    path = "/api/ideas",
    tag = "submissions",
    operation_id = "submitIdea",
    request_body = IdeaSubmission,
    responses(
        (status = 201, description = "Idée enregistrée et filtrée.", body = SubmissionResponse),
        (status = 400, description = "Consentements manquants ou vérification anti-robot \
            invalide (`consent_required`, `altcha_*`).", body = ErrorBody),
        (status = 422, description = "Texte ou champ invalide.", body = ErrorBody),
        (status = 429, description = "Limite d'envois atteinte (`rate_limited`).", body = ErrorBody),
        (status = 503, description = "Service momentanément surchargé.", body = ErrorBody)
    )
)]
pub async fn submit_idea(
    State(state): State<AppState>,
    ip: ClientIp,
    ApiJson(body): ApiJson<IdeaSubmission>,
) -> Result<(StatusCode, Json<SubmissionResponse>), ApiError> {
    require_consents(body.accept_licence, body.accept_publication)?;
    let text = validate_idea_text(&body.text)?;
    let territory = validate_line(
        body.territory.as_deref(),
        "Le territoire",
        TERRITORY_MAX_CHARS,
    )?;
    check_abuse(&state, ip, RateAction::Idea, &body.altcha)?;

    let idea = NewIdea {
        text: masked(&text, IDEA_TEXT_LIMITS)?,
        admin_level: body.admin_level,
        territory: match territory {
            Some(territory) => validate_line(
                Some(&redact(&territory).text),
                "Le territoire",
                TERRITORY_MAX_CHARS,
            )?,
            None => None,
        },
    };
    let (id, token) = submissions::insert_idea(&state.pool, &idea).await?;
    tracing::info!(idea_id = %id, "idea submitted");
    screen_and_respond(&state, Target::new(TargetKind::Idea, id), token).await
}
