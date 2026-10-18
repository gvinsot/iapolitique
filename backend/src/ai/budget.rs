//! Daily AI budget (AGENTS.md §6.4): the estimated costs of `ai_calls` of the current
//! Europe/Paris day are summed and compared with `MISTRAL_DAILY_BUDGET_EUR`. Once it is reached,
//! jobs that call Mistral are postponed to the next Paris midnight: they stay queued, they never
//! fail because of the budget.
//!
//! The day boundaries are computed by PostgreSQL in the `Europe/Paris` zone, so that summer and
//! winter time are handled (a day lasts 23 or 25 hours twice a year).

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::domain::money::MicroEur;
use crate::jobs::JobOutcome;

/// Spending of the current Paris day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetStatus {
    pub spent: MicroEur,
    pub limit: MicroEur,
    /// Paris midnight that started the day.
    pub day_start: DateTime<Utc>,
    /// Next Paris midnight, when the budget is renewed.
    pub next_reset: DateTime<Utc>,
}

impl BudgetStatus {
    pub fn exceeded(&self) -> bool {
        self.spent >= self.limit
    }

    pub fn remaining(&self) -> MicroEur {
        MicroEur((self.limit.0 - self.spent.0).max(0))
    }
}

/// Spending of the Paris day containing `at`.
pub async fn status_at(
    pool: &PgPool,
    limit: MicroEur,
    at: DateTime<Utc>,
) -> sqlx::Result<BudgetStatus> {
    let (day_start, next_reset, spent): (DateTime<Utc>, DateTime<Utc>, i64) = sqlx::query_as(
        "WITH bounds AS (
             SELECT date_trunc('day', $1::timestamptz AT TIME ZONE 'Europe/Paris')
                        AT TIME ZONE 'Europe/Paris' AS day_start,
                    (date_trunc('day', $1::timestamptz AT TIME ZONE 'Europe/Paris')
                        + interval '1 day') AT TIME ZONE 'Europe/Paris' AS next_reset
         )
         SELECT b.day_start, b.next_reset,
                coalesce((SELECT sum(c.cost_micro_eur) FROM ai_calls c
                          WHERE c.created_at >= b.day_start AND c.created_at < b.next_reset),
                         0)::bigint
         FROM bounds b",
    )
    .bind(at)
    .fetch_one(pool)
    .await?;
    Ok(BudgetStatus {
        spent: MicroEur(spent),
        limit,
        day_start,
        next_reset,
    })
}

/// Spending of the current Paris day.
pub async fn status(pool: &PgPool, limit: MicroEur) -> sqlx::Result<BudgetStatus> {
    status_at(pool, limit, Utc::now()).await
}

/// `Some(RetryAt(next Paris midnight))` when the daily budget is reached: the job must not call
/// Mistral now. Used by the worker before every job that needs Mistral, and by handlers that
/// chain several calls.
pub async fn postpone_if_exceeded(
    pool: &PgPool,
    limit: MicroEur,
) -> sqlx::Result<Option<JobOutcome>> {
    let status = status(pool, limit).await?;
    if status.exceeded() {
        tracing::info!(
            spent = %status.spent,
            limit = %status.limit,
            until = %status.next_reset,
            "daily AI budget reached, job postponed"
        );
        Ok(Some(JobOutcome::RetryAt(status.next_reset)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    async fn spend(pool: &PgPool, at: DateTime<Utc>, micro: i64) {
        sqlx::query(
            "INSERT INTO ai_calls (provider, purpose, model, cost_micro_eur, latency_ms, status,
                                   created_at)
             VALUES ('mistral', 'analysis_structuring', 'm', $1, 10, 'success', $2)",
        )
        .bind(micro)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn sums_the_paris_day(pool: PgPool) {
        // Summer time (UTC+2): the Paris day of 2026-06-15 is [06-14 22:00Z, 06-15 22:00Z).
        spend(&pool, utc(2026, 6, 14, 21, 59), 1_000_000).await;
        spend(&pool, utc(2026, 6, 14, 22, 0), 2_000_000).await;
        spend(&pool, utc(2026, 6, 15, 21, 59), 500_000).await;
        spend(&pool, utc(2026, 6, 15, 22, 0), 7_000_000).await;

        let status = status_at(&pool, MicroEur::from_eur(5), utc(2026, 6, 15, 12, 0))
            .await
            .unwrap();
        assert_eq!(status.day_start, utc(2026, 6, 14, 22, 0));
        assert_eq!(status.next_reset, utc(2026, 6, 15, 22, 0));
        assert_eq!(status.spent, MicroEur(2_500_000));
        assert!(!status.exceeded());
        assert_eq!(status.remaining(), MicroEur(2_500_000));

        // 23:30 Paris time belongs to the same day, 00:30 to the next one.
        let late = status_at(&pool, MicroEur::from_eur(5), utc(2026, 6, 15, 21, 30))
            .await
            .unwrap();
        assert_eq!(late.spent, MicroEur(2_500_000));
        let next = status_at(&pool, MicroEur::from_eur(5), utc(2026, 6, 15, 22, 30))
            .await
            .unwrap();
        assert_eq!(next.spent, MicroEur(7_000_000));
        assert!(next.exceeded());
        assert_eq!(next.remaining(), MicroEur::ZERO);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn handles_daylight_saving_changes(pool: PgPool) {
        // 2026-10-25: back to winter time, a 25-hour day starting at 10-24 22:00Z.
        let status = status_at(&pool, MicroEur::from_eur(1), utc(2026, 10, 25, 12, 0))
            .await
            .unwrap();
        assert_eq!(status.day_start, utc(2026, 10, 24, 22, 0));
        assert_eq!(status.next_reset, utc(2026, 10, 25, 23, 0));
        // 2026-03-29: summer time, a 23-hour day.
        let status = status_at(&pool, MicroEur::from_eur(1), utc(2026, 3, 29, 12, 0))
            .await
            .unwrap();
        assert_eq!(status.day_start, utc(2026, 3, 28, 23, 0));
        assert_eq!(status.next_reset, utc(2026, 3, 29, 22, 0));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn postpones_to_the_next_paris_midnight(pool: PgPool) {
        assert_eq!(
            postpone_if_exceeded(&pool, MicroEur::from_eur(1))
                .await
                .unwrap(),
            None
        );
        spend(&pool, Utc::now(), 1_000_000).await;
        let outcome = postpone_if_exceeded(&pool, MicroEur::from_eur(1))
            .await
            .unwrap();
        let expected = status(&pool, MicroEur::from_eur(1))
            .await
            .unwrap()
            .next_reset;
        assert_eq!(outcome, Some(JobOutcome::RetryAt(expected)));
        assert!(expected > Utc::now());
        // A zero budget stops every call.
        assert!(
            postpone_if_exceeded(&pool, MicroEur::ZERO)
                .await
                .unwrap()
                .is_some()
        );
    }
}
