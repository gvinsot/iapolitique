//! Daily retention purge (AGENTS.md §7.3), run by the worker at 03:30 Europe/Paris.
//!
//! - Rejected ideas, contributions and reports: their text (and the free-text territory of
//!   ideas) is erased 30 days after the rejection, their tracking tokens are deleted, the second
//!   opinion explanation (which may paraphrase the text) is erased, and the status becomes
//!   `purged`. Aggregated data stays: date, reason, level, decision model scores.
//! - Expired tracking tokens are deleted.
//! - Rate limit salts and counters of previous days are destroyed (§7.2).
//! - Finished jobs and superseded exports are removed after a while.

use serde::Serialize;
use sqlx::PgPool;

use super::schedule::DailySchedule;
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobError, JobKind, JobOutcome};
use crate::domain::status::{ContributionStatus, IdeaStatus, ReportStatus, StateMachine};

/// Days a rejected text is kept before being purged.
pub const REJECTED_TEXT_RETENTION_DAYS: i32 = 30;
/// Days a succeeded job is kept (debugging).
pub const SUCCEEDED_JOB_RETENTION_DAYS: i32 = 7;
/// Days a dead job is kept (investigation from the admin).
pub const DEAD_JOB_RETENTION_DAYS: i32 = 90;
/// Days an export is kept once a newer one exists.
pub const EXPORT_RETENTION_DAYS: i32 = 7;

pub const SCHEDULE: DailySchedule = DailySchedule::new("daily_purge", JobKind::Purge, 3, 30);

/// What the purge removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PurgeReport {
    pub ideas: i64,
    pub contributions: i64,
    pub reports: i64,
    pub tracking_tokens: i64,
    pub expired_tracking_tokens: i64,
    pub rate_limit_salts: i64,
    pub rate_limit_counters: i64,
    pub jobs: i64,
    pub exports: i64,
}

/// Runs the whole purge in one transaction.
pub async fn run(pool: &PgPool) -> Result<PurgeReport, sqlx::Error> {
    // The SQL below performs these transitions in bulk: keep it tied to the state machines.
    for (allowed, entity) in [
        (
            IdeaStatus::Rejected.can_transition_to(IdeaStatus::Purged),
            "idea",
        ),
        (
            ContributionStatus::Rejected.can_transition_to(ContributionStatus::Purged),
            "contribution",
        ),
        (
            ReportStatus::Rejected.can_transition_to(ReportStatus::Purged),
            "report",
        ),
    ] {
        assert!(allowed, "{entity}: rejected -> purged must be allowed");
    }

    let mut tx = pool.begin().await?;
    let mut report = PurgeReport::default();

    let (ideas, tokens): (i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE ideas SET status = 'purged', status_changed_at = now(),
                 original_text = NULL, submitted_territory = NULL,
                 original_text_purged_at = now()
             WHERE status = 'rejected'
               AND coalesce(rejected_at, status_changed_at) < now() - make_interval(days => $1)
             RETURNING id
         ), events AS (
             INSERT INTO idea_events (idea_id, from_status, to_status, actor, reason_code)
             SELECT id, 'rejected', 'purged', 'worker', 'retention' FROM purged
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'idea' AND t.target_id = p.id
             RETURNING 1
         ), explanations AS (
             UPDATE screenings s SET second_opinion_explanation = NULL FROM purged p
             WHERE s.target_kind = 'idea' AND s.target_id = p.id
               AND s.second_opinion_explanation IS NOT NULL
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens)",
    )
    .bind(REJECTED_TEXT_RETENTION_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    report.ideas = ideas;
    report.tracking_tokens += tokens;

    let (contributions, tokens): (i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE contributions SET status = 'purged', status_changed_at = now(),
                 text = NULL, text_purged_at = now()
             WHERE status = 'rejected'
               AND coalesce(rejected_at, status_changed_at) < now() - make_interval(days => $1)
             RETURNING id
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'contribution' AND t.target_id = p.id
             RETURNING 1
         ), explanations AS (
             UPDATE screenings s SET second_opinion_explanation = NULL FROM purged p
             WHERE s.target_kind = 'contribution' AND s.target_id = p.id
               AND s.second_opinion_explanation IS NOT NULL
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens)",
    )
    .bind(REJECTED_TEXT_RETENTION_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    report.contributions = contributions;
    report.tracking_tokens += tokens;

    let (reports, tokens): (i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE reports SET status = 'purged', status_changed_at = now(),
                 details = NULL, details_purged_at = now()
             WHERE status = 'rejected'
               AND coalesce(rejected_at, status_changed_at) < now() - make_interval(days => $1)
             RETURNING id
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'report' AND t.target_id = p.id
             RETURNING 1
         ), explanations AS (
             UPDATE screenings s SET second_opinion_explanation = NULL FROM purged p
             WHERE s.target_kind = 'report' AND s.target_id = p.id
               AND s.second_opinion_explanation IS NOT NULL
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens)",
    )
    .bind(REJECTED_TEXT_RETENTION_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    report.reports = reports;
    report.tracking_tokens += tokens;

    report.expired_tracking_tokens =
        sqlx::query("DELETE FROM tracking_tokens WHERE expires_at < now()")
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;

    // Salts live for their Paris calendar day only; counters go with them.
    report.rate_limit_counters = sqlx::query(
        "DELETE FROM rate_limit_counters WHERE day < (now() AT TIME ZONE 'Europe/Paris')::date",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    report.rate_limit_salts = sqlx::query(
        "DELETE FROM rate_limit_salts WHERE day < (now() AT TIME ZONE 'Europe/Paris')::date",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    report.jobs = sqlx::query(
        "DELETE FROM jobs
         WHERE (status = 'succeeded' AND finished_at < now() - make_interval(days => $1))
            OR (status = 'dead' AND finished_at < now() - make_interval(days => $2))",
    )
    .bind(SUCCEEDED_JOB_RETENTION_DAYS)
    .bind(DEAD_JOB_RETENTION_DAYS)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    report.exports = sqlx::query(
        "DELETE FROM exports
         WHERE generated_at < now() - make_interval(days => $1)
           AND id NOT IN (
               SELECT DISTINCT ON (format) id FROM exports ORDER BY format, generated_at DESC, id DESC
           )",
    )
    .bind(EXPORT_RETENTION_DAYS)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    tx.commit().await?;
    Ok(report)
}

/// Worker handler of [`JobKind::Purge`].
#[derive(Debug, Default, Clone, Copy)]
pub struct PurgeHandler;

impl JobHandler for PurgeHandler {
    fn kind(&self) -> JobKind {
        JobKind::Purge
    }

    fn schedule(&self) -> Option<DailySchedule> {
        Some(SCHEDULE)
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, _job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let report = run(&ctx.pool).await.map_err(JobError::from)?;
            tracing::info!(
                ideas = report.ideas,
                contributions = report.contributions,
                reports = report.reports,
                tracking_tokens = report.tracking_tokens,
                expired_tracking_tokens = report.expired_tracking_tokens,
                rate_limit_salts = report.rate_limit_salts,
                rate_limit_counters = report.rate_limit_counters,
                jobs = report.jobs,
                exports = report.exports,
                "retention purge done"
            );
            Ok(JobOutcome::Done)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    async fn rejected_idea(pool: &PgPool, days_ago: i32) -> Uuid {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO ideas (original_text, submitted_admin_level, submitted_territory, status,
                                rejection_reason_code, rejected_at)
             VALUES ('texte refusé', 'commune', 'Lyon', 'rejected', 'off_topic',
                     now() - make_interval(days => $1))
             RETURNING id",
        )
        .bind(days_ago)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
             VALUES (sha256(uuid_send($1)), 'idea', $1)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO screenings (target_kind, target_id, prompt_version, outcome, gray_zone,
                                     pass_probabilities, second_opinion_decision,
                                     second_opinion_explanation)
             VALUES ('idea', $1, '1', 'gray_zone', true, '{\"on_topic\": 0.4}', 'reject',
                     'paraphrase du texte')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn purges_rejected_ideas_after_30_days(pool: PgPool) {
        let old = rejected_idea(&pool, 31).await;
        let recent = rejected_idea(&pool, 29).await;
        let (published,): (Uuid,) = sqlx::query_as(
            "INSERT INTO ideas (original_text) VALUES ('texte conservé') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let report = run(&pool).await.unwrap();
        assert_eq!(report.ideas, 1);
        assert_eq!(report.tracking_tokens, 1);

        let row: (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        ) = sqlx::query_as(
            "SELECT status, original_text, submitted_territory, submitted_admin_level,
                    rejection_reason_code
             FROM ideas WHERE id = $1",
        )
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "purged".into(),
                None,
                None,
                Some("commune".into()),
                "off_topic".into()
            )
        );

        // Calibration data stays, without the paraphrase.
        let (probabilities, explanation): (serde_json::Value, Option<String>) = sqlx::query_as(
            "SELECT pass_probabilities, second_opinion_explanation FROM screenings WHERE target_id = $1",
        )
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(probabilities["on_topic"], 0.4);
        assert_eq!(explanation, None);

        let (event,): (String,) = sqlx::query_as(
            "SELECT from_status || '->' || to_status FROM idea_events WHERE idea_id = $1",
        )
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event, "rejected->purged");

        let tokens: Vec<(Uuid,)> =
            sqlx::query_as("SELECT target_id FROM tracking_tokens ORDER BY 1")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(tokens, vec![(recent,)]);

        for id in [recent, published] {
            let (text,): (Option<String>,) =
                sqlx::query_as("SELECT original_text FROM ideas WHERE id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert!(text.is_some());
        }

        // Idempotent.
        assert_eq!(run(&pool).await.unwrap(), PurgeReport::default());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn purges_rejected_contributions_and_reports(pool: PgPool) {
        let (idea,): (Uuid,) =
            sqlx::query_as("INSERT INTO ideas (original_text) VALUES ('idée') RETURNING id")
                .fetch_one(&pool)
                .await
                .unwrap();
        let (contribution,): (Uuid,) = sqlx::query_as(
            "INSERT INTO contributions (idea_id, kind, status, text, rejected_at)
             VALUES ($1, 'objection', 'rejected', 'texte', now() - interval '40 days')
             RETURNING id",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        let (report_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO reports (idea_id, reason, details, status, rejected_at)
             VALUES ($1, 'autre', 'précision', 'rejected', now() - interval '40 days')
             RETURNING id",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO reports (idea_id, reason, details, status, rejected_at)
             VALUES ($1, 'autre', 'récent', 'rejected', now() - interval '2 days')",
        )
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
             VALUES (sha256('c'::bytea), 'contribution', $1), (sha256('r'::bytea), 'report', $2)",
        )
        .bind(contribution)
        .bind(report_id)
        .execute(&pool)
        .await
        .unwrap();

        let report = run(&pool).await.unwrap();
        assert_eq!(report.contributions, 1);
        assert_eq!(report.reports, 1);
        assert_eq!(report.tracking_tokens, 2);

        let (status, text): (String, Option<String>) =
            sqlx::query_as("SELECT status, text FROM contributions WHERE id = $1")
                .bind(contribution)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((status.as_str(), text), ("purged", None));
        let (status, details): (String, Option<String>) =
            sqlx::query_as("SELECT status, details FROM reports WHERE id = $1")
                .bind(report_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((status.as_str(), details), ("purged", None));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn destroys_old_salts_tokens_jobs_and_exports(pool: PgPool) {
        sqlx::query(
            "INSERT INTO rate_limit_salts (day, salt) VALUES
                ((now() AT TIME ZONE 'Europe/Paris')::date, gen_random_bytes(32)),
                ((now() AT TIME ZONE 'Europe/Paris')::date - 1, gen_random_bytes(32)),
                ((now() AT TIME ZONE 'Europe/Paris')::date - 2, gen_random_bytes(32))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO rate_limit_counters (day, fingerprint, action, window_start, count)
             SELECT day, gen_random_bytes(32), 'idea', date_trunc('hour', now()), 3
             FROM rate_limit_salts",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tracking_tokens (token_hash, target_kind, target_id, expires_at) VALUES
                (sha256('a'::bytea), 'idea', gen_random_uuid(), now() - interval '1 second'),
                (sha256('b'::bytea), 'idea', gen_random_uuid(), now() + interval '1 day'),
                (sha256('c'::bytea), 'idea', gen_random_uuid(), NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO jobs (kind, status, finished_at) VALUES
                ('purge', 'succeeded', now() - interval '8 days'),
                ('purge', 'succeeded', now() - interval '6 days'),
                ('purge', 'dead', now() - interval '91 days'),
                ('purge', 'dead', now() - interval '30 days')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO exports (format, content, content_sha256, size_bytes, idea_count,
                                  scoring_version, generated_at) VALUES
                ('json', '', sha256(''), 0, 0, '1', now() - interval '20 days'),
                ('json', '', sha256(''), 0, 0, '1', now() - interval '10 days'),
                ('json', '', sha256(''), 0, 0, '1', now() - interval '1 day'),
                ('csv', '', sha256(''), 0, 0, '1', now() - interval '30 days')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let report = run(&pool).await.unwrap();
        assert_eq!(report.rate_limit_salts, 2);
        assert_eq!(report.rate_limit_counters, 2);
        assert_eq!(report.expired_tracking_tokens, 1);
        assert_eq!(report.jobs, 2);
        // The only CSV export is kept even if old.
        assert_eq!(report.exports, 2);

        let (salts, tokens, jobs, exports): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM rate_limit_salts), (SELECT count(*) FROM tracking_tokens),
                    (SELECT count(*) FROM jobs), (SELECT count(*) FROM exports)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((salts, tokens, jobs, exports), (1, 2, 2, 2));
    }
}
