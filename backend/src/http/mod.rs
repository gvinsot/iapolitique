//! HTTP servers (AGENTS.md §13.2). Every route lives under `/api` (Traefik does not strip the
//! prefix).
//!
//! - [`public_router`]: port 8080, routed from Internet.
//! - [`admin_router`]: port 8081, LAN only, `/api/admin/…` protected by the admin token.

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, post};
use sqlx::PgPool;

use crate::ai::decision::Screener;
use crate::ai::mistral::MistralClient;
use crate::ai::prompts::Prompts;
use crate::antispam::altcha::Altcha;
use crate::antispam::rate_limit::RateLimiter;
use crate::config::{Config, Secret};

pub mod admin;
pub mod antispam;
pub mod client_ip;
pub mod error;
pub mod extract;
pub mod health;
pub mod middleware;
pub mod openapi;
pub mod submissions;

#[cfg(test)]
mod submission_tests;

use admin::AdminToken;
use client_ip::TrustedProxyHops;
use error::ApiError;

/// State shared by the handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    /// Versioned prompts, validated at startup.
    pub prompts: Arc<Prompts>,
    /// Synchronous screening by the decision model (§5.3).
    pub screener: Screener,
    /// ALTCHA proof of work of the public forms (§7.2).
    pub altcha: Arc<Altcha>,
    /// Per-client limits of the public forms (§7.2).
    pub rate_limiter: Arc<RateLimiter>,
    /// Inline second opinions (§5.2); `None` without `MISTRAL_API_KEY` (the worker's job then
    /// handles them).
    pub mistral: Option<MistralClient>,
}

impl FromRef<AppState> for TrustedProxyHops {
    fn from_ref(state: &AppState) -> Self {
        Self(state.config.http.trusted_proxy_hops)
    }
}

async fn not_found() -> ApiError {
    ApiError::not_found()
}

/// Routes shared by both ports.
fn system_routes() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(health::health))
        .route("/api/ready", get(health::ready))
}

/// Public API (port 8080).
pub fn public_router(state: AppState) -> Router {
    let router = system_routes()
        .route("/api/openapi.json", get(openapi::openapi_json))
        .route("/api/altcha/challenge", get(antispam::altcha_challenge))
        .route("/api/ideas", post(submissions::submit_idea))
        .fallback(not_found)
        .with_state(state.clone());
    middleware::with_common_layers(router, &state.config.http)
}

/// Admin API (port 8081): health probes and the OpenAPI spec are open (healthchecks), every
/// `/api/admin/…` route requires the bearer token.
pub fn admin_router(state: AppState, token: Secret) -> Router {
    let protected = Router::new()
        .route("/session", get(admin::session))
        .fallback(not_found)
        .layer(axum::middleware::from_fn_with_state(
            AdminToken::new(token),
            admin::require_admin,
        ));
    let router = system_routes()
        .route("/api/openapi.json", get(openapi::openapi_json_full))
        .nest(openapi::ADMIN_PREFIX, protected)
        .fallback(not_found)
        .with_state(state.clone());
    middleware::with_common_layers(router, &state.config.http)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Role;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn state(pool: PgPool) -> AppState {
        let config = Config::from_lookup(Role::Api, |key| match key {
            "DATABASE_CONNECTION_STRING" => Some("postgres://unused/db".to_owned()),
            "ADMIN_API_TOKEN" | "ALTCHA_HMAC_KEY" => Some(TOKEN.to_owned()),
            _ => None,
        })
        .unwrap();
        let prompts = crate::ai::prompts::test_prompts();
        let screener = Screener::new(
            crate::ai::decision::DecisionClient::new(&config.decision_service).unwrap(),
            Arc::clone(&prompts),
        );
        let rate_limiter = Arc::new(RateLimiter::new(
            crate::antispam::rate_limit::RateLimits::from(&config.antispam),
        ));
        AppState {
            pool,
            config: Arc::new(config),
            prompts,
            screener,
            altcha: Arc::new(Altcha::new(Secret::new(TOKEN))),
            rate_limiter,
            mistral: None,
        }
    }

    async fn call(app: &Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn get_req(uri: &str) -> Request<Body> {
        Request::get(uri).body(Body::empty()).unwrap()
    }

    fn admin_req(uri: &str, authorization: Option<&str>) -> Request<Body> {
        let mut builder = Request::get(uri);
        if let Some(value) = authorization {
            builder = builder.header(header::AUTHORIZATION, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn public_health_ready_and_fallback(pool: PgPool) {
        let app = public_router(state(pool.clone()));
        assert_eq!(
            call(&app, get_req("/api/health")).await,
            (StatusCode::OK, serde_json::json!({ "status": "ok" }))
        );
        assert_eq!(
            call(&app, get_req("/api/ready")).await,
            (StatusCode::OK, serde_json::json!({ "status": "ready" }))
        );
        let (status, body) = call(&app, get_req("/api/nothing")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
        // Admin routes do not exist on the public port.
        let (status, _) = call(
            &app,
            admin_req("/api/admin/session", Some(&format!("Bearer {TOKEN}"))),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        pool.close().await;
        let (status, body) = call(&app, get_req("/api/ready")).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"]["code"], "database_unavailable");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn altcha_challenges_are_served_uncached(pool: PgPool) {
        let state = state(pool);
        let app = public_router(state.clone());
        let response = app
            .clone()
            .oneshot(get_req("/api/altcha/challenge"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let challenge: crate::antispam::altcha::AltchaChallenge =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(challenge.algorithm, "SHA-256");
        assert_eq!(challenge.maxnumber, crate::antispam::altcha::MAX_NUMBER);
        // Solvable and verifiable once with the same key.
        let payload = crate::antispam::altcha::tests::solve(&challenge);
        state.altcha.verify(&payload).unwrap();
        assert!(state.altcha.verify(&payload).is_err());

        // Not served on the admin port.
        let admin = admin_router(state, Secret::new(TOKEN));
        let (status, _) = call(&admin, get_req("/api/altcha/challenge")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn public_spec_is_served_without_admin_routes(pool: PgPool) {
        let app = public_router(state(pool));
        let (status, spec) = call(&app, get_req("/api/openapi.json")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(spec["paths"]["/api/health"].is_object());
        assert!(spec["paths"]["/api/admin/session"].is_null());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn admin_routes_require_the_token(pool: PgPool) {
        let app = admin_router(state(pool), Secret::new(TOKEN));

        let (status, body) = call(&app, admin_req("/api/admin/session", None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "unauthorized");
        for wrong in ["Bearer nope", "Basic abc", &format!("Bearer {TOKEN}x")] {
            let (status, _) = call(&app, admin_req("/api/admin/session", Some(wrong))).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{wrong}");
        }

        let bearer = format!("Bearer {TOKEN}");
        assert_eq!(
            call(&app, admin_req("/api/admin/session", Some(&bearer))).await,
            (StatusCode::OK, serde_json::json!({ "authenticated": true }))
        );
        // Unknown admin routes: 401 without token, 404 with it.
        let (status, _) = call(&app, admin_req("/api/admin/unknown", None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = call(&app, admin_req("/api/admin/unknown", Some(&bearer))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Probes and spec stay open for healthchecks and the admin frontend.
        let (status, _) = call(&app, get_req("/api/health")).await;
        assert_eq!(status, StatusCode::OK);
        let (status, spec) = call(&app, get_req("/api/openapi.json")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(spec["paths"]["/api/admin/session"].is_object());
    }
}
