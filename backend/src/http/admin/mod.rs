//! Admin API (port 8081, LAN only, AGENTS.md §8): `Authorization: Bearer $ADMIN_API_TOKEN`
//! on every `/api/admin/…` route, compared in constant time. After [`LOCKOUT_MAX_FAILURES`] wrong
//! tokens within [`LOCKOUT_WINDOW`], every admin request is refused for [`LOCKOUT_DURATION`]
//! (§14.4).
//!
//! Every answer is `Cache-Control: no-store` and `X-Robots-Tag: noindex, nofollow` (§14.2).
//! Every mutation is recorded in `admin_actions` in the transaction that applies it.
//!
//! - [`actions`]: journal of the admin actions;
//! - [`alerts`]: check of the Mistral API key on demand;
//! - [`dashboard`]: job queue, screening rates, AI costs, pending queues;
//! - [`jobs`]: job list and retry of dead jobs;
//! - [`ideas`]: ideas by status with search, hide / republish, re-analysis;
//! - [`moderation`]: reports, escalations of serious content, human review requests;
//! - [`screenings`]: screening decisions, labels and calibration export;
//! - [`replies`]: rights of reply published under ideas;
//! - [`settings`]: screening and duplicate thresholds.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
use super::security::security_event;
use crate::config::Secret;

pub mod actions;
pub mod alerts;
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
#[cfg(test)]
mod seal_tests;

/// Routes of the admin API, relative to `/api/admin` (token checked by the caller).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/session", get(session))
        .route("/dashboard", get(dashboard::dashboard))
        .route(
            "/provider-alerts/mistral/check",
            post(alerts::check_mistral_key),
        )
        .route("/jobs", get(jobs::list))
        .route("/jobs/{id}/retry", post(jobs::retry))
        .route("/jobs/{id}/logs", get(jobs::logs))
        .route("/ideas", get(ideas::list))
        .route("/ideas/{id}/hide", post(ideas::hide))
        .route("/ideas/{id}/reject", post(ideas::reject))
        .route("/ideas/{id}/accept", post(ideas::accept))
        .route("/ideas/{id}/republish", post(ideas::republish))
        .route("/ideas/{id}/reanalyze", post(ideas::reanalyze))
        .route("/ideas/reanalyze-failed", post(ideas::reanalyze_failed))
        .route("/reports", get(moderation::reports))
        .route("/escalations", get(moderation::escalations))
        .route(
            "/escalations/{id}/handle",
            post(moderation::handle_escalation),
        )
        .route("/escalations/{id}/seal", get(moderation::escalation_seal))
        .route("/escalation-seals", get(moderation::escalation_seals))
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

/// Wrong tokens tolerated within [`LOCKOUT_WINDOW`] before the lockout.
pub const LOCKOUT_MAX_FAILURES: usize = 10;
/// Sliding window over which wrong tokens are counted.
pub const LOCKOUT_WINDOW: Duration = Duration::from_secs(5 * 60);
/// Duration of the lockout: every admin request is refused with `429`, right token included.
pub const LOCKOUT_DURATION: Duration = Duration::from_secs(5 * 60);

/// Counter of admin authentication failures (§14.4), in memory, for the whole admin port: no
/// IP address is stored, only the instants of the recent failures.
#[derive(Debug)]
pub struct AuthLockout {
    max_failures: usize,
    window: Duration,
    duration: Duration,
    state: Mutex<LockoutState>,
}

#[derive(Debug, Default)]
struct LockoutState {
    failures: VecDeque<Instant>,
    locked_until: Option<Instant>,
}

impl Default for AuthLockout {
    fn default() -> Self {
        Self::new(LOCKOUT_MAX_FAILURES, LOCKOUT_WINDOW, LOCKOUT_DURATION)
    }
}

impl AuthLockout {
    pub fn new(max_failures: usize, window: Duration, duration: Duration) -> Self {
        Self {
            max_failures,
            window,
            duration,
            state: Mutex::new(LockoutState::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, LockoutState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Time left before the end of the lockout at `now`, `None` when not locked.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let mut state = self.state();
        match state.locked_until {
            Some(until) if until > now => Some(until - now),
            Some(_) => {
                state.locked_until = None;
                None
            }
            None => None,
        }
    }

    /// Records a failure at `now`; `true` when it starts a lockout.
    pub fn record_failure(&self, now: Instant) -> bool {
        let mut state = self.state();
        while state
            .failures
            .front()
            .is_some_and(|&at| now.saturating_duration_since(at) >= self.window)
        {
            state.failures.pop_front();
        }
        state.failures.push_back(now);
        if state.failures.len() >= self.max_failures {
            state.failures.clear();
            state.locked_until = Some(now + self.duration);
            true
        } else {
            false
        }
    }
}

/// The expected admin token and the failure counter of the admin port.
#[derive(Debug, Clone)]
pub struct AdminToken {
    secret: Arc<Secret>,
    lockout: Arc<AuthLockout>,
}

impl AdminToken {
    pub fn new(secret: Secret) -> Self {
        Self::with_lockout(secret, AuthLockout::default())
    }

    pub fn with_lockout(secret: Secret, lockout: AuthLockout) -> Self {
        Self {
            secret: Arc::new(secret),
            lockout: Arc::new(lockout),
        }
    }
}

fn locked(remaining: Duration) -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "admin_locked",
        "Trop d'échecs d'authentification : l'administration est verrouillée quelques minutes.",
    )
    .with_retry_after(remaining.as_secs().max(1))
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

/// Middleware rejecting requests without the right bearer token (`401`), and every request
/// while the port is locked after [`LOCKOUT_MAX_FAILURES`] wrong tokens (`429 admin_locked`).
/// A missing token is refused without counting (the admin pages probe the session before the
/// token is typed); a wrong one counts. Failures are security events (§14.6), never with the
/// token.
pub async fn require_admin(
    State(expected): State<AdminToken>,
    request: Request,
    next: Next,
) -> Response {
    let now = Instant::now();
    if let Some(remaining) = expected.lockout.remaining(now) {
        security_event("admin_auth_refused", "locked");
        return locked(remaining).into_response();
    }
    let Some(token) = bearer_token(&request) else {
        security_event("admin_auth_failure", "missing_token");
        return ApiError::unauthorized().into_response();
    };
    if constant_time_eq(token.as_bytes(), expected.secret.expose().as_bytes()) {
        return next.run(request).await;
    }
    security_event("admin_auth_failure", "invalid_token");
    if expected.lockout.record_failure(now) {
        security_event("admin_auth_lockout", "too_many_failures");
    }
    ApiError::unauthorized().into_response()
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
    fn lockout_counts_failures_in_a_sliding_window() {
        let lockout = AuthLockout::new(3, Duration::from_secs(60), Duration::from_secs(30));
        let t0 = Instant::now();
        assert_eq!(lockout.remaining(t0), None);
        assert!(!lockout.record_failure(t0));
        assert!(!lockout.record_failure(t0 + Duration::from_secs(10)));
        // The first failure left the window: still two failures.
        assert!(!lockout.record_failure(t0 + Duration::from_secs(61)));
        assert_eq!(lockout.remaining(t0 + Duration::from_secs(61)), None);
        let t1 = t0 + Duration::from_secs(62);
        assert!(lockout.record_failure(t1));
        assert_eq!(lockout.remaining(t1), Some(Duration::from_secs(30)));
        assert_eq!(
            lockout.remaining(t1 + Duration::from_secs(29)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(lockout.remaining(t1 + Duration::from_secs(30)), None);
        // Counting starts again from zero after the lockout.
        let t2 = t1 + Duration::from_secs(31);
        assert!(!lockout.record_failure(t2));
        assert!(!lockout.record_failure(t2));
        assert!(lockout.record_failure(t2));
        let error = locked(Duration::from_millis(300));
        assert_eq!(error.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error.code, "admin_locked");
        assert_eq!(error.retry_after, Some(1));
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
