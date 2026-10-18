//! Runtime settings of the admin (AGENTS.md §8, §13.6): screening thresholds, criterion by
//! criterion, and the duplicate similarity threshold. Values are validated against
//! `prompts/screening.yaml` before being stored.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::ai::decision::{PartialThresholds, ThresholdOverrides};
use crate::ai::prompts::Prompts;
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::settings;
use crate::domain::types::TargetKind;
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::ApiJson;

/// Lowest duplicate threshold accepted from the admin: below, unrelated ideas would merge.
pub const DEDUP_MIN_THRESHOLD: f64 = 0.5;

/// Thresholds of one criterion: in force and from `screening.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminCriterionThresholds {
    pub key: String,
    pub reason_code: String,
    pub reject_below: f64,
    pub accept_above: f64,
    pub default_reject_below: f64,
    pub default_accept_above: f64,
    /// The thresholds in force differ from the file.
    pub adjusted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminThresholdSection {
    pub kind: TargetKind,
    pub criteria: Vec<AdminCriterionThresholds>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminDedupThreshold {
    /// Cosine similarity in force.
    pub value: f64,
    /// `DEDUP_SIMILARITY_THRESHOLD` of the environment.
    pub default: f64,
    pub adjusted: bool,
    /// Lowest value accepted.
    pub minimum: f64,
}

/// Settings in force.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminSettings {
    /// Version of `screening.yaml`.
    pub grid_version: String,
    pub screening: Vec<AdminThresholdSection>,
    pub dedup_similarity_threshold: AdminDedupThreshold,
}

async fn current(state: &AppState) -> Result<AdminSettings, ApiError> {
    let overrides = ThresholdOverrides::load(&state.pool).await?;
    let prompts = &state.prompts;
    let screening = TargetKind::ALL
        .iter()
        .map(|&kind| AdminThresholdSection {
            kind,
            criteria: prompts
                .screening
                .section(kind)
                .iter()
                .map(|c| {
                    let effective = overrides.effective(kind, c);
                    AdminCriterionThresholds {
                        key: c.key.clone(),
                        reason_code: c.reason_code.clone(),
                        reject_below: effective.reject_below,
                        accept_above: effective.accept_above,
                        default_reject_below: c.thresholds.reject_below,
                        default_accept_above: c.thresholds.accept_above,
                        adjusted: effective != c.thresholds,
                    }
                })
                .collect(),
        })
        .collect();
    let default = state.config.dedup_similarity_threshold;
    let value = settings::dedup_similarity_threshold(&state.pool, default).await?;
    let stored = settings::get(&state.pool, settings::DEDUP_SIMILARITY_THRESHOLD).await?;
    Ok(AdminSettings {
        grid_version: prompts.screening.version.clone(),
        screening,
        dedup_similarity_threshold: AdminDedupThreshold {
            value,
            default,
            // A valid setting is in force (an invalid one is ignored, §13.6).
            adjusted: stored
                .and_then(|v| v.as_f64())
                .is_some_and(|v| (0.0..=1.0).contains(&v)),
            minimum: DEDUP_MIN_THRESHOLD,
        },
    })
}

/// Screening thresholds and duplicate threshold in force.
#[utoipa::path(
    get,
    path = "/api/admin/settings",
    tag = "admin",
    operation_id = "getAdminSettings",
    security(("admin_token" = [])),
    responses(
        (status = 200, description = "Réglages en vigueur.", body = AdminSettings),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn get(State(state): State<AppState>) -> Result<Json<AdminSettings>, ApiError> {
    Ok(Json(current(&state).await?))
}

/// Thresholds of one criterion; an omitted one keeps the value of `screening.yaml`.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminThresholdOverride {
    #[serde(default)]
    pub reject_below: Option<f64>,
    #[serde(default)]
    pub accept_above: Option<f64>,
}

/// Every threshold override, by section and criterion key. Replaces the previous ones; an empty
/// body returns to the thresholds of `screening.yaml`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminThresholdsUpdate {
    #[serde(default)]
    pub idea: BTreeMap<String, AdminThresholdOverride>,
    #[serde(default)]
    pub contribution: BTreeMap<String, AdminThresholdOverride>,
    #[serde(default)]
    pub report: BTreeMap<String, AdminThresholdOverride>,
}

fn invalid(message: String) -> ApiError {
    ApiError::validation("invalid_thresholds", message)
}

/// A criterion key safe to quote in a message.
fn quoted(key: &str) -> String {
    if !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        format!("« {key} »")
    } else {
        "indiqué".to_owned()
    }
}

/// Validates the overrides against the grid and returns the setting to store (`None`: no
/// override left).
pub fn validated_overrides(
    prompts: &Prompts,
    update: &AdminThresholdsUpdate,
) -> Result<Option<ThresholdOverrides>, ApiError> {
    let mut overrides = ThresholdOverrides::default();
    let mut any = false;
    for (kind, section) in [
        (TargetKind::Idea, &update.idea),
        (TargetKind::Contribution, &update.contribution),
        (TargetKind::Report, &update.report),
    ] {
        let label = crate::moderation::section_label(kind);
        for (key, value) in section {
            let criterion = prompts
                .screening
                .section(kind)
                .iter()
                .find(|c| &c.key == key)
                .ok_or_else(|| {
                    invalid(format!(
                        "Critère {} inconnu dans la section {label}.",
                        quoted(key)
                    ))
                })?;
            for threshold in [value.reject_below, value.accept_above]
                .into_iter()
                .flatten()
            {
                if !(threshold.is_finite() && (0.0..=1.0).contains(&threshold)) {
                    return Err(invalid(format!(
                        "Critère {} ({label}) : les seuils sont des probabilités entre 0 et 1.",
                        quoted(key)
                    )));
                }
            }
            if value.reject_below.is_none() && value.accept_above.is_none() {
                continue;
            }
            let partial = PartialThresholds {
                reject_below: value.reject_below,
                accept_above: value.accept_above,
            };
            let merged_reject = partial
                .reject_below
                .unwrap_or(criterion.thresholds.reject_below);
            let merged_accept = partial
                .accept_above
                .unwrap_or(criterion.thresholds.accept_above);
            if merged_reject > merged_accept {
                return Err(invalid(format!(
                    "Critère {} ({label}) : le seuil de rejet doit être inférieur ou égal au \
                     seuil d'acceptation.",
                    quoted(key)
                )));
            }
            overrides.set(kind, key, partial);
            any = true;
        }
    }
    Ok(any.then_some(overrides))
}

/// Replaces the screening threshold overrides (setting `screening.thresholds`, §13.6). The new
/// thresholds apply to the next screenings; each screening records the thresholds it used.
#[utoipa::path(
    put,
    path = "/api/admin/settings/screening-thresholds",
    tag = "admin",
    operation_id = "updateAdminScreeningThresholds",
    security(("admin_token" = [])),
    request_body = AdminThresholdsUpdate,
    responses(
        (status = 200, description = "Réglages en vigueur.", body = AdminSettings),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 422, description = "Critère inconnu ou seuils incohérents \
                                      (`invalid_thresholds`), corps invalide.", body = ErrorBody)
    )
)]
pub async fn update_thresholds(
    State(state): State<AppState>,
    ApiJson(update): ApiJson<AdminThresholdsUpdate>,
) -> Result<Json<AdminSettings>, ApiError> {
    let overrides = validated_overrides(&state.prompts, &update)?;
    let mut tx = state.pool.begin().await?;
    let before = settings::get(&mut *tx, settings::SCREENING_THRESHOLDS).await?;
    let after = match &overrides {
        Some(overrides) => {
            let value = serde_json::to_value(overrides).map_err(|_| ApiError::internal())?;
            settings::set(&mut *tx, settings::SCREENING_THRESHOLDS, &value).await?;
            value
        }
        None => {
            settings::delete(&mut *tx, settings::SCREENING_THRESHOLDS).await?;
            Value::Null
        }
    };
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("settings.screening_thresholds")
            .target("setting", settings::SCREENING_THRESHOLDS)
            .details(json!({ "before": before, "after": after })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(current(&state).await?))
}

/// New duplicate threshold; `null` returns to `DEDUP_SIMILARITY_THRESHOLD`.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminDedupUpdate {
    /// Cosine similarity between 0.5 and 1, or `null`.
    #[schema(required = true)]
    pub value: Option<f64>,
}

/// Sets the cosine similarity from which an accepted idea is merged into the nearest
/// published one (setting `dedup.similarity_threshold`, §5.4, §13.6).
#[utoipa::path(
    put,
    path = "/api/admin/settings/dedup-threshold",
    tag = "admin",
    operation_id = "updateAdminDedupThreshold",
    security(("admin_token" = [])),
    request_body = AdminDedupUpdate,
    responses(
        (status = 200, description = "Réglages en vigueur.", body = AdminSettings),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 422, description = "Valeur hors de [0,5 ; 1] (`invalid_threshold`), corps \
                                      invalide.", body = ErrorBody)
    )
)]
pub async fn update_dedup(
    State(state): State<AppState>,
    ApiJson(update): ApiJson<AdminDedupUpdate>,
) -> Result<Json<AdminSettings>, ApiError> {
    if let Some(value) = update.value
        && !(value.is_finite() && (DEDUP_MIN_THRESHOLD..=1.0).contains(&value))
    {
        return Err(ApiError::validation(
            "invalid_threshold",
            "Le seuil de similarité est compris entre 0,5 et 1.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let before = settings::get(&mut *tx, settings::DEDUP_SIMILARITY_THRESHOLD).await?;
    match update.value {
        Some(value) => {
            settings::set(
                &mut *tx,
                settings::DEDUP_SIMILARITY_THRESHOLD,
                &json!(value),
            )
            .await?;
        }
        None => {
            settings::delete(&mut *tx, settings::DEDUP_SIMILARITY_THRESHOLD).await?;
        }
    }
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("settings.dedup_threshold")
            .target("setting", settings::DEDUP_SIMILARITY_THRESHOLD)
            .details(json!({ "before": before, "after": update.value })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(current(&state).await?))
}
