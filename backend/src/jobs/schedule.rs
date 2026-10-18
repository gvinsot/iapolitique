//! Daily recurring jobs.
//!
//! Each schedule has a row in `job_schedules`. Every worker periodically calls
//! [`enqueue_due`]: the one that locks a due row enqueues the job and moves `next_run_at` to the
//! next occurrence, so a run happens once whatever the number of replicas. Times are local to
//! Europe/Paris (DST handled by PostgreSQL).

use sqlx::PgPool;

use super::queue::enqueue;
use super::{JobKind, NewJob};

/// A job enqueued every day at a fixed Europe/Paris time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailySchedule {
    /// Unique name, also used as dedupe key of the enqueued jobs.
    pub name: &'static str,
    pub kind: JobKind,
    pub hour: u8,
    pub minute: u8,
}

impl DailySchedule {
    pub const fn new(name: &'static str, kind: JobKind, hour: u8, minute: u8) -> Self {
        assert!(hour < 24 && minute < 60);
        Self {
            name,
            kind,
            hour,
            minute,
        }
    }
}

/// SQL expression of the next occurrence (strictly after now) of `$hour:$minute` Paris time.
const NEXT_RUN_SQL: &str =
    "(SELECT CASE WHEN today_run > now() THEN today_run ELSE tomorrow_run END
    FROM (SELECT
        (((now() AT TIME ZONE 'Europe/Paris')::date + make_time($2, $3, 0))
            AT TIME ZONE 'Europe/Paris') AS today_run,
        ((((now() AT TIME ZONE 'Europe/Paris')::date + 1) + make_time($2, $3, 0))
            AT TIME ZONE 'Europe/Paris') AS tomorrow_run) AS occurrences)";

/// Creates the missing schedule rows (existing ones keep their `next_run_at`).
pub async fn ensure(pool: &PgPool, schedules: &[DailySchedule]) -> Result<(), sqlx::Error> {
    let sql = format!(
        "INSERT INTO job_schedules (name, next_run_at) VALUES ($1, {NEXT_RUN_SQL})
         ON CONFLICT (name) DO NOTHING"
    );
    for schedule in schedules {
        sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
            .bind(schedule.name)
            .bind(i32::from(schedule.hour))
            .bind(i32::from(schedule.minute))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Enqueues the jobs of the due schedules and returns their ids.
pub async fn enqueue_due(
    pool: &PgPool,
    schedules: &[DailySchedule],
) -> Result<Vec<i64>, sqlx::Error> {
    let update = format!(
        "UPDATE job_schedules SET next_run_at = {NEXT_RUN_SQL}, last_enqueued_at = now(),
                last_job_id = coalesce($4, last_job_id)
         WHERE name = $1"
    );
    let mut enqueued = Vec::new();
    for schedule in schedules {
        let mut tx = pool.begin().await?;
        let due: Option<(String,)> = sqlx::query_as(
            "SELECT name FROM job_schedules
             WHERE name = $1 AND next_run_at <= now()
             FOR UPDATE SKIP LOCKED",
        )
        .bind(schedule.name)
        .fetch_optional(&mut *tx)
        .await?;
        if due.is_none() {
            continue;
        }
        let job_id = enqueue(
            &mut *tx,
            &NewJob::new(schedule.kind).dedupe_key(format!("schedule:{}", schedule.name)),
        )
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(update.clone()))
            .bind(schedule.name)
            .bind(i32::from(schedule.hour))
            .bind(i32::from(schedule.minute))
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        enqueued.extend(job_id);
    }
    Ok(enqueued)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    const PURGE: DailySchedule = DailySchedule::new("daily_purge", JobKind::Purge, 3, 30);

    async fn next_run(pool: &PgPool) -> DateTime<Utc> {
        let (at,): (DateTime<Utc>,) =
            sqlx::query_as("SELECT next_run_at FROM job_schedules WHERE name = 'daily_purge'")
                .fetch_one(pool)
                .await
                .unwrap();
        at
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn first_run_is_the_next_paris_occurrence(pool: PgPool) {
        ensure(&pool, &[PURGE]).await.unwrap();
        let at = next_run(&pool).await;
        let now = Utc::now();
        assert!(at > now && at <= now + chrono::Duration::hours(25), "{at}");
        let (local,): (String,) = sqlx::query_as(
            "SELECT to_char(next_run_at AT TIME ZONE 'Europe/Paris', 'HH24:MI') FROM job_schedules",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(local, "03:30");

        // ensure() keeps the existing row.
        sqlx::query("UPDATE job_schedules SET next_run_at = now() - interval '1 hour'")
            .execute(&pool)
            .await
            .unwrap();
        ensure(&pool, &[PURGE]).await.unwrap();
        assert!(next_run(&pool).await < Utc::now());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn due_schedules_are_enqueued_once(pool: PgPool) {
        ensure(&pool, &[PURGE]).await.unwrap();
        assert!(enqueue_due(&pool, &[PURGE]).await.unwrap().is_empty());

        sqlx::query("UPDATE job_schedules SET next_run_at = now() - interval '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        let ids = enqueue_due(&pool, &[PURGE]).await.unwrap();
        assert_eq!(ids.len(), 1);
        assert!(enqueue_due(&pool, &[PURGE]).await.unwrap().is_empty());
        assert!(next_run(&pool).await > Utc::now());

        let (kind, last_job): (String, Option<i64>) = sqlx::query_as(
            "SELECT j.kind, s.last_job_id FROM job_schedules s JOIN jobs j ON j.id = s.last_job_id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(kind, "purge");
        assert_eq!(last_job, Some(ids[0]));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn a_still_queued_run_is_not_duplicated(pool: PgPool) {
        ensure(&pool, &[PURGE]).await.unwrap();
        for _ in 0..2 {
            sqlx::query("UPDATE job_schedules SET next_run_at = now() - interval '1 minute'")
                .execute(&pool)
                .await
                .unwrap();
            enqueue_due(&pool, &[PURGE]).await.unwrap();
        }
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}
