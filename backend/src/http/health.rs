//! Liveness and readiness probes (AGENTS.md §4.1), served on both api ports.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use super::AppState;
use super::error::{ApiError, ErrorBody};
use crate::db;

/// Maximum time given to the database to answer the readiness probe.
const READY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(example = "ok")]
    pub status: String,
}

/// Liveness: the process answers.
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    operation_id = "getHealth",
    responses((status = 200, description = "Le service répond.", body = HealthResponse))
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_owned(),
    })
}

/// Readiness: the database is reachable.
#[utoipa::path(
    get,
    path = "/api/ready",
    tag = "system",
    operation_id = "getReady",
    responses(
        (status = 200, description = "Le service est prêt.", body = HealthResponse),
        (status = 503, description = "Base de données injoignable.", body = ErrorBody)
    )
)]
pub async fn ready(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    match tokio::time::timeout(READY_TIMEOUT, db::ping(&state.pool)).await {
        Ok(Ok(())) => Ok(Json(HealthResponse {
            status: "ready".to_owned(),
        })),
        Ok(Err(err)) => {
            tracing::warn!(error = %err, "readiness probe: database error");
            Err(unavailable())
        }
        Err(_) => {
            tracing::warn!("readiness probe: database timeout");
            Err(unavailable())
        }
    }
}

fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "database_unavailable",
        "Service momentanément indisponible, veuillez réessayer plus tard.",
    )
}
