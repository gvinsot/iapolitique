//! Admin API (port 8081, LAN only, AGENTS.md §8): `Authorization: Bearer $ADMIN_API_TOKEN`
//! on every `/api/admin/…` route, compared in constant time.
//!
//! Every answer is `Cache-Control: no-store` and `X-Robots-Tag: noindex, nofollow` (§14.2).
//! Every mutation is recorded in `admin_actions` in the transaction that applies it.
//!
//! - [`actions`]: journal of the admin actions;
//! - [`dashboard`]: job queue, screening rates, AI costs, pending queues;
//! - [`jobs`]: job list and retry of dead jobs;
//! - [`ideas`]: ideas by status with search, hide / republish, re-analysis;
//! - [`moderation`]: reports, escalations of serious content, human review requests;
//! - [`screenings`]: screening decisions, labels and calibration export;
//! - [`replies`]: rights of reply published under ideas;
//! - [`settings`]: screening and duplicate thresholds.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Serialize;
use utoipa::ToSchema;

use super::AppState;
use super::error::{ApiError, ErrorBody};
use crate::config::Secret;

pub mod actions;
pub mod dashboard;
pub mod ideas;
pub mod jobs;
pub mod moderation;
pub mod replies;
pub mod screenings;
pub mod settings;

#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod calibration_tests;
#[cfg(test)]
mod moderation_tests;
#[cfg(test)]
mod replies_tests;

/// Routes of the admin API, relative to `/api/admin` (token checked by the caller).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/session", get(session))
        .route("/dashboard", get(dashboard::dashboard))
        .route("/jobs", get(jobs::list))
        .route("/jobs/{id}/retry", post(jobs::retry))
        .route("/ideas", get(ideas::list))
        .route("/ideas/{id}/hide", post(ideas::hide))
        .route("/ideas/{id}/republish", post(ideas::republish))
        .route("/ideas/{id}/reanalyze", post(ideas::reanalyze))
        .route("/reports", get(moderation::reports))
        .route("/escalations", get(moderation::escalations))
        .route(
            "/escalations/{id}/handle",
            post(moderation::handle_escalation),
        )
        .route("/review-requests", get(moderation::review_requests))
        .route(
            "/review-requests/{id}/resolve",
            post(moderation::resolve_review_request),
        )
        .route("/screenings", get(screenings::list))
        .route("/screenings/export.jsonl", get(screenings::export))
        .route(
            "/screenings/{id}/label",
            put(screenings::label).delete(screenings::unlabel),
        )
        .route("/settings", get(settings::get))
        .route(
            "/settings/screening-thresholds",
            put(settings::update_thresholds),
        )
        .route("/settings/dedup-threshold", put(settings::update_dedup))
        .route(
            "/ideas/{id}/replies",
            get(replies::list).post(replies::create),
        )
        .route(
            "/replies/{id}",
            put(replies::update).delete(replies::delete),
        )
        .route("/actions", get(actions::list))
}

/// `404` with a specific code.
pub(crate) fn not_found(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, code, message)
}

/// `409`: the action does not apply to the current state.
pub(crate) fn conflict(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, code, message)
}

/// A free text typed by the administrator (reason, note, right of reply…): normalized
/// (line breaks kept, spaces collapsed), `None` when blank; `422 invalid_field` when required
/// and blank, or longer than `max` characters.
pub(crate) fn admin_text(
    raw: Option<&str>,
    max: usize,
    required: bool,
    label: &'static str,
) -> Result<Option<String>, ApiError> {
    let text = raw
        .map(crate::domain::validation::normalize_text)
        .filter(|t| !t.is_empty());
    match text {
        None if required => Err(ApiError::validation(
            "invalid_field",
            format!("{label} est obligatoire."),
        )),
        Some(t) if t.chars().count() > max => Err(ApiError::validation(
            "invalid_field",
            format!("{label} doit contenir au plus {max} caractères."),
        )),
        text => Ok(text),
    }
}

/// An idea as referred to by the admin lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminIdeaRef {
    pub id: uuid::Uuid,
    #[schema(required = true)]
    pub slug: Option<String>,
    #[schema(required = true)]
    pub title: Option<String>,
    pub status: String,
}

/// Micro-euros as euros (estimates, for display).
pub(crate) fn euros(micro_eur: i64) -> f64 {
    micro_eur as f64 / 1_000_000.0
}

/// The expected admin token.
#[derive(Debug, Clone)]
pub struct AdminToken(Arc<Secret>);

impl AdminToken {
    pub fn new(secret: Secret) -> Self {
        Self(Arc::new(secret))
    }
}

/// Compares two byte strings in time independent of their content.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    std::hint::black_box(diff) == 0
}

/// Extracts the token of an `Authorization: Bearer <token>` header (scheme case-insensitive).
fn bearer_token(request: &Request) -> Option<&str> {
    let value = request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

/// Middleware rejecting requests without the right bearer token.
pub async fn require_admin(
    State(expected): State<AdminToken>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = bearer_token(&request)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected.0.expose().as_bytes()));
    if authorized {
        next.run(request).await
    } else {
        tracing::warn!("admin request rejected: missing or invalid token");
        ApiError::unauthorized().into_response()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminSession {
    pub authenticated: bool,
}

/// Checks the admin token (used by the admin pages to validate the token typed by the user).
#[utoipa::path(
    get,
    path = "/api/admin/session",
    tag = "admin",
    operation_id = "getAdminSession",
    security(("admin_token" = [])),
    responses(
        (status = 200, description = "Jeton valide.", body = AdminSession),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn session() -> Json<AdminSession> {
    Json(AdminSession {
        authenticated: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
        assert!(!constant_time_eq(b"secret", b""));
        assert!(!constant_time_eq(b"\0", b""));
    }

    #[test]
    fn parses_bearer_tokens() {
        let request = |value: &str| {
            axum::http::Request::get("/")
                .header(header::AUTHORIZATION, value)
                .body(axum::body::Body::empty())
                .unwrap()
        };
        assert_eq!(bearer_token(&request("Bearer abc")), Some("abc"));
        assert_eq!(bearer_token(&request("bearer  abc ")), Some("abc"));
        assert_eq!(bearer_token(&request("Basic abc")), None);
        assert_eq!(bearer_token(&request("Bearer ")), None);
        assert_eq!(bearer_token(&request("Bearer")), None);
    }
}
