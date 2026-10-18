//! Daily retention purge (AGENTS.md §7.3), run by the worker at 03:30 Europe/Paris.
//!
//! - Rejected ideas, contributions and reports: their text (and the free-text territory of
//!   ideas) is erased 30 days after the rejection, their tracking tokens and review requests
//!   are deleted, the second opinion explanation (which may paraphrase the text) is erased, and
//!   the status becomes `purged`. Aggregated data stays: date, reason, level, decision model
//!   scores. A text whose human review is still pending waits for the administrator.
//! - Handled review requests are deleted one year after their handling.
//! - Reports: the optional name and e-mail of the reporter are erased 6 months after the report;
//!   reviewed reports lose their details and their tracking token 6 months after the review.
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
/// Months the contact of a reporter and the details of a reviewed report are kept (§7.3).
pub const REPORT_RETENTION_MONTHS: i32 = 6;
/// Months a handled review request is kept (requests about rights: one year).
pub const REVIEW_REQUEST_RETENTION_MONTHS: i32 = 12;
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
    /// Reports whose reporter contact was erased.
    pub report_contacts: i64,
    /// Reviewed reports whose details were erased.
    pub reviewed_reports: i64,
    pub tracking_tokens: i64,
    pub expired_tracking_tokens: i64,
    /// Review requests of purged texts and handled requests past their retention.
    pub review_requests: i64,
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

    let (ideas, tokens, requests): (i64, i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE ideas SET status = 'purged', status_changed_at = now(),
                 original_text = NULL, submitted_territory = NULL,
                 original_text_purged_at = now()
             WHERE status = 'rejected'
               AND coalesce(rejected_at, status_changed_at) < now() - make_interval(days => $1)
               AND NOT EXISTS (SELECT 1 FROM review_requests q
                               WHERE q.target_kind = 'idea' AND q.target_id = ideas.id
                                 AND q.status = 'pending')
             RETURNING id
         ), events AS (
             INSERT INTO idea_events (idea_id, from_status, to_status, actor, reason_code)
             SELECT id, 'rejected', 'purged', 'worker', 'retention' FROM purged
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'idea' AND t.target_id = p.id
             RETURNING 1
         ), requests AS (
             DELETE FROM review_requests q USING purged p
             WHERE q.target_kind = 'idea' AND q.target_id = p.id
             RETURNING 1
         ), explanations AS (
             UPDATE screenings s SET second_opinion_explanation = NULL FROM purged p
             WHERE s.target_kind = 'idea' AND s.target_id = p.id
               AND s.second_opinion_explanation IS NOT NULL
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens),
                (SELECT count(*) FROM requests)",
    )
    .bind(REJECTED_TEXT_RETENTION_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    report.ideas = ideas;
    report.tracking_tokens += tokens;
    report.review_requests += requests;

    let (contributions, tokens, requests): (i64, i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE contributions SET status = 'purged', status_changed_at = now(),
                 text = NULL, text_purged_at = now()
             WHERE status = 'rejected'
               AND coalesce(rejected_at, status_changed_at) < now() - make_interval(days => $1)
               AND NOT EXISTS (SELECT 1 FROM review_requests q
                               WHERE q.target_kind = 'contribution' AND q.target_id = contributions.id
                                 AND q.status = 'pending')
             RETURNING id
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'contribution' AND t.target_id = p.id
             RETURNING 1
         ), requests AS (
             DELETE FROM review_requests q USING purged p
             WHERE q.target_kind = 'contribution' AND q.target_id = p.id
             RETURNING 1
         ), explanations AS (
             UPDATE screenings s SET second_opinion_explanation = NULL FROM purged p
             WHERE s.target_kind = 'contribution' AND s.target_id = p.id
               AND s.second_opinion_explanation IS NOT NULL
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens),
                (SELECT count(*) FROM requests)",
    )
    .bind(REJECTED_TEXT_RETENTION_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    report.contributions = contributions;
    report.tracking_tokens += tokens;
    report.review_requests += requests;

    let (reports, tokens, requests): (i64, i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE reports SET status = 'purged', status_changed_at = now(),
                 details = NULL, details_purged_at = now()
             WHERE status = 'rejected'
               AND coalesce(rejected_at, status_changed_at) < now() - make_interval(days => $1)
               AND NOT EXISTS (SELECT 1 FROM review_requests q
                               WHERE q.target_kind = 'report' AND q.target_id = reports.id
                                 AND q.status = 'pending')
             RETURNING id
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'report' AND t.target_id = p.id
             RETURNING 1
         ), requests AS (
             DELETE FROM review_requests q USING purged p
             WHERE q.target_kind = 'report' AND q.target_id = p.id
             RETURNING 1
         ), explanations AS (
             UPDATE screenings s SET second_opinion_explanation = NULL FROM purged p
             WHERE s.target_kind = 'report' AND s.target_id = p.id
               AND s.second_opinion_explanation IS NOT NULL
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens),
                (SELECT count(*) FROM requests)",
    )
    .bind(REJECTED_TEXT_RETENTION_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    report.reports = reports;
    report.tracking_tokens += tokens;
    report.review_requests += requests;

    report.report_contacts = sqlx::query(
        "UPDATE reports SET reporter_name = NULL, reporter_email = NULL,
             reporter_purged_at = now()
         WHERE (reporter_name IS NOT NULL OR reporter_email IS NOT NULL)
           AND created_at < now() - make_interval(months => $1)",
    )
    .bind(REPORT_RETENTION_MONTHS)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    let (reviewed, tokens): (i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE reports SET details = NULL, details_purged_at = now()
             WHERE status = 'reviewed' AND details_purged_at IS NULL
               AND status_changed_at < now() - make_interval(months => $1)
             RETURNING id
         ), tokens AS (
             DELETE FROM tracking_tokens t USING purged p
             WHERE t.target_kind = 'report' AND t.target_id = p.id
             RETURNING 1
         )
         SELECT (SELECT count(*) FROM purged), (SELECT count(*) FROM tokens)",
    )
    .bind(REPORT_RETENTION_MONTHS)
    .fetch_one(&mut *tx)
    .await?;
    report.reviewed_reports = reviewed;
    report.tracking_tokens += tokens;

    report.review_requests += sqlx::query(
        "DELETE FROM review_requests
         WHERE status = 'done' AND handled_at < now() - make_interval(months => $1)",
    )
    .bind(REVIEW_REQUEST_RETENTION_MONTHS)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

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
                report_contacts = report.report_contacts,
                reviewed_reports = report.reviewed_reports,
                tracking_tokens = report.tracking_tokens,
                expired_tracking_tokens = report.expired_tracking_tokens,
                review_requests = report.review_requests,
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
    async fn erases_report_contacts_and_reviewed_reports_after_six_months(pool: PgPool) {
        let idea: Uuid =
            sqlx::query_scalar("INSERT INTO ideas (original_text) VALUES ('idée') RETURNING id")
                .fetch_one(&pool)
                .await
                .unwrap();
        let review: Uuid = sqlx::query_scalar(
            "INSERT INTO report_reviews (idea_id, outcome, explanation, model, prompt_version)
             VALUES ($1, 'maintain', 'RAS', 'm', '1') RETURNING id",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        let insert = |age: &'static str, status: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, Uuid>(
                    "INSERT INTO reports (idea_id, reason, details, status, review_id,
                         reporter_name, reporter_email, good_faith, created_at, status_changed_at)
                     VALUES ($1, 'erreur_chiffrage', 'précision', $2,
                             CASE WHEN $2 = 'reviewed' THEN $3 END, 'Camille', 'c@exemple.fr',
                             true, now() - $4::interval, now() - $4::interval)
                     RETURNING id",
                )
                .bind(idea)
                .bind(status)
                .bind(review)
                .bind(age)
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        let old_reviewed = insert("7 months", "reviewed").await;
        let old_accepted = insert("7 months", "accepted").await;
        let recent_reviewed = insert("5 months", "reviewed").await;
        sqlx::query(
            "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
             VALUES (sha256('old'::bytea), 'report', $1), (sha256('new'::bytea), 'report', $2)",
        )
        .bind(old_reviewed)
        .bind(recent_reviewed)
        .execute(&pool)
        .await
        .unwrap();

        let report = run(&pool).await.unwrap();
        assert_eq!(report.report_contacts, 2);
        assert_eq!(report.reviewed_reports, 1);
        assert_eq!(report.tracking_tokens, 1);

        type Row = (Uuid, Option<String>, Option<String>, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, details, reporter_name, reporter_email FROM reports ORDER BY created_at, id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        for (id, details, name, email) in rows {
            if id == recent_reviewed {
                assert_eq!(
                    (details.as_deref(), name.as_deref(), email.as_deref()),
                    (Some("précision"), Some("Camille"), Some("c@exemple.fr"))
                );
            } else {
                assert_eq!((name, email), (None, None));
                // Details of a report waiting for its review are kept.
                assert_eq!(details.is_some(), id == old_accepted);
            }
        }
        assert_eq!(run(&pool).await.unwrap(), PurgeReport::default());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn pending_human_reviews_hold_the_purge(pool: PgPool) {
        let waiting = rejected_idea(&pool, 40).await;
        let handled = rejected_idea(&pool, 40).await;
        sqlx::query(
            "INSERT INTO review_requests (target_kind, target_id, message, status, handled_at)
             VALUES ('idea', $1, 'Merci de relire.', 'pending', NULL),
                    ('idea', $2, NULL, 'done', now() - interval '1 day'),
                    ('report', gen_random_uuid(), NULL, 'done', now() - interval '13 months')",
        )
        .bind(waiting)
        .bind(handled)
        .execute(&pool)
        .await
        .unwrap();

        let report = run(&pool).await.unwrap();
        assert_eq!(report.ideas, 1);
        // The request of the purged idea and the old handled one.
        assert_eq!(report.review_requests, 2);
        let statuses: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, status FROM ideas ORDER BY created_at, id")
                .fetch_all(&pool)
                .await
                .unwrap();
        for (id, status) in statuses {
            assert_eq!(status, if id == waiting { "rejected" } else { "purged" });
        }
        let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM review_requests")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 1);

        // Once handled, the text goes at the next run.
        sqlx::query("UPDATE review_requests SET status = 'done', handled_at = now()")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(run(&pool).await.unwrap().ideas, 1);
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
