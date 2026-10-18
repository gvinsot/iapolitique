//! Calibration of the screening (AGENTS.md §5.3, §8): decisions of the decision model with the
//! probability of passing each criterion and the thresholds applied, labelling by the
//! administrator, and JSONL export for `docs/calibration/bench_screening.py`.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::{admin_text, not_found};
use crate::ai::decision::questions_hash;
use crate::ai::prompts::{Prompts, Taxonomy};
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::screenings::{self, CalibrationRow, ScreeningFilter, ScreeningRow};
use crate::domain::text_enum;
use crate::domain::types::{
    CriterionVerdict, Decision, LabelVerdict, ScreeningOutcome, TargetKind,
};
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::pagination::{PageRequest, page_type};

/// Longest note of a label.
pub const LABEL_NOTE_MAX_CHARS: usize = 1_000;
/// Lines of an export by default, and at most.
pub const EXPORT_DEFAULT_LIMIT: u32 = 5_000;
pub const EXPORT_MAX_LIMIT: u32 = 20_000;

fn screening_not_found() -> ApiError {
    not_found(
        "screening_not_found",
        "Cette décision de filtrage n'existe pas.",
    )
}

fn known<T: std::str::FromStr>(value: &str) -> Result<T, ApiError> {
    value.parse().map_err(|_| {
        tracing::error!("unknown value in the database");
        ApiError::internal()
    })
}

/// One criterion of a screening: probability of passing it, thresholds applied, verdict.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminCriterionScore {
    pub key: String,
    /// Probability of passing the criterion (0 to 1), `null` when not computed.
    #[schema(required = true)]
    pub probability: Option<f64>,
    #[schema(required = true)]
    pub reject_below: Option<f64>,
    #[schema(required = true)]
    pub accept_above: Option<f64>,
    #[schema(required = true)]
    pub verdict: Option<CriterionVerdict>,
}

/// The Mistral second opinion of a gray-zone screening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminSecondOpinion {
    #[schema(required = true)]
    pub decision: Option<Decision>,
    #[schema(required = true)]
    pub reason_code: Option<String>,
    #[schema(required = true)]
    pub explanation: Option<String>,
    #[schema(required = true)]
    pub model: Option<String>,
    #[schema(required = true)]
    pub prompt_version: Option<String>,
    #[schema(required = true)]
    pub at: Option<DateTime<Utc>>,
}

/// The administrator's label of a screening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminScreeningLabel {
    /// Whether the decision reached was right.
    pub label: LabelVerdict,
    /// The decision the screening should have reached.
    pub expected: Decision,
    #[schema(required = true)]
    pub note: Option<String>,
    pub labeled_at: DateTime<Utc>,
}

/// A screening by the decision model (scores kept without text after the purge).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminScreening {
    pub id: Uuid,
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    pub created_at: DateTime<Utc>,
    /// Version of `screening.yaml`.
    pub grid_version: String,
    /// `null` when the decision service failed (see `error_code`).
    #[schema(required = true)]
    pub outcome: Option<ScreeningOutcome>,
    #[schema(required = true)]
    pub reason_code: Option<String>,
    #[schema(required = true)]
    pub error_code: Option<String>,
    #[schema(required = true)]
    pub latency_ms: Option<i32>,
    /// Criteria in the order of the grid.
    pub criteria: Vec<AdminCriterionScore>,
    #[schema(required = true)]
    pub second_opinion: Option<AdminSecondOpinion>,
    /// Decision finally reached (decision model or second opinion).
    #[schema(required = true)]
    pub final_decision: Option<Decision>,
    #[schema(required = true)]
    pub decided_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub label: Option<AdminScreeningLabel>,
    /// Beginning of the masked text, `null` once erased.
    #[schema(required = true)]
    pub excerpt: Option<String>,
}

/// Decision reached by a screening: the final decision, or the clear-cut outcome.
fn reached(
    final_decision: Option<Decision>,
    outcome: Option<ScreeningOutcome>,
) -> Option<Decision> {
    final_decision.or(match outcome {
        Some(ScreeningOutcome::Accepted) => Some(Decision::Accept),
        Some(ScreeningOutcome::Rejected) => Some(Decision::Reject),
        _ => None,
    })
}

fn criteria(prompts: &Prompts, kind: TargetKind, row: &ScreeningRow) -> Vec<AdminCriterionScore> {
    let mut keys: Vec<String> = prompts
        .screening
        .section(kind)
        .iter()
        .map(|c| c.key.clone())
        .collect();
    let mut others: Vec<String> = [
        &row.pass_probabilities,
        &row.criterion_verdicts,
        &row.thresholds,
    ]
    .iter()
    .filter_map(|v| v.as_object())
    .flat_map(|m| m.keys().cloned())
    .filter(|k| !keys.contains(k))
    .collect();
    others.sort();
    others.dedup();
    keys.extend(others);
    keys.into_iter()
        .filter(|key| {
            row.pass_probabilities.get(key).is_some() || row.criterion_verdicts.get(key).is_some()
        })
        .map(|key| {
            let thresholds = row.thresholds.get(&key);
            AdminCriterionScore {
                probability: row.pass_probabilities.get(&key).and_then(Value::as_f64),
                reject_below: thresholds
                    .and_then(|t| t.get("reject_below"))
                    .and_then(Value::as_f64),
                accept_above: thresholds
                    .and_then(|t| t.get("accept_above"))
                    .and_then(Value::as_f64),
                verdict: row
                    .criterion_verdicts
                    .get(&key)
                    .and_then(Value::as_str)
                    .and_then(|v| v.parse().ok()),
                key,
            }
        })
        .collect()
}

fn screening_view(prompts: &Prompts, row: ScreeningRow) -> Result<AdminScreening, ApiError> {
    let kind: TargetKind = known(&row.target_kind)?;
    let outcome: Option<ScreeningOutcome> = row.outcome.as_deref().map(known).transpose()?;
    let final_decision: Option<Decision> = row.final_decision.as_deref().map(known).transpose()?;
    let label = match (row.human_label.as_deref(), row.labeled_at) {
        (Some(expected), Some(labeled_at)) => {
            let expected: Decision = known(expected)?;
            Some(AdminScreeningLabel {
                label: if reached(final_decision, outcome) == Some(expected) {
                    LabelVerdict::Correct
                } else {
                    LabelVerdict::Incorrect
                },
                expected,
                note: row.human_label_note.clone(),
                labeled_at,
            })
        }
        _ => None,
    };
    let second_opinion = if row.second_opinion_decision.is_some() || row.second_opinion_at.is_some()
    {
        Some(AdminSecondOpinion {
            decision: row
                .second_opinion_decision
                .as_deref()
                .map(known)
                .transpose()?,
            reason_code: row.second_opinion_reason_code.clone(),
            explanation: row.second_opinion_explanation.clone(),
            model: row.second_opinion_model.clone(),
            prompt_version: row.second_opinion_prompt_version.clone(),
            at: row.second_opinion_at,
        })
    } else {
        None
    };
    Ok(AdminScreening {
        criteria: criteria(prompts, kind, &row),
        id: row.id,
        target_kind: kind,
        target_id: row.target_id,
        created_at: row.created_at,
        grid_version: row.prompt_version,
        outcome,
        reason_code: row.reason_code,
        error_code: row.error_code,
        latency_ms: row.latency_ms,
        second_opinion,
        final_decision,
        decided_at: row.decided_at,
        label,
        excerpt: row.excerpt,
    })
}

page_type!(
    /// A page of screenings, most recent first.
    AdminScreeningPage,
    AdminScreening
);

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminScreeningQuery {
    #[serde(default)]
    pub kind: Option<TargetKind>,
    #[serde(default)]
    pub outcome: Option<ScreeningOutcome>,
    /// `true`: labelled only; `false`: decided and not labelled yet.
    #[serde(default)]
    pub labeled: Option<bool>,
    /// Refusal reason (`reason_code` of the grid).
    #[serde(default)]
    pub reason_code: Option<String>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

/// Screenings with their raw scores, for the calibration of the thresholds.
#[utoipa::path(
    get,
    path = "/api/admin/screenings",
    tag = "admin",
    operation_id = "listAdminScreenings",
    security(("admin_token" = [])),
    params(AdminScreeningQuery),
    responses(
        (status = 200, description = "Une page de décisions de filtrage.", body = AdminScreeningPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminScreeningQuery>,
) -> Result<Json<AdminScreeningPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let reason_code = match query.reason_code {
        Some(code)
            if state
                .prompts
                .screening
                .reason_codes()
                .contains(&code.as_str()) =>
        {
            Some(code)
        }
        Some(_) => {
            return Err(ApiError::bad_request(
                "invalid_query",
                "Motif de refus inconnu.",
            ));
        }
        None => None,
    };
    let filter = ScreeningFilter {
        kind: query.kind,
        outcome: query.outcome,
        labeled: query.labeled,
        reason_code,
    };
    let (rows, total) =
        screenings::list(&state.pool, &filter, pages.limit(), pages.offset()).await?;
    let items = rows
        .into_iter()
        .map(|row| screening_view(&state.prompts, row))
        .collect::<Result<_, _>>()?;
    Ok(Json(AdminScreeningPage::new(items, pages, total)))
}

/// Judgement of a screening decision.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminScreeningLabelRequest {
    pub label: LabelVerdict,
    /// Decision expected. Optional when it follows from `label` and the decision reached;
    /// required when no decision was reached (service error, second opinion pending).
    #[serde(default)]
    pub expected: Option<Decision>,
    /// 1 000 characters.
    #[serde(default)]
    pub note: Option<String>,
}

fn inconsistent(message: &'static str) -> ApiError {
    ApiError::validation("label_inconsistent", message)
}

/// The decision expected from a label and the decision reached.
pub fn expected_decision(
    label: LabelVerdict,
    expected: Option<Decision>,
    reached: Option<Decision>,
) -> Result<Decision, ApiError> {
    let opposite = |d: Decision| match d {
        Decision::Accept => Decision::Reject,
        Decision::Reject => Decision::Accept,
    };
    match (label, expected, reached) {
        (_, None, None) => Err(inconsistent(
            "Aucune décision n'a été prise : indiquez la décision attendue.",
        )),
        (LabelVerdict::Correct, _, None) => Err(inconsistent(
            "Aucune décision n'a été prise : elle ne peut pas être jugée correcte.",
        )),
        (LabelVerdict::Incorrect, Some(expected), None) => Ok(expected),
        (LabelVerdict::Correct, Some(expected), Some(reached)) if expected != reached => Err(
            inconsistent("Une décision correcte est la décision attendue."),
        ),
        (LabelVerdict::Incorrect, Some(expected), Some(reached)) if expected == reached => Err(
            inconsistent("Une décision incorrecte diffère de la décision attendue."),
        ),
        (LabelVerdict::Correct, _, Some(reached)) => Ok(reached),
        (LabelVerdict::Incorrect, _, Some(reached)) => Ok(opposite(reached)),
    }
}

/// Labels a screening: whether its decision was right and which one was expected (§8).
#[utoipa::path(
    put,
    path = "/api/admin/screenings/{id}/label",
    tag = "admin",
    operation_id = "labelAdminScreening",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de la décision de filtrage")),
    request_body = AdminScreeningLabelRequest,
    responses(
        (status = 200, description = "Décision étiquetée.", body = AdminScreening),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Décision inconnue (`screening_not_found`).", body = ErrorBody),
        (status = 422, description = "Étiquette incohérente (`label_inconsistent`) ou corps invalide.",
         body = ErrorBody)
    )
)]
pub async fn label(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AdminScreeningLabelRequest>,
) -> Result<Json<AdminScreening>, ApiError> {
    let note = admin_text(body.note.as_deref(), LABEL_NOTE_MAX_CHARS, false, "La note")?;
    let mut tx = state.pool.begin().await?;
    let reached = screenings::lock_decision(&mut tx, id)
        .await?
        .ok_or_else(screening_not_found)?;
    let expected = expected_decision(body.label, body.expected, reached)?;
    screenings::set_label(&mut tx, id, Some((expected, note.as_deref()))).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("screening.label")
            .target("screening", id)
            .details(json!({
                "label": body.label.as_str(),
                "expected": expected.as_str(),
                "note": note,
            })),
    )
    .await?;
    let row = screenings::get(&mut *tx, id)
        .await?
        .ok_or_else(ApiError::internal)?;
    tx.commit().await?;
    Ok(Json(screening_view(&state.prompts, row)?))
}

/// Removes the label of a screening.
#[utoipa::path(
    delete,
    path = "/api/admin/screenings/{id}/label",
    tag = "admin",
    operation_id = "unlabelAdminScreening",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de la décision de filtrage")),
    responses(
        (status = 200, description = "Étiquette retirée.", body = AdminScreening),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Décision inconnue (`screening_not_found`).", body = ErrorBody)
    )
)]
pub async fn unlabel(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<AdminScreening>, ApiError> {
    let mut tx = state.pool.begin().await?;
    screenings::lock_decision(&mut tx, id)
        .await?
        .ok_or_else(screening_not_found)?;
    screenings::set_label(&mut tx, id, None).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("screening.unlabel").target("screening", id),
    )
    .await?;
    let row = screenings::get(&mut *tx, id)
        .await?
        .ok_or_else(ApiError::internal)?;
    tx.commit().await?;
    Ok(Json(screening_view(&state.prompts, row)?))
}

text_enum! {
    /// File of the calibration bench to produce.
    pub enum CalibrationPart {
        /// `screening_results.jsonl`: raw answers of the decision model.
        Results => "results",
        /// `screening_samples.jsonl`: labelled texts (expected decision, failing criteria).
        Samples => "samples",
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminCalibrationQuery {
    /// `results` (default) or `samples` (labelled screenings only).
    #[serde(default)]
    pub part: Option<CalibrationPart>,
    #[serde(default)]
    pub kind: Option<TargetKind>,
    /// Only the labelled screenings (always the case for `samples`).
    #[serde(default)]
    pub labeled_only: Option<bool>,
    /// Lines at most, most recent first (5 000 by default, 20 000 at most).
    #[serde(default)]
    #[param(minimum = 1, maximum = 20000)]
    pub limit: Option<u32>,
}

/// One line of `screening_results.jsonl`.
fn result_line(
    prompts: &Prompts,
    hashes: &BTreeMap<String, String>,
    row: &CalibrationRow,
) -> Value {
    let hash = if row.prompt_version == prompts.screening.version {
        hashes.get(&row.target_kind).cloned().unwrap_or_default()
    } else {
        // Questions of another grid version: the bench reports these lines as stale.
        format!("grid-{}", row.prompt_version)
    };
    json!({
        "id": row.id.to_string(),
        "section": row.target_kind,
        "questions_hash": hash,
        "latency_ms": row.latency_ms.unwrap_or(0),
        "input_tokens": 0,
        "answers": row.raw_answers.clone().unwrap_or(Value::Null),
        "grid_version": row.prompt_version,
    })
}

/// One line of `screening_samples.jsonl`, `None` without a label.
fn sample_line(taxonomy: &Taxonomy, row: &CalibrationRow) -> Option<Value> {
    let expected: Decision = row.human_label.as_deref()?.parse().ok()?;
    let reached: Option<Decision> = row
        .final_decision
        .as_deref()
        .and_then(|d| d.parse().ok())
        .or(match row.outcome.as_deref() {
            Some("accepted") => Some(Decision::Accept),
            Some("rejected") => Some(Decision::Reject),
            _ => None,
        });
    // The failing criteria are known when the model rightly refused the text.
    let fails: Vec<String> = if expected == Decision::Reject && reached == Some(Decision::Reject) {
        row.criterion_verdicts
            .as_object()
            .map(|verdicts| {
                verdicts
                    .iter()
                    .filter(|(_, v)| v.as_str() == Some(CriterionVerdict::Fail.as_str()))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut tags = vec!["production".to_owned()];
    if let Some(outcome) = &row.outcome {
        tags.push(outcome.clone());
    }
    if row.text.is_none() {
        tags.push("text_purged".to_owned());
    }
    let mut line = json!({
        "id": row.id.to_string(),
        "section": row.target_kind,
        "text": row.text.clone().unwrap_or_default(),
        "expected": expected.as_str(),
        "fails": fails,
        "gray_ok": false,
        "split": "tune",
        "tags": tags,
    });
    let parent = row.parent_summary.clone().unwrap_or_default();
    match row.target_kind.as_str() {
        "contribution" => {
            line["parent"] = json!(parent);
            line["kind"] = json!(row.label_key.clone().unwrap_or_default());
        }
        "report" => {
            let key = row.label_key.clone().unwrap_or_default();
            let label = Taxonomy::label(&taxonomy.report_reasons, &key)
                .unwrap_or(&key)
                .to_owned();
            line["parent"] = json!(parent);
            line["reason_label"] = json!(label);
        }
        _ => {}
    }
    Some(line)
}

/// JSON Lines export of the screenings for `docs/calibration/bench_screening.py`: `results`
/// gives the raw answers (`id`, `section`, `questions_hash`, `latency_ms`, `input_tokens`,
/// `answers`), `samples` the labelled texts (`id`, `section`, `text`, `expected`, `fails`,
/// `gray_ok`, `split`, `tags`, plus `parent` and `kind` or `reason_label`). The ids match
/// between both files. Texts are masked and absent once erased.
#[utoipa::path(
    get,
    path = "/api/admin/screenings/export.jsonl",
    tag = "admin",
    operation_id = "exportAdminScreenings",
    security(("admin_token" = [])),
    params(AdminCalibrationQuery),
    responses(
        (status = 200, description = "Fichier JSON Lines.", content_type = "application/x-ndjson",
         body = String),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn export(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminCalibrationQuery>,
) -> Result<Response, ApiError> {
    let part = query.part.unwrap_or(CalibrationPart::Results);
    let limit = query.limit.unwrap_or(EXPORT_DEFAULT_LIMIT);
    if !(1..=EXPORT_MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request(
            "invalid_query",
            "La limite doit être comprise entre 1 et 20 000 lignes.",
        ));
    }
    let labeled_only = part == CalibrationPart::Samples || query.labeled_only.unwrap_or(false);
    let rows =
        screenings::calibration_rows(&state.pool, query.kind, labeled_only, i64::from(limit))
            .await?;
    let prompts = &state.prompts;
    let hashes: BTreeMap<String, String> = TargetKind::ALL
        .iter()
        .map(|kind| {
            (
                kind.as_str().to_owned(),
                questions_hash(prompts.screening.section(*kind)),
            )
        })
        .collect();
    let mut body = String::new();
    for row in &rows {
        let line = match part {
            CalibrationPart::Results => Some(result_line(prompts, &hashes, row)),
            CalibrationPart::Samples => sample_line(&prompts.taxonomy, row),
        };
        if let Some(line) = line {
            body.push_str(&line.to_string());
            body.push('\n');
        }
    }
    let file_name = format!(
        "attachment; filename=\"iapolitique-screening-{}-{}.jsonl\"",
        part.as_str(),
        Utc::now().format("%Y-%m-%d")
    );
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&file_name).map_err(|_| ApiError::internal())?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_decision_reached() {
        use Decision::{Accept, Reject};
        use LabelVerdict::{Correct, Incorrect};
        assert_eq!(
            expected_decision(Correct, None, Some(Accept)).unwrap(),
            Accept
        );
        assert_eq!(
            expected_decision(Incorrect, None, Some(Accept)).unwrap(),
            Reject
        );
        assert_eq!(
            expected_decision(Incorrect, Some(Reject), Some(Accept)).unwrap(),
            Reject
        );
        assert_eq!(
            expected_decision(Incorrect, Some(Accept), None).unwrap(),
            Accept
        );
        for (label, expected, reached) in [
            (Correct, Some(Reject), Some(Accept)),
            (Incorrect, Some(Accept), Some(Accept)),
            (Correct, None, None),
            (Incorrect, None, None),
            (Correct, Some(Accept), None),
        ] {
            let err = expected_decision(label, expected, reached).unwrap_err();
            assert_eq!(
                err.code, "label_inconsistent",
                "{label:?} {expected:?} {reached:?}"
            );
        }
    }
}
