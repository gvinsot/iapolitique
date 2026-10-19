//! Gate of the handlers that call Mistral (AGENTS.md §6.4, §13.1).
//!
//! Before calling Mistral, a handler asks [`mistral_gate`]:
//!
//! - without `MISTRAL_API_KEY`, the job is rescheduled (not failed, no attempt consumed) with a
//!   warning; the worker normally does not even claim these kinds without a key, so this only
//!   happens when a job is handled by a context built without a client;
//! - while Mistral refuses the API key (HTTP 401/403, alert `mistral_auth` of
//!   `provider_alerts`), one job per period calls it to see whether the key was fixed and the
//!   others are rescheduled without calling it; a handler whose call is refused reschedules its
//!   job with [`auth_postponed`] (15 min to 1 h, growing with the age of the alert): no attempt
//!   consumed, no step failed or skipped;
//! - once the daily budget is reached, the job waits for the next Paris midnight.
//!
//! The worker applies the budget check before every Mistral job too; the handlers repeat it
//! because they may chain several calls (analysis: research, then structuring).

use chrono::{Duration, Utc};

use super::worker::JobContext;
use super::{JobError, JobKind, JobOutcome};
use crate::ai::budget;
use crate::ai::mistral::MistralClient;
use crate::db::provider_alerts::{self, MISTRAL_AUTH, Probe};

/// Delay before a job that needs Mistral runs again when no API key is configured.
pub const MISSING_KEY_RETRY: Duration = Duration::hours(1);

/// Whether a handler may call Mistral now.
#[derive(Debug)]
pub enum Gate<'a> {
    Ready(&'a MistralClient),
    /// Run the job again later, without consuming an attempt.
    Postpone(JobOutcome),
}

/// Checks the API key and the daily budget (see the module documentation).
pub async fn mistral_gate(ctx: &JobContext, kind: JobKind) -> Result<Gate<'_>, JobError> {
    let Some(client) = &ctx.mistral else {
        tracing::warn!(
            kind = kind.as_str(),
            "MISTRAL_API_KEY is not set: AI job rescheduled"
        );
        return Ok(Gate::Postpone(JobOutcome::RetryAt(
            Utc::now() + MISSING_KEY_RETRY,
        )));
    };
    if let Some(outcome) =
        budget::postpone_if_exceeded(&ctx.pool, ctx.config.mistral.daily_budget).await?
    {
        return Ok(Gate::Postpone(outcome));
    }
    match provider_alerts::claim_probe(&ctx.pool, MISTRAL_AUTH).await? {
        Probe::Clear | Probe::Mine => Ok(Gate::Ready(client)),
        Probe::WaitUntil(at) => {
            tracing::debug!(
                kind = kind.as_str(),
                "Mistral API key refused: AI job rescheduled"
            );
            Ok(Gate::Postpone(JobOutcome::RetryAt(at)))
        }
    }
}

/// Outcome of a job whose Mistral call was refused for authentication (HTTP 401/403): run
/// again later, without consuming an attempt (the client already recorded and logged the
/// alert).
pub async fn auth_postponed(ctx: &JobContext) -> Result<JobOutcome, JobError> {
    Ok(JobOutcome::RetryAt(
        provider_alerts::retry_at(&ctx.pool, MISTRAL_AUTH).await?,
    ))
}

/// Only the budget part of [`mistral_gate`], between two calls of the same job.
pub async fn budget_gate(ctx: &JobContext) -> Result<Option<JobOutcome>, JobError> {
    Ok(budget::postpone_if_exceeded(&ctx.pool, ctx.config.mistral.daily_budget).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestApp;
    use sqlx::PgPool;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn postpones_without_key_or_budget(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[("MISTRAL_DAILY_BUDGET_EUR", "1")]).await;
        let ctx = app.job_context();
        assert!(matches!(
            mistral_gate(&ctx, JobKind::AnalyzeIdea).await,
            Ok(Gate::Ready(_))
        ));
        assert_eq!(budget_gate(&ctx).await, Ok(None));

        let mut without_key = ctx.clone();
        without_key.mistral = None;
        let before = Utc::now();
        match mistral_gate(&without_key, JobKind::AnalyzeIdea).await {
            Ok(Gate::Postpone(JobOutcome::RetryAt(at))) => {
                assert!(at >= before + MISSING_KEY_RETRY);
            }
            other => panic!("unexpected {other:?}"),
        }

        sqlx::query(
            "INSERT INTO ai_calls (provider, purpose, model, cost_micro_eur, latency_ms, status)
             VALUES ('mistral', 'analysis_research', 'm', 1000000, 10, 'success')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let next_reset = budget::status(&pool, ctx.config.mistral.daily_budget)
            .await
            .unwrap()
            .next_reset;
        assert!(matches!(
            mistral_gate(&ctx, JobKind::ReviewReports).await,
            Ok(Gate::Postpone(JobOutcome::RetryAt(at))) if at == next_reset
        ));
        assert_eq!(
            budget_gate(&ctx).await,
            Ok(Some(JobOutcome::RetryAt(next_reset)))
        );
    }
}
