//! Grouped AI review of the reports of an idea (AGENTS.md §5.6, §7.5), job `review_reports`.
//!
//! One job per idea (key `review_reports:<idea>`) takes every accepted report of the idea
//! (50 at most), unless the idea was reviewed less than 24 hours ago and none of them is
//! `contenu_problematique` (it then waits until the 24 hours have passed). Mistral reads the
//! idea, its current analysis and the reports (`prompts/report_review.md`) and concludes:
//!
//! - `maintain`: nothing changes;
//! - `update`: a re-analysis is queued (trigger `report`), which receives the corrections;
//! - `hide`: the idea leaves the portal (`hidden_reason = report_review`) and the exports are
//!   regenerated;
//! - `escalate` (serious content, always with `hide`): hidden at once
//!   (`hidden_reason = escalation`) and queued in `escalations` for a human (PHAROS, 24 h).
//!
//! The review is stored in `report_reviews`; the reports become `reviewed` (their authors see
//! the outcome on their tracking page) and `ideas.last_report_review_at` is set. Reports that
//! arrived during the review could not enqueue a job (same key): the job runs again for them,
//! at once for `contenu_problematique`, 24 hours later otherwise.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::guard::{self, Gate};
use super::pipeline::{self, IdeaPayload};
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobError, JobKind, JobOutcome, queue};
use crate::ai::mistral::{CallContext, ModelTier};
use crate::ai::outputs::ReportReviewOutput;
use crate::ai::prompts::{PromptVars, Taxonomy};
use crate::db::status::{self, Change, StatusError};
use crate::db::{escalation_seals, ideas};
use crate::domain::status::{IdeaStatus, ReportStatus};
use crate::domain::types::{
    Actor, AiPurpose, AnalysisTrigger, HiddenReason, ReportReason, ReviewOutcome,
};
use crate::moderation::REPORT_REVIEW_INTERVAL;

/// Reports reviewed together at most (prompt size); the next ones wait for the next run.
pub const MAX_REPORTS_PER_REVIEW: i64 = 50;

/// What the review reads about the idea.
#[derive(Debug, sqlx::FromRow)]
struct IdeaToReview {
    status: String,
    text: Option<String>,
    analysis_id: Option<Uuid>,
    analysis: Option<Value>,
    last_review: Option<DateTime<Utc>>,
    today: chrono::NaiveDate,
    now: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct PendingReport {
    id: Uuid,
    reason: String,
    details: Option<String>,
}

/// `{{reports}}`: one line per report, « identifiant — motif — précision ».
fn reports_for_prompt(taxonomy: &Taxonomy, reports: &[PendingReport]) -> String {
    reports
        .iter()
        .map(|r| {
            let reason = Taxonomy::label(&taxonomy.report_reasons, &r.reason).unwrap_or(&r.reason);
            let details = r
                .details
                .as_deref()
                .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "(aucune précision)".to_owned());
            format!("{} — {reason} — {details}", r.id)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn status_error(err: StatusError) -> JobError {
    match err {
        StatusError::Database(err) => err.into(),
        other => JobError::Permanent(other.to_string()),
    }
}

/// When the next review of the idea may run: at once if a report is `contenu_problematique`
/// or the idea was never reviewed, 24 hours after the last review otherwise.
fn due(last_review: Option<DateTime<Utc>>, now: DateTime<Utc>, urgent: bool) -> DateTime<Utc> {
    match last_review {
        Some(last) if !urgent => (last + REPORT_REVIEW_INTERVAL).max(now),
        _ => now,
    }
}

/// Handler of [`JobKind::ReviewReports`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ReviewReportsHandler;

impl JobHandler for ReviewReportsHandler {
    fn kind(&self) -> JobKind {
        JobKind::ReviewReports
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let IdeaPayload { idea_id } = job.payload_as()?;
            let pool = &ctx.pool;
            let idea: Option<IdeaToReview> = sqlx::query_as(
                "SELECT i.status, i.original_text AS text, i.current_analysis_id AS analysis_id,
                        a.content AS analysis, i.last_report_review_at AS last_review,
                        (now() AT TIME ZONE 'Europe/Paris')::date AS today, now() AS now
                 FROM ideas i LEFT JOIN analyses a ON a.id = i.current_analysis_id
                 WHERE i.id = $1",
            )
            .bind(idea_id)
            .fetch_optional(pool)
            .await?;
            let Some(idea) = idea else {
                return Ok(JobOutcome::Done);
            };
            let reviewable = idea
                .status
                .parse::<IdeaStatus>()
                .is_ok_and(IdeaStatus::has_publication);
            let Some(text) = idea.text.as_deref().filter(|_| reviewable) else {
                return Ok(JobOutcome::Done);
            };
            let reports: Vec<PendingReport> = sqlx::query_as(
                "SELECT id, reason, details FROM reports
                 WHERE idea_id = $1 AND status = 'accepted'
                 ORDER BY created_at, id LIMIT $2",
            )
            .bind(idea_id)
            .bind(MAX_REPORTS_PER_REVIEW)
            .fetch_all(pool)
            .await?;
            if reports.is_empty() {
                return Ok(JobOutcome::Done);
            }
            let urgent = reports
                .iter()
                .any(|r| r.reason == ReportReason::ProblematicContent.as_str());
            let due_at = due(idea.last_review, idea.now, urgent);
            if due_at > idea.now {
                return Ok(JobOutcome::RetryAt(due_at));
            }
            let client = match guard::mistral_gate(ctx, self.kind()).await? {
                Gate::Ready(client) => client,
                Gate::Postpone(outcome) => return Ok(outcome),
            };

            let template = &ctx.prompts.report_review;
            let rendered = template
                .render(
                    &PromptVars::new()
                        .set("today", idea.today.format("%Y-%m-%d").to_string())
                        .set("idea_text", text)
                        .set(
                            "analysis",
                            idea.analysis
                                .as_ref()
                                .map_or_else(|| "{}".to_owned(), Value::to_string),
                        )
                        .set(
                            "reports",
                            reports_for_prompt(&ctx.prompts.taxonomy, &reports),
                        ),
                )
                .map_err(|err| JobError::Permanent(err.to_string()))?;
            let schema = template
                .output_schema
                .as_ref()
                .expect("report_review.md declares its output schema (checked at load)");
            let call = CallContext {
                prompt_version: Some(template.version.clone()),
                idea_id: Some(idea_id),
                job_id: Some(job.id),
                ..CallContext::new(AiPurpose::ReportReview)
            };
            let completion = match client
                .structured::<ReportReviewOutput>(
                    pool,
                    &call,
                    ModelTier::Review,
                    &rendered,
                    schema,
                    ReportReviewOutput::check,
                )
                .await
            {
                Ok(completion) => completion,
                // Configuration error: the reports wait for a working key (§6.4).
                Err(err) if err.is_auth() => return guard::auth_postponed(ctx).await,
                Err(err) if err.is_transient() => return Err(JobError::Transient(err.to_string())),
                Err(err) => return Err(JobError::Permanent(err.to_string())),
            };
            let review = completion.value.normalized();
            let report_ids: Vec<Uuid> = reports.iter().map(|r| r.id).collect();
            let applied = apply(
                pool,
                idea_id,
                idea.analysis_id,
                &review,
                &completion.model,
                &template.version,
                &report_ids,
            )
            .await?;
            if !applied {
                return Ok(JobOutcome::Done);
            }

            // Reports accepted while this review ran.
            let left: Vec<(bool, DateTime<Utc>)> = sqlx::query_as(
                "SELECT reason = $2, now() FROM reports WHERE idea_id = $1 AND status = 'accepted'",
            )
            .bind(idea_id)
            .bind(ReportReason::ProblematicContent.as_str())
            .fetch_all(pool)
            .await?;
            match left.first() {
                None => Ok(JobOutcome::Done),
                Some((_, now)) => {
                    let urgent = left.iter().any(|(urgent, _)| *urgent);
                    Ok(JobOutcome::RetryAt(due(Some(*now), *now, urgent)))
                }
            }
        })
    }
}

/// Stores the review and applies its outcome in one transaction. Returns `false` when the idea
/// disappeared or is no longer published or hidden (nothing stored).
async fn apply(
    pool: &PgPool,
    idea_id: Uuid,
    analysis_id: Option<Uuid>,
    review: &ReportReviewOutput,
    model: &str,
    prompt_version: &str,
    report_ids: &[Uuid],
) -> Result<bool, JobError> {
    let mut tx = pool.begin().await?;
    let current = status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?;
    if !current.is_some_and(IdeaStatus::has_publication) {
        return Ok(false);
    }
    let review_id: Uuid = sqlx::query_scalar(
        "INSERT INTO report_reviews (idea_id, analysis_id, outcome, explanation, model,
                                     prompt_version, corrections, escalate)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(idea_id)
    .bind(analysis_id)
    .bind(review.outcome.as_str())
    .bind(&review.explanation)
    .bind(model)
    .bind(prompt_version)
    .bind(&review.corrections)
    .bind(review.escalate)
    .fetch_one(&mut *tx)
    .await?;
    let change = Change::by(Actor::Worker)
        .reason(review.outcome.as_str())
        .details(json!({ "review_id": review_id, "escalate": review.escalate }));

    for &report_id in report_ids {
        if status::lock::<ReportStatus>(&mut tx, report_id)
            .await
            .map_err(status_error)?
            != Some(ReportStatus::Accepted)
        {
            continue;
        }
        // `reports_reviewed_has_review`: the review is set before the status.
        sqlx::query("UPDATE reports SET review_id = $2 WHERE id = $1")
            .bind(report_id)
            .bind(review_id)
            .execute(&mut *tx)
            .await?;
        status::transition(&mut tx, report_id, ReportStatus::Reviewed, &change)
            .await
            .map_err(status_error)?;
    }

    match review.outcome {
        ReviewOutcome::Maintain => {}
        ReviewOutcome::Update => {
            queue::enqueue(
                &mut *tx,
                &pipeline::analyze_job(idea_id, AnalysisTrigger::Report),
            )
            .await?;
        }
        ReviewOutcome::Hide => {
            let reason = if review.escalate {
                HiddenReason::Escalation
            } else {
                HiddenReason::ReportReview
            };
            let change = Change {
                reason_code: Some(reason.as_str().to_owned()),
                ..change.clone()
            };
            if ideas::hide(&mut tx, idea_id, reason, &change)
                .await
                .map_err(status_error)?
            {
                queue::enqueue(&mut *tx, &pipeline::exports_job()).await?;
            }
        }
    }
    if review.escalate {
        let escalation: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO escalations (idea_id, review_id) VALUES ($1, $2)
             ON CONFLICT (idea_id) WHERE status = 'pending' DO NOTHING
             RETURNING id",
        )
        .bind(idea_id)
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await?;
        // Sealed copy, kept one year whatever happens to the idea (owner decision D3, §7.5).
        if let Some(escalation) = escalation {
            escalation_seals::seal(&mut tx, escalation).await?;
        }
    }
    sqlx::query("UPDATE ideas SET last_report_review_at = now() WHERE id = $1")
        .bind(idea_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if review.escalate {
        tracing::warn!(idea_id = %idea_id, review_id = %review_id, "serious content hidden and escalated");
    } else {
        tracing::info!(
            idea_id = %idea_id,
            review_id = %review_id,
            outcome = review.outcome.as_str(),
            "reports reviewed"
        );
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
