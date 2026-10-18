//! Admin API (port 8081, LAN only, AGENTS.md §8): `Authorization: Bearer $ADMIN_API_TOKEN`
//! on every `/api/admin/…` route, compared in constant time.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use super::error::{ApiError, ErrorBody};
use crate::config::Secret;

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
