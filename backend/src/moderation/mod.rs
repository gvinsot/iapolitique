//! Moderation of the submitted texts (AGENTS.md §5.2, §5.3, §5.5, §5.6), shared by the api
//! (synchronous screening at submission) and the worker (retries, second opinions).
//!
//! A text (idea, contribution or report) is stored `submitted`, then:
//!
//! - [`screen`] asks the decision model and settles the outcome in one transaction:
//!   `submitted → screening → accepted | rejected | second_opinion`, each change recorded in the
//!   history. When the decision service fails, the text stays `submitted` and the screening job
//!   enqueued with it retries later.
//! - [`second_opinion::run`] asks Mistral about a text in the gray zone and settles
//!   `second_opinion → accepted | rejected`.
//! - An accepted text enqueues what follows: duplicate detection of an idea, re-analysis of the
//!   idea of a contribution, grouped review of the reports of an idea (§5.6: at most one review
//!   per idea and per 24 h, `contenu_problematique` without delay).
//!
//! Every step checks the current status under a row lock: a step that finds the text already
//! decided, deleted by its author or moved by another process changes nothing ([`Step::Stale`]).
//! Texts are never logged, only identifiers.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::ai::decision::{self, Screener, ScreeningError, ScreeningRequest, ScreeningResult};
use crate::ai::prompts::{Prompts, Taxonomy};
use crate::db::status::{self, Change, StatusError, StatusTable};
use crate::domain::status::{ContributionStatus, IdeaStatus, ReportStatus, ScreenedStatus};
use crate::domain::types::{Actor, AnalysisTrigger, ReportReason, ScreeningOutcome, TargetKind};
use crate::jobs::{pipeline, queue};

pub mod second_opinion;
pub mod statement;

/// Delay before the screening job of a new text runs: the api screens synchronously, the job
/// only takes over when the decision service failed (or the api stopped meanwhile).
pub const SCREENING_RETRY_DELAY: Duration = Duration::seconds(60);
/// Minimum delay between two AI reviews of the reports of an idea (§5.6).
pub const REPORT_REVIEW_INTERVAL: Duration = Duration::hours(24);

/// A screened text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetKind,
    pub id: Uuid,
}

impl Target {
    pub const fn new(kind: TargetKind, id: Uuid) -> Self {
        Self { kind, id }
    }
}

/// French name of a section of `screening.yaml` (`{{section}}` of `second_opinion.md`).
pub const fn section_label(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Idea => "idée",
        TargetKind::Contribution => "contribution",
        TargetKind::Report => "signalement",
    }
}

/// Where a text stands after a moderation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Accepted,
    Rejected {
        reason_code: String,
    },
    /// Gray zone: the second opinion is pending.
    SecondOpinion,
    /// The decision service failed: still `submitted`, retried by a job.
    Pending,
    /// The text was not in the expected status (already decided, deleted…): nothing changed.
    Stale,
}

impl Step {
    /// Status of the text after the step (`None` for [`Step::Stale`]): the same text for ideas,
    /// contributions and reports.
    pub fn status(&self) -> Option<&'static str> {
        match self {
            Self::Accepted => Some("accepted"),
            Self::Rejected { .. } => Some("rejected"),
            Self::SecondOpinion => Some("second_opinion"),
            Self::Pending => Some("submitted"),
            Self::Stale => None,
        }
    }
}

/// Failure of a moderation step (database only: the AI failures are outcomes).
#[derive(Debug, thiserror::Error)]
pub enum ModerationError {
    #[error(transparent)]
    Status(#[from] StatusError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// What the moderation reads about a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Content {
    pub status: String,
    /// The idea itself, or the idea concerned by the contribution or the report.
    pub idea_id: Uuid,
    /// Masked text of the idea or of the contribution, details of the report.
    pub text: Option<String>,
    /// Neutral summary of the published idea (contributions and reports).
    pub idea_summary: Option<String>,
    /// Kind of contribution or reason of the report, as a French label.
    pub label: Option<String>,
}

impl Content {
    /// `state` sent to the decision model; `None` when there is nothing to screen (report
    /// without details).
    pub fn screening_state(&self, kind: TargetKind) -> Option<Value> {
        let summary = self.idea_summary.as_deref().unwrap_or_default();
        let label = self.label.as_deref().unwrap_or_default();
        match kind {
            TargetKind::Idea => self.text.as_deref().map(decision::idea_state),
            TargetKind::Contribution => self
                .text
                .as_deref()
                .map(|text| decision::contribution_state(summary, label, text)),
            TargetKind::Report => self
                .text
                .as_deref()
                .map(|details| decision::report_state(summary, label, Some(details))),
        }
    }

    /// `{{content}}` of `second_opinion.md`: the text, preceded for a contribution or a report
    /// by the summary of the idea, as in the `state` of the decision model.
    pub fn prompt_content(&self, kind: TargetKind) -> String {
        let summary = self.idea_summary.as_deref().unwrap_or_default();
        let label = self.label.as_deref().unwrap_or_default();
        let text = self.text.as_deref().unwrap_or_default();
        match kind {
            TargetKind::Idea => text.to_owned(),
            TargetKind::Contribution => format!(
                "Résumé de l'idée concernée :\n{summary}\n\nType : {label}\n\nContribution :\n{text}"
            ),
            TargetKind::Report => format!(
                "Résumé de l'idée concernée :\n{summary}\n\nMotif du signalement : {label}\n\nPrécision :\n{text}"
            ),
        }
    }
}

fn contribution_label(taxonomy: &Taxonomy, kind: &str) -> String {
    Taxonomy::label(&taxonomy.contribution_kinds, kind)
        .unwrap_or(kind)
        .to_lowercase()
}

fn report_label(taxonomy: &Taxonomy, reason: &str) -> String {
    Taxonomy::label(&taxonomy.report_reasons, reason)
        .unwrap_or(reason)
        .to_owned()
}

#[derive(sqlx::FromRow)]
struct ContentRow {
    status: String,
    idea_id: Uuid,
    text: Option<String>,
    idea_summary: Option<String>,
    /// Contribution kind or report reason.
    key: Option<String>,
}

/// Reads what the moderation needs about a text, `None` when it does not exist.
pub(crate) async fn load_content(
    pool: &PgPool,
    prompts: &Prompts,
    target: Target,
) -> sqlx::Result<Option<Content>> {
    let sql = match target.kind {
        TargetKind::Idea => {
            "SELECT status, id AS idea_id, original_text AS text, NULL::text AS idea_summary,
                    NULL::text AS key
             FROM ideas WHERE id = $1"
        }
        TargetKind::Contribution => {
            "SELECT c.status, c.idea_id, c.text, i.summary AS idea_summary, c.kind AS key
             FROM contributions c JOIN ideas i ON i.id = c.idea_id WHERE c.id = $1"
        }
        TargetKind::Report => {
            "SELECT r.status, r.idea_id, r.details AS text, i.summary AS idea_summary,
                    r.reason AS key
             FROM reports r JOIN ideas i ON i.id = r.idea_id WHERE r.id = $1"
        }
    };
    let row: Option<ContentRow> = sqlx::query_as(sql)
        .bind(target.id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| {
        let ContentRow {
            status,
            idea_id,
            text,
            idea_summary,
            key,
        } = row;
        let label = key.map(|key| match target.kind {
            TargetKind::Contribution => contribution_label(&prompts.taxonomy, &key),
            _ => report_label(&prompts.taxonomy, &key),
        });
        Content {
            status,
            idea_id,
            text,
            idea_summary,
            label,
        }
    }))
}

/// Screens a `submitted` text with the decision model and settles the outcome. A report
/// without details is accepted without screening (there is no visitor text to check).
///
/// `second_opinion_at`: when the Mistral second opinion job of a gray zone may run (the api
/// delays it while it tries the second opinion inline); `None`: at once (database clock).
pub async fn screen(
    pool: &PgPool,
    screener: &Screener,
    prompts: &Prompts,
    target: Target,
    job_id: Option<i64>,
    actor: Actor,
    second_opinion_at: Option<DateTime<Utc>>,
) -> Result<Step, ModerationError> {
    let Some(content) = load_content(pool, prompts, target).await? else {
        return Ok(Step::Stale);
    };
    if content.status != "submitted" {
        return Ok(Step::Stale);
    }
    let Some(state) = content.screening_state(target.kind) else {
        let change = Change::by(actor).details(json!({ "screening": "skipped_without_text" }));
        return settle(
            pool,
            target,
            Stage::Screening,
            Verdict::Accept,
            change,
            None,
        )
        .await;
    };
    let request = ScreeningRequest {
        kind: target.kind,
        target_id: target.id,
        idea_id: Some(content.idea_id),
        job_id,
        state,
    };
    match screener.screen(pool, &request).await {
        Ok(result) => apply_screening(pool, target, &result, actor, second_opinion_at).await,
        Err(ScreeningError::Service { .. }) => Ok(Step::Pending),
        Err(ScreeningError::Database(err)) => Err(err.into()),
    }
}

/// Settles the result of the decision model.
pub async fn apply_screening(
    pool: &PgPool,
    target: Target,
    result: &ScreeningResult,
    actor: Actor,
    second_opinion_at: Option<DateTime<Utc>>,
) -> Result<Step, ModerationError> {
    let evaluation = &result.evaluation;
    let verdict = match evaluation.outcome {
        ScreeningOutcome::Accepted => Verdict::Accept,
        ScreeningOutcome::Rejected => Verdict::Reject(
            evaluation
                .reason_code
                .clone()
                .unwrap_or_else(|| "rejected".to_owned()),
        ),
        ScreeningOutcome::GrayZone => Verdict::Gray,
    };
    let mut change = Change::by(actor).details(json!({
        "screening_id": result.screening_id,
        "prompt_version": result.prompt_version,
    }));
    if let Verdict::Reject(code) = &verdict {
        change = change.reason(code.clone());
    }
    settle(
        pool,
        target,
        Stage::Screening,
        verdict,
        change,
        second_opinion_at,
    )
    .await
}

/// Moderation stage being settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// From `submitted`, through `screening`.
    Screening,
    /// From `second_opinion`.
    SecondOpinion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Accept,
    Reject(String),
    /// Only from the screening stage.
    Gray,
}

/// Applies a verdict in one transaction (see [`settle_in`]).
pub(crate) async fn settle(
    pool: &PgPool,
    target: Target,
    stage: Stage,
    verdict: Verdict,
    change: Change,
    second_opinion_at: Option<DateTime<Utc>>,
) -> Result<Step, ModerationError> {
    let mut tx = pool.begin().await?;
    let step = settle_in(&mut tx, target, stage, verdict, &change, second_opinion_at).await?;
    tx.commit().await?;
    Ok(step)
}

/// Applies a verdict: status transitions with their history, rejection fields, follow-up jobs.
pub(crate) async fn settle_in(
    conn: &mut PgConnection,
    target: Target,
    stage: Stage,
    verdict: Verdict,
    change: &Change,
    second_opinion_at: Option<DateTime<Utc>>,
) -> Result<Step, ModerationError> {
    match target.kind {
        TargetKind::Idea => {
            settle_typed::<IdeaStatus>(conn, target, stage, verdict, change, second_opinion_at)
                .await
        }
        TargetKind::Contribution => {
            settle_typed::<ContributionStatus>(
                conn,
                target,
                stage,
                verdict,
                change,
                second_opinion_at,
            )
            .await
        }
        TargetKind::Report => {
            settle_typed::<ReportStatus>(conn, target, stage, verdict, change, second_opinion_at)
                .await
        }
    }
}

async fn settle_typed<S: ScreenedStatus + StatusTable>(
    conn: &mut PgConnection,
    target: Target,
    stage: Stage,
    verdict: Verdict,
    change: &Change,
    second_opinion_at: Option<DateTime<Utc>>,
) -> Result<Step, ModerationError> {
    let expected = match stage {
        Stage::Screening => S::SUBMITTED,
        Stage::SecondOpinion => S::SECOND_OPINION,
    };
    if status::lock::<S>(conn, target.id).await? != Some(expected) {
        return Ok(Step::Stale);
    }
    let last = match verdict {
        Verdict::Accept => S::ACCEPTED,
        Verdict::Reject(_) => S::REJECTED,
        Verdict::Gray => S::SECOND_OPINION,
    };
    let path: &[S] = match stage {
        Stage::Screening => &[S::SCREENING, last],
        Stage::SecondOpinion => &[last],
    };
    status::transition_path(conn, target.id, path, change).await?;

    // The job that would have retried this stage is not needed any more.
    let pending_job = match stage {
        Stage::Screening => pipeline::screen_key(target.kind, target.id),
        Stage::SecondOpinion => pipeline::second_opinion_key(target.kind, target.id),
    };
    queue::cancel_queued(&mut *conn, &pending_job).await?;

    Ok(match verdict {
        Verdict::Accept => {
            follow_up(conn, target).await?;
            Step::Accepted
        }
        Verdict::Reject(reason_code) => {
            sqlx::query(S::REJECT_SQL)
                .bind(target.id)
                .bind(&reason_code)
                .execute(&mut *conn)
                .await?;
            Step::Rejected { reason_code }
        }
        Verdict::Gray => {
            let mut job = pipeline::second_opinion_job(target.kind, target.id);
            if let Some(at) = second_opinion_at {
                job = job.run_after(at);
            }
            queue::enqueue(&mut *conn, &job).await?;
            Step::SecondOpinion
        }
    })
}

/// Enqueues what follows the acceptance of a text.
async fn follow_up(conn: &mut PgConnection, target: Target) -> sqlx::Result<()> {
    match target.kind {
        TargetKind::Idea => {
            queue::enqueue(&mut *conn, &pipeline::dedup_job(target.id)).await?;
        }
        TargetKind::Contribution => {
            let (idea_id,): (Uuid,) =
                sqlx::query_as("SELECT idea_id FROM contributions WHERE id = $1")
                    .bind(target.id)
                    .fetch_one(&mut *conn)
                    .await?;
            queue::enqueue(
                &mut *conn,
                &pipeline::analyze_job(idea_id, AnalysisTrigger::Contribution),
            )
            .await?;
        }
        TargetKind::Report => schedule_report_review(conn, target.id).await?,
    }
    Ok(())
}

/// Schedules the grouped review of the reports of an idea (§5.6): at most one review per idea
/// and per 24 hours, except for `contenu_problematique`, reviewed without delay (a queued review
/// is then brought forward).
async fn schedule_report_review(conn: &mut PgConnection, report_id: Uuid) -> sqlx::Result<()> {
    // Times come from the database clock, like the `run_after` the queue compares them with.
    let (idea_id, urgent, now, run_after): (Uuid, bool, DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT r.idea_id, r.reason = $2, now(),
                    CASE WHEN r.reason = $2 OR i.last_report_review_at IS NULL THEN now()
                         ELSE greatest(now(), i.last_report_review_at
                                              + make_interval(secs => $3)) END
             FROM reports r JOIN ideas i ON i.id = r.idea_id WHERE r.id = $1",
        )
        .bind(report_id)
        .bind(ReportReason::ProblematicContent.as_str())
        .bind(REPORT_REVIEW_INTERVAL.num_seconds() as f64)
        .fetch_one(&mut *conn)
        .await?;
    let enqueued = queue::enqueue(
        &mut *conn,
        &pipeline::review_reports_job(idea_id, run_after),
    )
    .await?;
    if enqueued.is_none() && urgent {
        queue::bring_forward(&mut *conn, &pipeline::review_reports_key(idea_id), now).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
