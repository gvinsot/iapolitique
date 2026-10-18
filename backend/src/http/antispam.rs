//! Anti-abuse routes of the public API (AGENTS.md §7.2).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};

use super::AppState;
use crate::antispam::altcha::AltchaChallenge;

/// A new ALTCHA proof-of-work challenge, to solve before submitting a form.
#[utoipa::path(
    get,
    path = "/api/altcha/challenge",
    tag = "antispam",
    operation_id = "getAltchaChallenge",
    responses(
        (status = 200, description = "Défi à résoudre par le widget ALTCHA (valable 20 minutes).",
         body = AltchaChallenge)
    )
)]
pub async fn altcha_challenge(State(state): State<AppState>) -> Response {
    let mut response = Json(state.altcha.create_challenge()).into_response();
    // A challenge is single-use: never cached by a browser or a proxy.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
