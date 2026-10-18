//! Internal health endpoint of the worker (port 8082, never routed by Traefik).
//!
//! The worker loop calls [`Liveness::tick`] at every iteration; `/health` answers 503 when the
//! loop has been silent for too long (stuck worker), so that Docker restarts the container.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use serde_json::json;

/// Timestamp of the last loop iteration, shared with the health endpoint.
#[derive(Debug, Clone)]
pub struct Liveness {
    last_tick_ms: Arc<AtomicI64>,
    max_silence: Duration,
}

impl Liveness {
    pub fn new(max_silence: Duration) -> Self {
        let liveness = Self {
            last_tick_ms: Arc::new(AtomicI64::new(0)),
            max_silence,
        };
        liveness.tick();
        liveness
    }

    pub fn tick(&self) {
        self.last_tick_ms
            .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
    }

    pub fn is_alive(&self) -> bool {
        let silence =
            chrono::Utc::now().timestamp_millis() - self.last_tick_ms.load(Ordering::Relaxed);
        silence <= i64::try_from(self.max_silence.as_millis()).unwrap_or(i64::MAX)
    }
}

/// `GET /health` (and `/api/health` for symmetry with the api).
pub fn router(liveness: Liveness) -> Router {
    let handler = move || {
        let liveness = liveness.clone();
        async move {
            if liveness.is_alive() {
                (StatusCode::OK, Json(json!({ "status": "ok" }))).into_response()
            } else {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "status": "stalled" })),
                )
                    .into_response()
            }
        }
    };
    Router::new()
        .route("/health", get(handler.clone()))
        .route("/api/health", get(handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn liveness_expires() {
        let liveness = Liveness::new(Duration::from_secs(60));
        assert!(liveness.is_alive());
        liveness.last_tick_ms.store(
            chrono::Utc::now().timestamp_millis() - 61_000,
            Ordering::Relaxed,
        );
        assert!(!liveness.is_alive());
        liveness.tick();
        assert!(liveness.is_alive());
    }

    #[tokio::test]
    async fn endpoint_reflects_liveness() {
        let liveness = Liveness::new(Duration::from_secs(60));
        let app = router(liveness.clone());
        for path in ["/health", "/api/health"] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        liveness.last_tick_ms.store(0, Ordering::Relaxed);
        let response = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
