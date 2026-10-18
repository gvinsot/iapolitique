//! `Cache-Control` of the public answers (AGENTS.md §14.2): a short `max-age` on public GETs,
//! `no-store` on everything that carries a token or the state of a tracked text (see
//! `submissions::no_store`) and on the admin API.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Lists and pages that change with every publication.
pub const SHORT: HeaderValue = HeaderValue::from_static("public, max-age=60");
/// Reference data that changes with a deployment or once a night (methodology, taxonomy,
/// legal details, exports).
pub const MEDIUM: HeaderValue = HeaderValue::from_static("public, max-age=300");
/// Never stored by a browser or a proxy.
pub const NO_STORE: HeaderValue = HeaderValue::from_static("no-store");

/// `200` JSON answer with the given `Cache-Control`.
pub fn json(body: impl Serialize, cache_control: HeaderValue) -> Response {
    with_cache_control((StatusCode::OK, Json(body)).into_response(), cache_control)
}

pub fn with_cache_control(mut response: Response, cache_control: HeaderValue) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, cache_control);
    response
}
