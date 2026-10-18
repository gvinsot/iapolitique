//! `screenings`: decisions of the decision model (AGENTS.md §5.3), kept **without text** for the
//! calibration of the thresholds.

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::domain::types::{ScreeningOutcome, TargetKind};

/// A screening to record. Either `outcome` (the service answered) or `error_code` is set.
#[derive(Debug, Clone, PartialEq)]
pub struct NewScreening {
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    /// Version of `prompts/screening.yaml`.
    pub prompt_version: String,
    /// `{ criterion: { reject_below, accept_above } }` actually applied.
    pub thresholds: Value,
    /// Raw `answers` of the service; `None` when the call failed.
    pub raw_answers: Option<Value>,
    /// `{ criterion: probability of passing }`.
    pub pass_probabilities: Value,
    /// `{ criterion: "pass" | "fail" | "gray" }`.
    pub criterion_verdicts: Value,
    pub outcome: Option<ScreeningOutcome>,
    pub reason_code: Option<String>,
    pub latency_ms: i32,
    pub error_code: Option<String>,
}

/// Inserts the screening and returns its id. A clear-cut outcome is also the final decision.
pub async fn insert<'e>(
    executor: impl PgExecutor<'e>,
    screening: &NewScreening,
) -> sqlx::Result<Uuid> {
    let final_decision = match screening.outcome {
        Some(ScreeningOutcome::Accepted) => Some("accept"),
        Some(ScreeningOutcome::Rejected) => Some("reject"),
        Some(ScreeningOutcome::GrayZone) | None => None,
    };
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO screenings (target_kind, target_id, prompt_version, thresholds, raw_answers,
             pass_probabilities, criterion_verdicts, outcome, reason_code, gray_zone, latency_ms,
             error_code, final_decision, decided_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::text,
             CASE WHEN $13::text IS NULL THEN NULL ELSE now() END)
         RETURNING id",
    )
    .bind(screening.target_kind.as_str())
    .bind(screening.target_id)
    .bind(&screening.prompt_version)
    .bind(&screening.thresholds)
    .bind(&screening.raw_answers)
    .bind(&screening.pass_probabilities)
    .bind(&screening.criterion_verdicts)
    .bind(screening.outcome.map(ScreeningOutcome::as_str))
    .bind(screening.reason_code.as_deref())
    .bind(screening.outcome == Some(ScreeningOutcome::GrayZone))
    .bind(screening.latency_ms.max(0))
    .bind(screening.error_code.as_deref())
    .bind(final_decision)
    .fetch_one(executor)
    .await?;
    Ok(id)
}
