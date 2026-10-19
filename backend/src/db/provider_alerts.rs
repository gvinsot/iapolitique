//! Configuration alerts of the AI providers (`provider_alerts`, AGENTS.md §6.4, §13.1).
//!
//! - [`MISTRAL_AUTH`]: Mistral refused the API key (HTTP 401 or 403). It is a configuration
//!   error, not a failure of the job: the jobs that need Mistral wait (no attempt consumed), one
//!   of them calls Mistral again per period to see whether the key was fixed, the first success
//!   deletes the alert, and the admin dashboard shows it meanwhile.
//! - [`MISTRAL_REASONING_UNSUPPORTED`] (table `mistral_reasoning_refusals`, §6.1): a model
//!   refused `reasoning_effort`; its calls go without it. Nothing waits: the dashboard advises to
//!   pick a model that takes the parameter, and the methodology page publishes the effort really
//!   sent.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

/// The Mistral API key is refused.
pub const MISTRAL_AUTH: &str = "mistral_auth";
/// A configured Mistral model refuses `reasoning_effort` (dashboard code, §6.1).
pub const MISTRAL_REASONING_UNSUPPORTED: &str = "mistral_reasoning_unsupported";

/// Shortest delay between two calls that check whether a refused key works again.
pub const AUTH_RETRY_MIN: Duration = Duration::minutes(15);
/// Longest such delay (the delay grows with the age of the alert).
pub const AUTH_RETRY_MAX: Duration = Duration::hours(1);
/// One ERROR log per alert and per period, never one per job.
pub const AUTH_LOG_PERIOD: Duration = Duration::hours(1);

/// Delay before the next check of a refused key: a quarter of the age of the alert, between
/// [`AUTH_RETRY_MIN`] and [`AUTH_RETRY_MAX`].
pub fn retry_delay(age: Duration) -> Duration {
    (age / 4).clamp(AUTH_RETRY_MIN, AUTH_RETRY_MAX)
}

/// An alert in progress.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ProviderAlert {
    pub alert: String,
    pub since: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_http_status: Option<i32>,
    pub failures: i64,
    pub probed_at: DateTime<Utc>,
    /// `since` in Europe/Paris, `DD/MM/YYYY HH24:MI` (French message of the dashboard).
    pub since_paris: String,
}

/// Columns of [`ProviderAlert`] (a literal, for `concat!`: queries are never built at run time).
macro_rules! alert_columns {
    () => {
        "alert, since, last_seen_at, last_http_status, failures, probed_at,
         to_char(since AT TIME ZONE 'Europe/Paris', 'DD/MM/YYYY à HH24:MI') AS since_paris"
    };
}

/// Records a refusal. Returns whether it should be logged now (first refusal, or the last log
/// is older than [`AUTH_LOG_PERIOD`]).
pub async fn record_failure(pool: &PgPool, alert: &str, http_status: u16) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "INSERT INTO provider_alerts AS a (alert, last_http_status) VALUES ($1, $2)
         ON CONFLICT (alert) DO UPDATE SET
             last_seen_at = now(), last_http_status = EXCLUDED.last_http_status,
             failures = a.failures + 1, probed_at = now(),
             logged_at = CASE WHEN a.logged_at <= now() - make_interval(secs => $3)
                              THEN now() ELSE a.logged_at END
         RETURNING logged_at = now()",
    )
    .bind(alert)
    .bind(i32::from(http_status))
    .bind(AUTH_LOG_PERIOD.num_seconds() as f64)
    .fetch_one(pool)
    .await
}

/// Ends the alert (a call succeeded). Returns whether there was one.
pub async fn clear(pool: &PgPool, alert: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM provider_alerts WHERE alert = $1")
        .bind(alert)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// The alert in progress, if any.
pub async fn get(pool: &PgPool, alert: &str) -> sqlx::Result<Option<ProviderAlert>> {
    sqlx::query_as(concat!(
        "SELECT ",
        alert_columns!(),
        " FROM provider_alerts WHERE alert = $1"
    ))
    .bind(alert)
    .fetch_optional(pool)
    .await
}

/// Every alert in progress, oldest first (admin dashboard).
pub async fn all(pool: &PgPool) -> sqlx::Result<Vec<ProviderAlert>> {
    sqlx::query_as(concat!(
        "SELECT ",
        alert_columns!(),
        " FROM provider_alerts ORDER BY since, alert"
    ))
    .fetch_all(pool)
    .await
}

/// Whether a job may call the provider now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// No alert: call.
    Clear,
    /// Alert in progress, and this job checks it now (the others wait).
    Mine,
    /// Alert in progress, checked recently: run again at this time, without calling.
    WaitUntil(DateTime<Utc>),
}

/// Lets one job per period call the provider while the alert lasts.
pub async fn claim_probe(pool: &PgPool, alert: &str) -> sqlx::Result<Probe> {
    let mut tx = pool.begin().await?;
    let row: Option<(DateTime<Utc>, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT since, probed_at, now() FROM provider_alerts WHERE alert = $1 FOR UPDATE",
    )
    .bind(alert)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((since, probed_at, now)) = row else {
        return Ok(Probe::Clear);
    };
    let next = probed_at + retry_delay(now - since);
    if next > now {
        return Ok(Probe::WaitUntil(next));
    }
    sqlx::query("UPDATE provider_alerts SET probed_at = now() WHERE alert = $1")
        .bind(alert)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Probe::Mine)
}

/// When a job refused by the provider runs again: [`retry_delay`] after now.
pub async fn retry_at(pool: &PgPool, alert: &str) -> sqlx::Result<DateTime<Utc>> {
    let (since, now): (Option<DateTime<Utc>>, DateTime<Utc>) =
        sqlx::query_as("SELECT (SELECT since FROM provider_alerts WHERE alert = $1), now()")
            .bind(alert)
            .fetch_one(pool)
            .await?;
    Ok(now + retry_delay(now - since.unwrap_or(now)))
}

/// A Mistral model that refused `reasoning_effort` (`mistral_reasoning_refusals`).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ReasoningRefusal {
    pub model: String,
    pub since: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_http_status: i32,
    pub refusals: i64,
    /// `since` in Europe/Paris, `DD/MM/YYYY à HH24:MI`.
    pub since_paris: String,
}

/// Records that `model` refused `reasoning_effort` (and accepted the same call without it).
pub async fn record_reasoning_refusal(
    pool: &PgPool,
    model: &str,
    http_status: u16,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO mistral_reasoning_refusals AS r (model, last_http_status) VALUES ($1, $2)
         ON CONFLICT (model) DO UPDATE SET
             last_seen_at = now(), last_http_status = EXCLUDED.last_http_status,
             refusals = r.refusals + 1",
    )
    .bind(model)
    .bind(i32::from(http_status))
    .execute(pool)
    .await
    .map(|_| ())
}

/// `model` accepted `reasoning_effort`: forgets a refusal. Returns whether there was one.
pub async fn clear_reasoning_refusal(pool: &PgPool, model: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM mistral_reasoning_refusals WHERE model = $1")
        .bind(model)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Every model known to refuse `reasoning_effort`, oldest refusal first.
pub async fn reasoning_refusals(pool: &PgPool) -> sqlx::Result<Vec<ReasoningRefusal>> {
    sqlx::query_as(
        "SELECT model, since, last_seen_at, last_http_status, refusals,
                to_char(since AT TIME ZONE 'Europe/Paris', 'DD/MM/YYYY à HH24:MI') AS since_paris
         FROM mistral_reasoning_refusals ORDER BY since, model",
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delay_grows_with_the_alert_and_is_capped() {
        assert_eq!(retry_delay(Duration::zero()), AUTH_RETRY_MIN);
        assert_eq!(retry_delay(Duration::hours(1)), AUTH_RETRY_MIN);
        assert_eq!(retry_delay(Duration::hours(2)), Duration::minutes(30));
        assert_eq!(retry_delay(Duration::hours(4)), AUTH_RETRY_MAX);
        assert_eq!(retry_delay(Duration::days(30)), AUTH_RETRY_MAX);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn records_logs_once_per_period_probes_and_clears(pool: PgPool) {
        assert_eq!(
            claim_probe(&pool, MISTRAL_AUTH).await.unwrap(),
            Probe::Clear
        );
        assert!(get(&pool, MISTRAL_AUTH).await.unwrap().is_none());

        assert!(record_failure(&pool, MISTRAL_AUTH, 401).await.unwrap());
        // Same period: counted, not logged again.
        assert!(!record_failure(&pool, MISTRAL_AUTH, 403).await.unwrap());
        let alert = get(&pool, MISTRAL_AUTH).await.unwrap().unwrap();
        assert_eq!((alert.failures, alert.last_http_status), (2, Some(403)));
        assert!(alert.since_paris.contains(" à "));

        // Just refused: the other jobs wait until the next check.
        match claim_probe(&pool, MISTRAL_AUTH).await.unwrap() {
            Probe::WaitUntil(at) => assert!(at >= alert.probed_at + AUTH_RETRY_MIN),
            other => panic!("unexpected {other:?}"),
        }
        // A period later, one job checks and the others still wait.
        sqlx::query(
            "UPDATE provider_alerts SET probed_at = now() - interval '16 minutes',
                 logged_at = now() - interval '61 minutes'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(claim_probe(&pool, MISTRAL_AUTH).await.unwrap(), Probe::Mine);
        assert!(matches!(
            claim_probe(&pool, MISTRAL_AUTH).await.unwrap(),
            Probe::WaitUntil(_)
        ));
        assert!(record_failure(&pool, MISTRAL_AUTH, 401).await.unwrap());
        let at = retry_at(&pool, MISTRAL_AUTH).await.unwrap();
        assert!(at >= chrono::Utc::now() + AUTH_RETRY_MIN - Duration::seconds(5));
        assert_eq!(all(&pool).await.unwrap().len(), 1);

        assert!(clear(&pool, MISTRAL_AUTH).await.unwrap());
        assert!(!clear(&pool, MISTRAL_AUTH).await.unwrap());
        assert_eq!(
            claim_probe(&pool, MISTRAL_AUTH).await.unwrap(),
            Probe::Clear
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reasoning_refusals_are_counted_and_cleared(pool: PgPool) {
        assert!(reasoning_refusals(&pool).await.unwrap().is_empty());
        record_reasoning_refusal(&pool, "mistral-large-latest", 400)
            .await
            .unwrap();
        record_reasoning_refusal(&pool, "mistral-large-latest", 422)
            .await
            .unwrap();
        let refusals = reasoning_refusals(&pool).await.unwrap();
        assert_eq!(refusals.len(), 1);
        assert_eq!(
            (
                refusals[0].model.as_str(),
                refusals[0].last_http_status,
                refusals[0].refusals
            ),
            ("mistral-large-latest", 422, 2)
        );
        assert!(refusals[0].since_paris.contains(" à "));
        assert!(
            clear_reasoning_refusal(&pool, "mistral-large-latest")
                .await
                .unwrap()
        );
        assert!(
            !clear_reasoning_refusal(&pool, "mistral-large-latest")
                .await
                .unwrap()
        );
    }
}
