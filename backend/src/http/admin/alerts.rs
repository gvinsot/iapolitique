//! `POST /api/admin/provider-alerts/mistral/check` (AGENTS.md §6.4, §8): « Vérifier la clé
//! Mistral maintenant ». One call that bills nothing (`GET /v1/models`, [`MistralClient::check_key`])
//! tells at once whether Mistral accepts the API key, instead of waiting for the next job allowed
//! to check it (15 min to 1 h after the last refusal):
//!
//! - accepted: the `mistral_auth` alert is closed and the Mistral jobs that wait for the key
//!   (queued within the hour) are due now;
//! - refused (HTTP 401/403): the alert is opened or extended, like a refused job call;
//! - anything else (timeout, 5xx…): no conclusion, nothing changes.
//!
//! Not recorded in `ai_calls` (no tokens, no cost, no purpose of the pipeline); every check is
//! journaled in `admin_actions` (`provider_alert.mistral_check`), in the transaction that
//! applies its effects.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use super::conflict;
use crate::ai::mistral::{KeyCheck, MistralClient};
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::provider_alerts::{self, AUTH_RETRY_MAX, MISTRAL_AUTH};
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::jobs::{JobKind, queue};

/// Result of a key check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MistralKeyCheckResult {
    /// Mistral accepts the key.
    Ok,
    /// Mistral refuses the key (HTTP 401 or 403).
    Refused,
    /// No conclusion (timeout, Mistral unreachable or another HTTP status): try again later.
    Unavailable,
}

impl MistralKeyCheckResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Refused => "refused",
            Self::Unavailable => "unavailable",
        }
    }
}

/// What a key check found and did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminMistralKeyCheck {
    pub result: MistralKeyCheckResult,
    /// HTTP status answered by Mistral (`null`: no answer).
    #[schema(required = true)]
    pub http_status: Option<i32>,
    /// The `mistral_auth` alert was in progress and is now closed.
    pub alert_cleared: bool,
    /// Mistral jobs that waited for the key and are now due.
    pub jobs_due: i64,
    /// French message to display as is.
    pub message: String,
}

/// Kinds of the jobs that call Mistral (AGENTS.md §13.1).
fn mistral_kinds() -> Vec<JobKind> {
    JobKind::ALL
        .iter()
        .copied()
        .filter(|kind| kind.requires_mistral())
        .collect()
}

/// Checks the Mistral API key now (see the module documentation).
#[utoipa::path(
    post,
    path = "/api/admin/provider-alerts/mistral/check",
    tag = "admin",
    operation_id = "checkAdminMistralKey",
    security(("admin_token" = [])),
    responses(
        (status = 200, description = "Résultat de la vérification.", body = AdminMistralKeyCheck),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 409, description = "Aucune clé configurée pour l'api (`mistral_key_missing`).", body = ErrorBody)
    )
)]
pub async fn check_mistral_key(
    State(state): State<AppState>,
) -> Result<Json<AdminMistralKeyCheck>, ApiError> {
    let client: &MistralClient = state.mistral.as_ref().ok_or_else(|| {
        conflict(
            "mistral_key_missing",
            "MISTRAL_API_KEY n'est pas défini pour l'api : aucune clé à vérifier.",
        )
    })?;
    let check = client.check_key().await;
    let mut tx = state.pool.begin().await?;
    let (result, http_status, alert_cleared, jobs_due) = match check {
        KeyCheck::Accepted => {
            let cleared = provider_alerts::clear(&mut *tx, MISTRAL_AUTH).await?;
            let due = queue::bring_forward_kinds(
                &mut *tx,
                &mistral_kinds(),
                AUTH_RETRY_MAX.to_std().unwrap_or_default(),
            )
            .await?;
            (
                MistralKeyCheckResult::Ok,
                Some(200),
                cleared,
                i64::try_from(due).unwrap_or(i64::MAX),
            )
        }
        KeyCheck::Refused(status) => {
            if provider_alerts::record_failure(&mut *tx, MISTRAL_AUTH, status).await? {
                // Same event, same period as a refused job call: one ERROR per hour.
                tracing::error!(
                    http_status = status,
                    "Mistral refused the API key: check MISTRAL_API_KEY (AI jobs wait)"
                );
            }
            (MistralKeyCheckResult::Refused, Some(status), false, 0)
        }
        KeyCheck::Inconclusive(status) => (MistralKeyCheckResult::Unavailable, status, false, 0),
    };
    let http_status = http_status.map(i32::from);
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("provider_alert.mistral_check")
            .target("provider_alert", MISTRAL_AUTH)
            .details(json!({
                "result": result.as_str(),
                "http_status": http_status,
                "alert_cleared": alert_cleared,
                "jobs_due": jobs_due,
            })),
    )
    .await?;
    tx.commit().await?;
    if alert_cleared {
        tracing::info!("Mistral accepts the API key again (admin check): alert closed");
    }
    Ok(Json(AdminMistralKeyCheck {
        message: message(result, http_status, alert_cleared, jobs_due),
        result,
        http_status,
        alert_cleared,
        jobs_due,
    }))
}

fn message(
    result: MistralKeyCheckResult,
    http_status: Option<i32>,
    alert_cleared: bool,
    jobs_due: i64,
) -> String {
    match result {
        MistralKeyCheckResult::Ok => {
            let jobs = match jobs_due {
                0 => "aucun job IA n'attendait la clé".to_owned(),
                1 => "1 job IA en attente repart maintenant".to_owned(),
                n => format!("{n} jobs IA en attente repartent maintenant"),
            };
            let alert = if alert_cleared {
                "alerte close"
            } else {
                "aucune alerte en cours"
            };
            format!("Clé Mistral acceptée : {alert}, {jobs}.")
        }
        MistralKeyCheckResult::Refused => format!(
            "Clé Mistral toujours refusée (HTTP {}) : vérifier MISTRAL_API_KEY puis redéployer \
             (docs/ops.md, rotation de la clé Mistral).",
            http_status.unwrap_or_default()
        ),
        MistralKeyCheckResult::Unavailable => match http_status {
            Some(status) => format!(
                "Vérification impossible : Mistral a répondu HTTP {status}. Réessayer dans \
                 quelques minutes."
            ),
            None => "Vérification impossible : Mistral ne répond pas. Réessayer dans quelques \
                     minutes."
                .to_owned(),
        },
    }
}
