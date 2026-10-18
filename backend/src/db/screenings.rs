//! `screenings`: decisions of the decision model (AGENTS.md §5.3), kept **without text** for the
//! calibration of the thresholds.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::domain::types::{Decision, ScreeningOutcome, TargetKind};

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

// ---------------------------------------------------------------------------------------------
// Admin: calibration (AGENTS.md §5.3, §8)
// ---------------------------------------------------------------------------------------------

/// A screening as shown by the admin, with the beginning of the masked text when it still
/// exists.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ScreeningRow {
    pub id: Uuid,
    pub target_kind: String,
    pub target_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub prompt_version: String,
    pub thresholds: Value,
    pub pass_probabilities: Value,
    pub criterion_verdicts: Value,
    pub outcome: Option<String>,
    pub reason_code: Option<String>,
    pub gray_zone: bool,
    pub latency_ms: Option<i32>,
    pub error_code: Option<String>,
    pub second_opinion_decision: Option<String>,
    pub second_opinion_reason_code: Option<String>,
    pub second_opinion_explanation: Option<String>,
    pub second_opinion_model: Option<String>,
    pub second_opinion_prompt_version: Option<String>,
    pub second_opinion_at: Option<DateTime<Utc>>,
    pub final_decision: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub human_label: Option<String>,
    pub human_label_note: Option<String>,
    pub labeled_at: Option<DateTime<Utc>>,
    pub excerpt: Option<String>,
}

/// Filters of the screening list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreeningFilter {
    pub kind: Option<TargetKind>,
    pub outcome: Option<ScreeningOutcome>,
    /// `Some(true)`: labelled only; `Some(false)`: not labelled yet (decided ones only).
    pub labeled: Option<bool>,
    pub reason_code: Option<String>,
}

macro_rules! screening_select {
    () => {
        "SELECT s.id, s.target_kind, s.target_id, s.created_at, s.prompt_version, s.thresholds,
                s.pass_probabilities, s.criterion_verdicts, s.outcome, s.reason_code, s.gray_zone,
                s.latency_ms, s.error_code, s.second_opinion_decision,
                s.second_opinion_reason_code, s.second_opinion_explanation,
                s.second_opinion_model, s.second_opinion_prompt_version, s.second_opinion_at,
                s.final_decision, s.decided_at, s.human_label, s.human_label_note, s.labeled_at,
                left(coalesce(i.original_text, c.text, r.details), 300) AS excerpt
         FROM screenings s
         LEFT JOIN ideas i ON s.target_kind = 'idea' AND i.id = s.target_id
         LEFT JOIN contributions c ON s.target_kind = 'contribution' AND c.id = s.target_id
         LEFT JOIN reports r ON s.target_kind = 'report' AND r.id = s.target_id "
    };
}

/// `$1` kind, `$2` outcome, `$3` labelled, `$4` reason code.
macro_rules! screening_filter {
    () => {
        "WHERE ($1::text IS NULL OR s.target_kind = $1)
           AND ($2::text IS NULL OR s.outcome = $2)
           AND ($3::boolean IS NULL
                OR ($3 AND s.human_label IS NOT NULL)
                OR (NOT $3 AND s.human_label IS NULL AND s.outcome IS NOT NULL))
           AND ($4::text IS NULL OR s.reason_code = $4) "
    };
}

/// Screenings, most recent first, with their total.
pub async fn list(
    pool: &sqlx::PgPool,
    filter: &ScreeningFilter,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<ScreeningRow>, i64)> {
    let kind = filter.kind.map(TargetKind::as_str);
    let outcome = filter.outcome.map(ScreeningOutcome::as_str);
    let total: i64 = sqlx::query_scalar(concat!(
        "SELECT count(*) FROM screenings s ",
        screening_filter!()
    ))
    .bind(kind)
    .bind(outcome)
    .bind(filter.labeled)
    .bind(filter.reason_code.as_deref())
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query_as(concat!(
        screening_select!(),
        screening_filter!(),
        "ORDER BY s.created_at DESC, s.id DESC LIMIT $5 OFFSET $6"
    ))
    .bind(kind)
    .bind(outcome)
    .bind(filter.labeled)
    .bind(filter.reason_code.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}

/// One screening, `None` when it does not exist.
pub async fn get<'e>(
    executor: impl PgExecutor<'e>,
    id: Uuid,
) -> sqlx::Result<Option<ScreeningRow>> {
    sqlx::query_as(concat!(screening_select!(), "WHERE s.id = $1"))
        .bind(id)
        .fetch_optional(executor)
        .await
}

/// Locks a screening and returns the decision it reached (final decision, or the clear-cut
/// outcome of the decision model): `None` when it does not exist, `Some(None)` when it reached
/// no decision (service error, second opinion pending).
pub async fn lock_decision(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<Option<Decision>>> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT final_decision, outcome FROM screenings WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(row.map(|(final_decision, outcome)| {
        final_decision
            .as_deref()
            .and_then(|d| d.parse().ok())
            .or(match outcome.as_deref() {
                Some("accepted") => Some(Decision::Accept),
                Some("rejected") => Some(Decision::Reject),
                _ => None,
            })
    }))
}

/// Sets (or, with `None`, removes) the administrator's label: the decision the screening
/// should have reached.
pub async fn set_label(
    conn: &mut PgConnection,
    id: Uuid,
    label: Option<(Decision, Option<&str>)>,
) -> sqlx::Result<()> {
    let (decision, note) = label.map_or((None, None), |(d, n)| (Some(d.as_str()), n));
    sqlx::query(
        "UPDATE screenings SET human_label = $2, human_label_note = $3,
             labeled_at = CASE WHEN $2::text IS NULL THEN NULL ELSE now() END
         WHERE id = $1",
    )
    .bind(id)
    .bind(decision)
    .bind(note)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A screening as exported for `docs/calibration/bench_screening.py`, with what is needed to
/// rebuild the decision model `state` while the texts exist.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct CalibrationRow {
    pub id: Uuid,
    pub target_kind: String,
    pub prompt_version: String,
    pub raw_answers: Option<Value>,
    pub latency_ms: Option<i32>,
    pub outcome: Option<String>,
    pub final_decision: Option<String>,
    pub human_label: Option<String>,
    pub criterion_verdicts: Value,
    /// Masked text (idea, contribution, report details), `None` once erased.
    pub text: Option<String>,
    /// Neutral summary of the idea of a contribution or a report.
    pub parent_summary: Option<String>,
    /// Contribution kind or report reason.
    pub label_key: Option<String>,
}

/// Screenings answered by the decision model (raw answers kept), most recent first; only the
/// labelled ones with `labeled_only`.
pub async fn calibration_rows(
    pool: &sqlx::PgPool,
    kind: Option<TargetKind>,
    labeled_only: bool,
    limit: i64,
) -> sqlx::Result<Vec<CalibrationRow>> {
    sqlx::query_as(
        "SELECT s.id, s.target_kind, s.prompt_version, s.raw_answers, s.latency_ms, s.outcome,
                s.final_decision, s.human_label, s.criterion_verdicts,
                coalesce(i.original_text, c.text, r.details) AS text,
                p.summary AS parent_summary, coalesce(c.kind, r.reason) AS label_key
         FROM screenings s
         LEFT JOIN ideas i ON s.target_kind = 'idea' AND i.id = s.target_id
         LEFT JOIN contributions c ON s.target_kind = 'contribution' AND c.id = s.target_id
         LEFT JOIN reports r ON s.target_kind = 'report' AND r.id = s.target_id
         LEFT JOIN ideas p ON p.id = coalesce(c.idea_id, r.idea_id)
         WHERE s.raw_answers IS NOT NULL
           AND ($1::text IS NULL OR s.target_kind = $1)
           AND (NOT $2 OR s.human_label IS NOT NULL)
         ORDER BY s.created_at DESC, s.id DESC
         LIMIT $3",
    )
    .bind(kind.map(TargetKind::as_str))
    .bind(labeled_only)
    .bind(limit)
    .fetch_all(pool)
    .await
}
