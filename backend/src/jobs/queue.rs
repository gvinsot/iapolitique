//! SQL operations of the job queue.
//!
//! Lifecycle: `queued` ─claim─▶ `running` ─▶ `succeeded` | `queued` (retry with backoff,
//! or rescheduled without consuming an attempt) | `dead` (permanent error or attempts
//! exhausted). A `running` job whose lock is not refreshed (worker crash) is put back in the
//! queue by [`reclaim_stale`].

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor, PgPool};

use super::{Job, JobKind, JobStatus, NewJob};

/// First retry delay; doubled at each attempt.
pub const BACKOFF_BASE: Duration = Duration::from_secs(30);
/// Maximum retry delay.
pub const BACKOFF_MAX: Duration = Duration::from_secs(3600);
/// `last_error` is truncated to this many characters.
pub const MAX_ERROR_LEN: usize = 2000;

/// Delay before the next try after the `attempt`-th failure (1-based): 30 s, 1 min, 2 min…
/// capped at one hour.
pub fn backoff(attempt: i32) -> Duration {
    let exponent = u32::try_from(attempt.saturating_sub(1))
        .unwrap_or(0)
        .min(20);
    BACKOFF_BASE
        .saturating_mul(2u32.saturating_pow(exponent))
        .min(BACKOFF_MAX)
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn truncate(message: &str) -> String {
    match message.char_indices().nth(MAX_ERROR_LEN) {
        Some((pos, _)) => format!("{}…", &message[..pos]),
        None => message.to_owned(),
    }
}

/// Enqueues a job. Returns `None` when a queued or running job already has the same
/// `dedupe_key`. Accepts a transaction so that a job is enqueued atomically with the state
/// change that requires it.
pub async fn enqueue<'e>(
    executor: impl PgExecutor<'e>,
    job: &NewJob,
) -> Result<Option<i64>, sqlx::Error> {
    let id: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO jobs (kind, payload, priority, max_attempts, run_after, dedupe_key)
         VALUES ($1, $2, $3, $4, coalesce($5, now()), $6)
         ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(job.kind.as_str())
    .bind(&job.payload)
    .bind(job.priority)
    .bind(job.max_attempts)
    .bind(job.run_after)
    .bind(job.dedupe_key.as_deref())
    .fetch_optional(executor)
    .await?;
    Ok(id.map(|(id,)| id))
}

/// Removes the queued job with this dedupe key; a running job is left alone (its handler sees
/// that there is nothing left to do). Returns whether a job was removed.
pub async fn cancel_queued<'e>(
    executor: impl PgExecutor<'e>,
    dedupe_key: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM jobs WHERE dedupe_key = $1 AND status = 'queued'")
        .bind(dedupe_key)
        .execute(executor)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Makes the queued job with this dedupe key due at `at` at the latest. Returns whether a
/// queued job exists.
pub async fn bring_forward<'e>(
    executor: impl PgExecutor<'e>,
    dedupe_key: &str,
    at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE jobs SET run_after = least(run_after, $2)
         WHERE dedupe_key = $1 AND status = 'queued'",
    )
    .bind(dedupe_key)
    .bind(at)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Makes due now the queued jobs of `kinds` scheduled within `within` from now (jobs that wait
/// for a short delay, such as a refused API key: 15 min to 1 h). Returns how many.
pub async fn bring_forward_kinds<'e>(
    executor: impl PgExecutor<'e>,
    kinds: &[JobKind],
    within: Duration,
) -> Result<u64, sqlx::Error> {
    let kinds: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
    let result = sqlx::query(
        "UPDATE jobs SET run_after = now()
         WHERE status = 'queued' AND kind = ANY($1) AND run_after > now()
           AND run_after <= now() + $2 * interval '1 millisecond'",
    )
    .bind(&kinds)
    .bind(millis(within))
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// Claims the most urgent due job among `kinds`: highest priority, then oldest `run_after`.
/// Concurrent workers never get the same job (`FOR UPDATE SKIP LOCKED`).
pub async fn claim(
    pool: &PgPool,
    worker_id: &str,
    kinds: &[JobKind],
) -> Result<Option<Job>, sqlx::Error> {
    if kinds.is_empty() {
        return Ok(None);
    }
    let kinds: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
    sqlx::query_as(
        "UPDATE jobs SET status = 'running', attempts = attempts + 1, locked_at = now(),
                locked_by = $1
         WHERE id = (
             SELECT id FROM jobs
             WHERE status = 'queued' AND run_after <= now() AND kind = ANY($2)
             ORDER BY priority DESC, run_after, id
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING id, kind, payload, priority, attempts, max_attempts, dedupe_key, created_at",
    )
    .bind(worker_id)
    .bind(&kinds)
    .fetch_optional(pool)
    .await
}

/// Marks a running job as succeeded. Returns `false` if the job is no longer locked by
/// `worker_id` (its lock expired and another worker took it).
pub async fn complete(pool: &PgPool, id: i64, worker_id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE jobs SET status = 'succeeded', locked_at = NULL, locked_by = NULL,
                finished_at = now(), last_error = NULL
         WHERE id = $1 AND status = 'running' AND locked_by = $2",
    )
    .bind(id)
    .bind(worker_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// What happened to a failed job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOutcome {
    /// Back in the queue, due at the given time.
    Retrying(DateTime<Utc>),
    Dead,
    /// The job was not locked by this worker any more: nothing changed.
    LockLost,
}

/// Records a failure of the `attempts`-th try. Transient failures are retried after
/// [`backoff`] until `max_attempts` is reached; permanent ones die immediately.
pub async fn fail(
    pool: &PgPool,
    job: &Job,
    worker_id: &str,
    error: &str,
    permanent: bool,
) -> Result<FailOutcome, sqlx::Error> {
    let row: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "UPDATE jobs SET
            status = CASE WHEN $3 OR attempts >= max_attempts THEN 'dead' ELSE 'queued' END,
            run_after = CASE WHEN $3 OR attempts >= max_attempts THEN run_after
                             ELSE now() + $4 * interval '1 millisecond' END,
            finished_at = CASE WHEN $3 OR attempts >= max_attempts THEN now() ELSE NULL END,
            locked_at = NULL,
            locked_by = NULL,
            last_error = $5
         WHERE id = $1 AND status = 'running' AND locked_by = $2
         RETURNING status, run_after",
    )
    .bind(job.id)
    .bind(worker_id)
    .bind(permanent)
    .bind(millis(backoff(job.attempts)))
    .bind(truncate(error))
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        None => FailOutcome::LockLost,
        Some((status, _)) if status == JobStatus::Dead.as_str() => FailOutcome::Dead,
        Some((_, run_after)) => FailOutcome::Retrying(run_after),
    })
}

/// Puts a running job back in the queue for `run_after` without consuming an attempt.
pub async fn reschedule(
    pool: &PgPool,
    id: i64,
    worker_id: &str,
    run_after: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE jobs SET status = 'queued', attempts = greatest(attempts - 1, 0),
                run_after = $3, locked_at = NULL, locked_by = NULL
         WHERE id = $1 AND status = 'running' AND locked_by = $2",
    )
    .bind(id)
    .bind(worker_id)
    .bind(run_after)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Refreshes the lock of a running job. Returns `false` if the lock was lost.
pub async fn heartbeat(pool: &PgPool, id: i64, worker_id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE jobs SET locked_at = now()
         WHERE id = $1 AND status = 'running' AND locked_by = $2",
    )
    .bind(id)
    .bind(worker_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Result of [`reclaim_stale`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reclaimed {
    pub requeued: u64,
    pub dead: u64,
}

/// Requeues running jobs whose lock is older than `lock_timeout` (the worker died or hung).
/// The interrupted try counts as an attempt: a job that keeps killing its worker ends dead.
pub async fn reclaim_stale(
    pool: &PgPool,
    lock_timeout: Duration,
) -> Result<Reclaimed, sqlx::Error> {
    let statuses: Vec<(String,)> = sqlx::query_as(
        "UPDATE jobs SET
            status = CASE WHEN attempts >= max_attempts THEN 'dead' ELSE 'queued' END,
            finished_at = CASE WHEN attempts >= max_attempts THEN now() ELSE NULL END,
            run_after = now(),
            locked_at = NULL,
            locked_by = NULL,
            last_error = 'lock expired: the worker stopped or hung'
         WHERE status = 'running' AND locked_at < now() - $1 * interval '1 millisecond'
         RETURNING status",
    )
    .bind(millis(lock_timeout))
    .fetch_all(pool)
    .await?;
    let dead = statuses
        .iter()
        .filter(|(s,)| s == JobStatus::Dead.as_str())
        .count() as u64;
    Ok(Reclaimed {
        requeued: statuses.len() as u64 - dead,
        dead,
    })
}

/// Number of jobs per kind and status (admin dashboard).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct QueueCount {
    pub kind: String,
    pub status: String,
    pub count: i64,
}

pub async fn counts(pool: &PgPool) -> Result<Vec<QueueCount>, sqlx::Error> {
    sqlx::query_as(
        "SELECT kind, status, count(*) AS count FROM jobs GROUP BY kind, status ORDER BY kind, status",
    )
    .fetch_all(pool)
    .await
}

/// A job as listed by the admin.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub priority: i16,
    pub attempts: i32,
    pub max_attempts: i32,
    pub payload: serde_json::Value,
    pub dedupe_key: Option<String>,
    pub run_after: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Jobs of a status and/or kind, most recent first, with their total.
pub async fn list(
    pool: &PgPool,
    status: Option<JobStatus>,
    kind: Option<JobKind>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<JobRow>, i64), sqlx::Error> {
    let status = status.map(JobStatus::as_str);
    let kind = kind.map(JobKind::as_str);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs
         WHERE ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR kind = $2)",
    )
    .bind(status)
    .bind(kind)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query_as(const_format_jobs::LIST)
        .bind(status)
        .bind(kind)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    Ok((rows, total))
}

/// One job, `None` when it does not exist.
pub async fn get<'e>(
    executor: impl PgExecutor<'e>,
    id: i64,
) -> Result<Option<JobRow>, sqlx::Error> {
    sqlx::query_as(const_format_jobs::GET)
        .bind(id)
        .fetch_optional(executor)
        .await
}

/// Why a dead job could not be queued again.
#[derive(Debug, thiserror::Error)]
pub enum RetryError {
    #[error("job not found")]
    NotFound,
    /// Only `dead` jobs are retried by hand.
    #[error("job is not dead")]
    NotDead,
    /// Another job with the same dedupe key is queued or running.
    #[error("an equivalent job is already queued")]
    Duplicate,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Queues a dead job again, due now, with its attempts reset (its last error is kept). On
/// [`RetryError::Duplicate`] the transaction is aborted: the caller rolls it back.
pub async fn retry_dead(conn: &mut PgConnection, id: i64) -> Result<JobRow, RetryError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    match status.as_deref() {
        None => return Err(RetryError::NotFound),
        Some(status) if status != JobStatus::Dead.as_str() => return Err(RetryError::NotDead),
        Some(_) => {}
    }
    match sqlx::query_as(const_format_jobs::RETRY)
        .bind(id)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(row) => Ok(row),
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => Err(RetryError::Duplicate),
        Err(err) => Err(err.into()),
    }
}

/// Constant SQL of the admin operations (built at compile time, §14.1).
mod const_format_jobs {
    macro_rules! columns {
        () => {
            "id, kind, status, priority, attempts, max_attempts, payload, dedupe_key, run_after,
             last_error, created_at, updated_at, finished_at"
        };
    }
    pub const LIST: &str = concat!(
        "SELECT ",
        columns!(),
        " FROM jobs WHERE ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR kind = $2)
          ORDER BY id DESC LIMIT $3 OFFSET $4"
    );
    pub const GET: &str = concat!("SELECT ", columns!(), " FROM jobs WHERE id = $1");
    pub const RETRY: &str = concat!(
        "UPDATE jobs SET status = 'queued', attempts = 0, run_after = now(), finished_at = NULL,
             locked_at = NULL, locked_by = NULL
         WHERE id = $1 AND status = 'dead' RETURNING ",
        columns!()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::priority;
    use serde_json::json;

    #[test]
    fn backoff_doubles_and_is_capped() {
        assert_eq!(backoff(1), Duration::from_secs(30));
        assert_eq!(backoff(2), Duration::from_secs(60));
        assert_eq!(backoff(3), Duration::from_secs(120));
        assert_eq!(backoff(7), Duration::from_secs(1920));
        assert_eq!(backoff(8), BACKOFF_MAX);
        assert_eq!(backoff(1_000), BACKOFF_MAX);
        assert_eq!(backoff(0), BACKOFF_BASE);
        assert_eq!(backoff(-3), BACKOFF_BASE);
    }

    #[test]
    fn errors_are_truncated_on_char_boundaries() {
        let long = "é".repeat(MAX_ERROR_LEN + 10);
        let truncated = truncate(&long);
        assert_eq!(truncated.chars().count(), MAX_ERROR_LEN + 1);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncate("short"), "short");
    }

    async fn status_of(pool: &PgPool, id: i64) -> (String, i32, Option<String>) {
        sqlx::query_as("SELECT status, attempts, last_error FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    const ALL: &[JobKind] = JobKind::ALL;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn claims_by_priority_then_run_after(pool: PgPool) {
        let low = enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap()
            .unwrap();
        let high = enqueue(
            &pool,
            &NewJob::new(JobKind::SecondOpinion)
                .run_after(Utc::now() - chrono::Duration::minutes(1)),
        )
        .await
        .unwrap()
        .unwrap();
        let older_mid = enqueue(
            &pool,
            &NewJob::new(JobKind::AnalyzeIdea).run_after(Utc::now() - chrono::Duration::minutes(5)),
        )
        .await
        .unwrap()
        .unwrap();
        let newer_mid = enqueue(&pool, &NewJob::new(JobKind::AnalyzeIdea))
            .await
            .unwrap()
            .unwrap();
        let _future = enqueue(
            &pool,
            &NewJob::new(JobKind::SecondOpinion).run_after(Utc::now() + chrono::Duration::hours(1)),
        )
        .await
        .unwrap()
        .unwrap();

        let mut order = Vec::new();
        while let Some(job) = claim(&pool, "w1", ALL).await.unwrap() {
            assert_eq!(job.attempts, 1);
            order.push(job.id);
        }
        assert_eq!(order, vec![high, older_mid, newer_mid, low]);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn claims_only_requested_kinds(pool: PgPool) {
        enqueue(&pool, &NewJob::new(JobKind::AnalyzeIdea))
            .await
            .unwrap();
        assert!(
            claim(&pool, "w", &[JobKind::Purge])
                .await
                .unwrap()
                .is_none()
        );
        assert!(claim(&pool, "w", &[]).await.unwrap().is_none());
        let job = claim(&pool, "w", &[JobKind::AnalyzeIdea])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.kind, JobKind::AnalyzeIdea);
        assert_eq!(job.priority, priority::NEW_IDEA);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn concurrent_claims_never_share_a_job(pool: PgPool) {
        for _ in 0..20 {
            enqueue(&pool, &NewJob::new(JobKind::Purge)).await.unwrap();
        }
        let mut tasks = Vec::new();
        for w in 0..5 {
            let pool = pool.clone();
            tasks.push(tokio::spawn(async move {
                let mut ids = Vec::new();
                while let Some(job) = claim(&pool, &format!("w{w}"), ALL).await.unwrap() {
                    ids.push(job.id);
                }
                ids
            }));
        }
        let mut all = Vec::new();
        for task in tasks {
            all.extend(task.await.unwrap());
        }
        all.sort_unstable();
        let before = all.len();
        all.dedup();
        assert_eq!(before, 20);
        assert_eq!(all.len(), 20);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn dedupe_key_allows_one_active_job(pool: PgPool) {
        let job = NewJob::new(JobKind::AnalyzeIdea).dedupe_key("analyze:1");
        let first = enqueue(&pool, &job).await.unwrap();
        assert!(first.is_some());
        assert_eq!(enqueue(&pool, &job).await.unwrap(), None);

        // Still deduplicated while running, allowed again once finished.
        let claimed = claim(&pool, "w", ALL).await.unwrap().unwrap();
        assert_eq!(enqueue(&pool, &job).await.unwrap(), None);
        assert!(complete(&pool, claimed.id, "w").await.unwrap());
        assert!(enqueue(&pool, &job).await.unwrap().is_some());

        // Jobs without key are never deduplicated.
        let plain = NewJob::new(JobKind::Purge);
        assert!(enqueue(&pool, &plain).await.unwrap().is_some());
        assert!(enqueue(&pool, &plain).await.unwrap().is_some());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn enqueue_inside_a_rolled_back_transaction_leaves_nothing(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        enqueue(&mut *tx, &NewJob::new(JobKind::Purge))
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        assert!(claim(&pool, "w", ALL).await.unwrap().is_none());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn transient_failures_back_off_then_die(pool: PgPool) {
        let id = enqueue(
            &pool,
            &NewJob::new(JobKind::AnalyzeIdea)
                .payload(json!({ "idea_id": 1 }))
                .max_attempts(2),
        )
        .await
        .unwrap()
        .unwrap();

        let job = claim(&pool, "w", ALL).await.unwrap().unwrap();
        let before = Utc::now();
        let outcome = fail(&pool, &job, "w", "timeout", false).await.unwrap();
        let FailOutcome::Retrying(run_after) = outcome else {
            panic!("expected a retry, got {outcome:?}");
        };
        assert!(run_after >= before + chrono::Duration::seconds(29));
        assert!(run_after <= Utc::now() + chrono::Duration::seconds(31));
        assert_eq!(
            status_of(&pool, id).await,
            ("queued".into(), 1, Some("timeout".into()))
        );
        // Not due yet.
        assert!(claim(&pool, "w", ALL).await.unwrap().is_none());

        sqlx::query("UPDATE jobs SET run_after = now() WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let job = claim(&pool, "w", ALL).await.unwrap().unwrap();
        assert_eq!(job.attempts, 2);
        assert_eq!(
            fail(&pool, &job, "w", "again", false).await.unwrap(),
            FailOutcome::Dead
        );
        assert_eq!(
            status_of(&pool, id).await,
            ("dead".into(), 2, Some("again".into()))
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn permanent_failures_die_immediately(pool: PgPool) {
        let id = enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap()
            .unwrap();
        let job = claim(&pool, "w", ALL).await.unwrap().unwrap();
        assert_eq!(
            fail(&pool, &job, "w", "bad payload", true).await.unwrap(),
            FailOutcome::Dead
        );
        assert_eq!(status_of(&pool, id).await.0, "dead");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn operations_require_the_lock(pool: PgPool) {
        enqueue(&pool, &NewJob::new(JobKind::Purge)).await.unwrap();
        let job = claim(&pool, "owner", ALL).await.unwrap().unwrap();
        assert!(!complete(&pool, job.id, "intruder").await.unwrap());
        assert!(!heartbeat(&pool, job.id, "intruder").await.unwrap());
        assert!(
            !reschedule(&pool, job.id, "intruder", Utc::now())
                .await
                .unwrap()
        );
        assert_eq!(
            fail(&pool, &job, "intruder", "x", false).await.unwrap(),
            FailOutcome::LockLost
        );
        assert!(heartbeat(&pool, job.id, "owner").await.unwrap());
        assert!(complete(&pool, job.id, "owner").await.unwrap());
        assert!(!complete(&pool, job.id, "owner").await.unwrap());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reschedule_does_not_consume_an_attempt(pool: PgPool) {
        let id = enqueue(&pool, &NewJob::new(JobKind::AnalyzeIdea))
            .await
            .unwrap()
            .unwrap();
        let job = claim(&pool, "w", ALL).await.unwrap().unwrap();
        let later = Utc::now() + chrono::Duration::hours(3);
        assert!(reschedule(&pool, job.id, "w", later).await.unwrap());
        assert_eq!(status_of(&pool, id).await, ("queued".into(), 0, None));
        assert!(claim(&pool, "w", ALL).await.unwrap().is_none());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn stale_locks_are_reclaimed(pool: PgPool) {
        let alive = enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap()
            .unwrap();
        let stale = enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap()
            .unwrap();
        let exhausted = enqueue(&pool, &NewJob::new(JobKind::Purge).max_attempts(1))
            .await
            .unwrap()
            .unwrap();
        for _ in 0..3 {
            claim(&pool, "w", ALL).await.unwrap().unwrap();
        }
        sqlx::query("UPDATE jobs SET locked_at = now() - interval '10 minutes' WHERE id = ANY($1)")
            .bind(vec![stale, exhausted])
            .execute(&pool)
            .await
            .unwrap();

        let reclaimed = reclaim_stale(&pool, Duration::from_secs(300))
            .await
            .unwrap();
        assert_eq!(
            reclaimed,
            Reclaimed {
                requeued: 1,
                dead: 1
            }
        );
        assert_eq!(status_of(&pool, alive).await.0, "running");
        assert_eq!(status_of(&pool, stale).await.0, "queued");
        assert_eq!(status_of(&pool, exhausted).await.0, "dead");

        let again = claim(&pool, "w2", ALL).await.unwrap().unwrap();
        assert_eq!(again.id, stale);
        assert_eq!(again.attempts, 2);

        let counts = counts(&pool).await.unwrap();
        let total: i64 = counts.iter().map(|c| c.count).sum();
        assert_eq!(total, 3);
    }
}
