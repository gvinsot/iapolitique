//! Daily retention purge (AGENTS.md §7.3), run by the worker at 03:30 Europe/Paris.
//!
//! - Rejected ideas, contributions and reports: their text (and the free-text territory of
//!   ideas) is erased 30 days after the rejection, their tracking tokens and review requests
//!   are deleted, the second opinion explanation (which may paraphrase the text) is erased, and
//!   the status becomes `purged`. Aggregated data stays: date, reason, level, decision model
//!   scores. A text whose human review is still pending waits for the administrator.
//! - Hidden ideas (§7.3): 12 months after their masking (and without a pending human review)
//!   their text and everything derived from it are erased and the status becomes `purged`: the
//!   original text, the territory, the title, summary and slug, the text of their analyses
//!   (amounts, category and scores stay for statistics), the explanations of their report
//!   reviews, their embedding, the rights of reply published under them; their contributions
//!   are deleted (with their tracking tokens) and the duplicates merged into them are purged too.
//! - Withdrawn ideas (deleted by their author after publication, §7.1): the same purge, 12 months
//!   after the withdrawal (`withdrawn → purged`).
//! - Sealed copies of escalated content (owner decision D3) are deleted one year after the
//!   escalation, whatever the purges above did to the idea meanwhile.
//! - Handled review requests are deleted one year after their handling.
//! - Reports: the optional name and e-mail of the reporter are erased 6 months after the report;
//!   reviewed reports lose their details and their tracking token 6 months after the review;
//!   reports that never reached a decision (stuck after a dead job) lose them 6 months after
//!   the report; the reports of a purged hidden idea lose them with it, whatever their status
//!   (the status itself is kept: the state machine has no `accepted → purged`).
//! - Expired tracking tokens are deleted.
//! - Rate limit salts and counters of previous days are destroyed (§7.2).
//! - Finished jobs and superseded exports are removed after a while, job logs after
//!   [`JOB_LOG_RETENTION_DAYS`] (and with their job).

use serde::Serialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::schedule::DailySchedule;
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobError, JobKind, JobOutcome};
use crate::domain::status::{ContributionStatus, IdeaStatus, ReportStatus, StateMachine};

/// Days a rejected text is kept before being purged.
pub const REJECTED_TEXT_RETENTION_DAYS: i32 = 30;
/// Months the contact of a reporter and the details of a reviewed report are kept (§7.3).
pub const REPORT_RETENTION_MONTHS: i32 = 6;
/// Months a hidden idea keeps its text (§7.3).
pub const HIDDEN_TEXT_RETENTION_MONTHS: i32 = 12;
/// Months a withdrawn idea is kept for the admin before its purge (§7.3, owner decision D2).
pub const WITHDRAWN_RETENTION_MONTHS: i32 = 12;
/// Months a handled review request is kept (requests about rights: one year).
pub const REVIEW_REQUEST_RETENTION_MONTHS: i32 = 12;
/// Days a succeeded job is kept (debugging).
pub const SUCCEEDED_JOB_RETENTION_DAYS: i32 = 7;
/// Days a dead job is kept (investigation from the admin).
pub const DEAD_JOB_RETENTION_DAYS: i32 = 90;
/// Days a job log line is kept (§8): those of a dead job go before the job itself.
pub const JOB_LOG_RETENTION_DAYS: i32 = 30;
/// Days an export is kept once a newer one exists.
pub const EXPORT_RETENTION_DAYS: i32 = 7;

pub const SCHEDULE: DailySchedule = DailySchedule::new("daily_purge", JobKind::Purge, 3, 30);

/// What the purge removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PurgeReport {
    pub ideas: i64,
    pub contributions: i64,
    pub reports: i64,
    /// Hidden ideas purged after 12 months.
    pub hidden_ideas: i64,
    /// Withdrawn ideas purged 12 months after their withdrawal.
    pub withdrawn_ideas: i64,
    /// Duplicates purged with the hidden idea they were merged into.
    pub merged_duplicates: i64,
    /// Contributions deleted with their hidden idea.
    pub hidden_idea_contributions: i64,
    /// Reports whose reporter contact was erased.
    pub report_contacts: i64,
    /// Reviewed reports whose details were erased.
    pub reviewed_reports: i64,
    /// Reports of purged hidden ideas whose details, contact and token were erased.
    pub hidden_idea_reports: i64,
    /// Reports without a decision 6 months after their creation, details and token erased.
    pub stale_reports: i64,
    pub tracking_tokens: i64,
    pub expired_tracking_tokens: i64,
    /// Review requests of purged texts and handled requests past their retention.
    pub review_requests: i64,
    /// Sealed copies of escalated content, one year after the escalation.
    pub escalation_seals: i64,
    pub rate_limit_salts: i64,
    pub rate_limit_counters: i64,
    pub jobs: i64,
    pub job_logs: i64,
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
        (
            IdeaStatus::Hidden.can_transition_to(IdeaStatus::Purged),
            "hidden idea",
        ),
        (
            IdeaStatus::Merged.can_transition_to(IdeaStatus::Purged),
            "merged idea",
        ),
        (
            IdeaStatus::Withdrawn.can_transition_to(IdeaStatus::Purged),
            "withdrawn idea",
        ),
    ] {
        assert!(allowed, "{entity}: -> purged must be allowed");
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

    purge_hidden_ideas(&mut tx, &mut report).await?;

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

    // Backstop: a report stuck before its review (dead screening or review job) keeps neither
    // its details nor its token beyond the retention of reports (PRIV-6).
    let (stale, tokens): (i64, i64) = sqlx::query_as(
        "WITH purged AS (
             UPDATE reports SET details = NULL, details_purged_at = now()
             WHERE status IN ('submitted', 'screening', 'second_opinion', 'accepted')
               AND details_purged_at IS NULL
               AND created_at < now() - make_interval(months => $1)
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
    report.stale_reports = stale;
    report.tracking_tokens += tokens;

    report.escalation_seals = crate::db::escalation_seals::purge(&mut tx).await?;

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

    report.job_logs =
        sqlx::query("DELETE FROM job_logs WHERE logged_at < now() - make_interval(days => $1)")
            .bind(JOB_LOG_RETENTION_DAYS)
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

/// Purge of the ideas hidden for [`HIDDEN_TEXT_RETENTION_MONTHS`] and of those withdrawn by
/// their author for [`WITHDRAWN_RETENTION_MONTHS`] (see the module documentation).
async fn purge_hidden_ideas(
    conn: &mut PgConnection,
    report: &mut PurgeReport,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM ideas
         WHERE (status = 'hidden'
                AND coalesce(hidden_at, status_changed_at) < now() - make_interval(months => $1)
                AND NOT EXISTS (SELECT 1 FROM review_requests q
                                WHERE q.target_kind = 'idea' AND q.target_id = ideas.id
                                  AND q.status = 'pending'))
            OR (status = 'withdrawn'
                AND coalesce(withdrawn_at, status_changed_at) < now() - make_interval(months => $2))
         FOR UPDATE",
    )
    .bind(HIDDEN_TEXT_RETENTION_MONTHS)
    .bind(WITHDRAWN_RETENTION_MONTHS)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let ideas: Vec<Uuid> = rows.iter().map(|(id, _)| *id).collect();
    let from: Vec<&str> = rows.iter().map(|(_, status)| status.as_str()).collect();

    // Contributions (improvements, objections, duplicates): deleted with their traces.
    let contributions: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM contributions WHERE idea_id = ANY($1) FOR UPDATE")
            .bind(&ideas)
            .fetch_all(&mut *conn)
            .await?;
    let (tokens, requests) = erase_traces(conn, "contribution", &contributions).await?;
    report.tracking_tokens += tokens;
    report.review_requests += requests;
    report.hidden_idea_contributions = sqlx::query("DELETE FROM contributions WHERE id = ANY($1)")
        .bind(&contributions)
        .execute(&mut *conn)
        .await?
        .rows_affected() as i64;

    // Rights of reply published under them (they quote or answer the idea).
    sqlx::query("DELETE FROM rights_of_reply WHERE idea_id = ANY($1)")
        .bind(&ideas)
        .execute(&mut *conn)
        .await?;

    // Duplicates merged into them: same purge as the ideas, without publication columns.
    let duplicates: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE ideas SET status = 'purged', status_changed_at = now(), parent_idea_id = NULL,
             original_text = NULL, submitted_territory = NULL, original_text_purged_at = now()
         WHERE parent_idea_id = ANY($1) AND status = 'merged'
         RETURNING id",
    )
    .bind(&ideas)
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO idea_events (idea_id, from_status, to_status, actor, reason_code)
         SELECT id, 'merged', 'purged', 'worker', 'retention' FROM unnest($1::uuid[]) AS id",
    )
    .bind(&duplicates)
    .execute(&mut *conn)
    .await?;
    report.merged_duplicates = duplicates.len() as i64;

    sqlx::query(
        "UPDATE ideas SET status = 'purged', status_changed_at = now(),
             original_text = NULL, submitted_territory = NULL, original_text_purged_at = now(),
             title = NULL, summary = NULL, slug = NULL, territory_name = NULL
         WHERE id = ANY($1)",
    )
    .bind(&ideas)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO idea_events (idea_id, from_status, to_status, actor, reason_code)
         SELECT id, from_status, 'purged', 'worker', 'retention'
         FROM unnest($1::uuid[], $2::text[]) AS t (id, from_status)",
    )
    .bind(&ideas)
    .bind(&from)
    .execute(&mut *conn)
    .await?;
    report.withdrawn_ideas = from.iter().filter(|s| **s == "withdrawn").count() as i64;
    report.hidden_ideas = ideas.len() as i64 - report.withdrawn_ideas;

    // Texts generated from them: the figures stay for statistics.
    sqlx::query(
        "UPDATE analyses SET content = '{}'::jsonb, title = '', summary = '',
             sources = '[]'::jsonb, territory_name = NULL
         WHERE idea_id = ANY($1)",
    )
    .bind(&ideas)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE report_reviews SET explanation = '', corrections = '' WHERE idea_id = ANY($1)",
    )
    .bind(&ideas)
    .execute(&mut *conn)
    .await?;
    let mut purged = ideas;
    purged.extend(duplicates);
    sqlx::query("DELETE FROM idea_embeddings WHERE idea_id = ANY($1)")
        .bind(&purged)
        .execute(&mut *conn)
        .await?;
    let (tokens, requests) = erase_traces(conn, "idea", &purged).await?;
    report.tracking_tokens += tokens;
    report.review_requests += requests;

    // Their reports, whatever their status (one stuck in `accepted` after a dead review job
    // too): details, reporter contact, tokens, review requests and explanations (PRIV-6).
    let reports: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE reports SET details = NULL,
             details_purged_at = coalesce(details_purged_at, now()),
             reporter_name = NULL, reporter_email = NULL,
             reporter_purged_at = coalesce(reporter_purged_at, now())
         WHERE idea_id = ANY($1)
         RETURNING id",
    )
    .bind(&purged)
    .fetch_all(&mut *conn)
    .await?;
    let (tokens, requests) = erase_traces(conn, "report", &reports).await?;
    report.tracking_tokens += tokens;
    report.review_requests += requests;
    report.hidden_idea_reports = reports.len() as i64;
    Ok(())
}

/// Deletes the tracking tokens and the review requests of purged texts of kind `kind` and
/// erases their second opinion explanations. Returns the numbers of tokens and requests deleted.
async fn erase_traces(
    conn: &mut PgConnection,
    kind: &str,
    ids: &[Uuid],
) -> Result<(i64, i64), sqlx::Error> {
    if ids.is_empty() {
        return Ok((0, 0));
    }
    let tokens =
        sqlx::query("DELETE FROM tracking_tokens WHERE target_kind = $1 AND target_id = ANY($2)")
            .bind(kind)
            .bind(ids)
            .execute(&mut *conn)
            .await?
            .rows_affected() as i64;
    let requests =
        sqlx::query("DELETE FROM review_requests WHERE target_kind = $1 AND target_id = ANY($2)")
            .bind(kind)
            .bind(ids)
            .execute(&mut *conn)
            .await?
            .rows_affected() as i64;
    sqlx::query(
        "UPDATE screenings SET second_opinion_explanation = NULL
         WHERE target_kind = $1 AND target_id = ANY($2)
           AND second_opinion_explanation IS NOT NULL",
    )
    .bind(kind)
    .bind(ids)
    .execute(&mut *conn)
    .await?;
    Ok((tokens, requests))
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
                hidden_ideas = report.hidden_ideas,
                withdrawn_ideas = report.withdrawn_ideas,
                merged_duplicates = report.merged_duplicates,
                hidden_idea_contributions = report.hidden_idea_contributions,
                report_contacts = report.report_contacts,
                reviewed_reports = report.reviewed_reports,
                hidden_idea_reports = report.hidden_idea_reports,
                stale_reports = report.stale_reports,
                tracking_tokens = report.tracking_tokens,
                expired_tracking_tokens = report.expired_tracking_tokens,
                review_requests = report.review_requests,
                escalation_seals = report.escalation_seals,
                rate_limit_salts = report.rate_limit_salts,
                rate_limit_counters = report.rate_limit_counters,
                jobs = report.jobs,
                job_logs = report.job_logs,
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
        assert_eq!(report.stale_reports, 1);
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
                // A report still waiting for its review 6 months later is stuck (dead job):
                // its details go too (PRIV-6).
                assert_eq!(details, None, "{}", id == old_accepted);
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
        // Logs of the dead job that is kept: the one older than 30 days goes.
        sqlx::query(
            "INSERT INTO job_logs (job_id, logged_at, level, target, message)
             SELECT id, now() - age, 'info', 't', 'm'
             FROM jobs, (VALUES (interval '31 days'), (interval '1 day')) AS a(age)
             WHERE status = 'dead' AND finished_at > now() - interval '31 days'",
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
        assert_eq!(report.job_logs, 1);
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

    /// A published idea hidden `months` ago, with a tracking token.
    async fn hidden_idea(pool: &PgPool, slug: &str, months: i32) -> Uuid {
        let id = crate::test_support::published_idea(pool, slug).await;
        sqlx::query(
            "UPDATE ideas SET status = 'hidden', hidden_reason = 'report_review',
                 hidden_at = now() - make_interval(months => $2)
             WHERE id = $1",
        )
        .bind(id)
        .bind(months)
        .execute(pool)
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
        id
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn purges_hidden_ideas_after_12_months(pool: PgPool) {
        use crate::db::status::Change;
        use crate::domain::types::Actor;

        let old = hidden_idea(&pool, "ancienne", 13).await;
        let recent = hidden_idea(&pool, "recente", 11).await;
        let reviewed = hidden_idea(&pool, "en-reexamen", 13).await;
        sqlx::query("INSERT INTO review_requests (target_kind, target_id) VALUES ('idea', $1)")
            .bind(reviewed)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE analyses SET content = $2 WHERE idea_id = $1")
            .bind(old)
            .bind(crate::test_support::analysis_output())
            .execute(&pool)
            .await
            .unwrap();
        crate::db::embeddings::upsert(
            &pool,
            old,
            "mistral-embed",
            &crate::test_support::unit_vector(0.0),
            &crate::db::embeddings::content_hash("texte"),
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO report_reviews (idea_id, outcome, explanation, model, prompt_version)
             VALUES ($1, 'hide', 'Le texte nomme un particulier.', 'm', '1.1.0')",
        )
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
        // A contribution taken into account and a duplicate merged into the old idea.
        let contribution: Uuid = sqlx::query_scalar(
            "INSERT INTO contributions (idea_id, kind, status, text)
             VALUES ($1, 'objection', 'accepted', 'Objection.') RETURNING id",
        )
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
             VALUES (sha256(uuid_send($1)), 'contribution', $1)",
        )
        .bind(contribution)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on)
             VALUES ($1, 'Le service concerné', 'Réponse.', current_date)",
        )
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
        let duplicate = crate::test_support::idea_with_status(&pool, "dedup").await;
        sqlx::query("UPDATE ideas SET status = 'published' WHERE id = $1")
            .bind(old)
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        crate::db::ideas::merge(&mut tx, duplicate, old, &Change::by(Actor::Worker))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        sqlx::query("UPDATE ideas SET status = 'hidden' WHERE id = $1")
            .bind(old)
            .execute(&pool)
            .await
            .unwrap();

        let report = run(&pool).await.unwrap();
        assert_eq!(
            (
                report.hidden_ideas,
                report.merged_duplicates,
                report.hidden_idea_contributions
            ),
            (1, 1, 2)
        );
        assert_eq!(report.tracking_tokens, 2);

        let idea: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(i) FROM ideas i WHERE id = $1")
                .bind(old)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(idea["status"], "purged");
        let replies: i64 =
            sqlx::query_scalar("SELECT count(*) FROM rights_of_reply WHERE idea_id = $1")
                .bind(old)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(replies, 0);
        for field in [
            "original_text",
            "title",
            "summary",
            "slug",
            "territory_name",
        ] {
            assert_eq!(idea[field], serde_json::Value::Null, "{field}");
        }
        // Figures stay for statistics.
        assert_eq!(idea["score_eur"], 1000);
        assert!(idea["original_text_purged_at"].is_string());
        let (content, title, sources): (serde_json::Value, String, serde_json::Value) =
            sqlx::query_as("SELECT content, title, sources FROM analyses WHERE idea_id = $1")
                .bind(old)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            (content, title.as_str(), sources),
            (serde_json::json!({}), "", serde_json::json!([]))
        );
        let (explanation,): (String,) =
            sqlx::query_as("SELECT explanation FROM report_reviews WHERE idea_id = $1")
                .bind(old)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(explanation, "");
        let (status, parent, text): (String, Option<Uuid>, Option<String>) =
            sqlx::query_as("SELECT status, parent_idea_id, original_text FROM ideas WHERE id = $1")
                .bind(duplicate)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((status.as_str(), parent, text), ("purged", None, None));
        let (contributions, embeddings, tokens): (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM contributions),
                    (SELECT count(*) FROM idea_embeddings),
                    (SELECT count(*) FROM tracking_tokens WHERE target_id = ANY($1))",
        )
        .bind(vec![old, duplicate, contribution])
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((contributions, embeddings, tokens), (0, 0, 0));
        let events: Vec<(Uuid, Option<String>, String)> = sqlx::query_as(
            "SELECT idea_id, from_status, to_status FROM idea_events
             WHERE reason_code = 'retention' ORDER BY idea_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let mut expected = vec![
            (old, Some("hidden".to_owned()), "purged".to_owned()),
            (duplicate, Some("merged".to_owned()), "purged".to_owned()),
        ];
        expected.sort();
        assert_eq!(events, expected);

        // Hidden for less than 12 months, or with a pending human review: kept.
        for id in [recent, reviewed] {
            let (status, text): (String, Option<String>) =
                sqlx::query_as("SELECT status, original_text FROM ideas WHERE id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(status, "hidden");
            assert!(text.is_some());
        }
        // Idempotent.
        assert_eq!(run(&pool).await.unwrap().hidden_ideas, 0);
    }

    /// A published idea withdrawn by its author `months` ago (owner decision D2), with a
    /// contribution, a report, a right of reply and a merged duplicate.
    async fn withdrawn_idea(pool: &PgPool, slug: &str, months: i32) -> (Uuid, Uuid, Uuid) {
        let id = crate::test_support::published_idea(pool, slug).await;
        let duplicate: Uuid = sqlx::query_scalar(
            "INSERT INTO ideas (original_text, status, parent_idea_id)
             VALUES ('Même idée.', 'merged', $1) RETURNING id",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contributions (idea_id, kind, status, text)
             VALUES ($1, 'objection', 'accepted', 'Objection.')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        let report: Uuid = sqlx::query_scalar(
            "INSERT INTO reports (idea_id, reason, details, reporter_email, status)
             VALUES ($1, 'autre', 'Précision.', 'r@exemple.fr', 'accepted') RETURNING id",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on)
             VALUES ($1, 'Le service concerné', 'Réponse.', current_date)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE ideas SET status = 'withdrawn', withdrawn_from = 'published',
                 withdrawn_at = now() - make_interval(months => $2)
             WHERE id = $1",
        )
        .bind(id)
        .bind(months)
        .execute(pool)
        .await
        .unwrap();
        for (kind, target) in [("idea", duplicate), ("report", report)] {
            sqlx::query(
                "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
                 VALUES (sha256(uuid_send($2)), $1, $2)",
            )
            .bind(kind)
            .bind(target)
            .execute(pool)
            .await
            .unwrap();
        }
        (id, duplicate, report)
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn purges_withdrawn_ideas_12_months_after_their_withdrawal(pool: PgPool) {
        let (old, duplicate, report_id) = withdrawn_idea(&pool, "ancienne", 13).await;
        let (recent, _, _) = withdrawn_idea(&pool, "recente", 11).await;

        let report = run(&pool).await.unwrap();
        assert_eq!(
            (
                report.withdrawn_ideas,
                report.hidden_ideas,
                report.merged_duplicates,
                report.hidden_idea_contributions,
                report.hidden_idea_reports
            ),
            (1, 0, 1, 1, 1)
        );

        let idea: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(i) FROM ideas i WHERE id = $1")
                .bind(old)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(idea["status"], "purged");
        for field in ["original_text", "title", "summary", "slug"] {
            assert_eq!(idea[field], serde_json::Value::Null, "{field}");
        }
        // The withdrawal stays known: the idea is never counted again in public statistics.
        assert!(idea["withdrawn_at"].is_string());
        assert_eq!(idea["withdrawn_from"], "published");
        let (contributions, replies, details, contact, tokens): (
            i64,
            i64,
            Option<String>,
            Option<String>,
            i64,
        ) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM contributions WHERE idea_id = $1),
                    (SELECT count(*) FROM rights_of_reply WHERE idea_id = $1),
                    (SELECT details FROM reports WHERE id = $2),
                    (SELECT reporter_email FROM reports WHERE id = $2),
                    (SELECT count(*) FROM tracking_tokens WHERE target_id = ANY($3))",
        )
        .bind(old)
        .bind(report_id)
        .bind(vec![duplicate, report_id])
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (contributions, replies, details, contact, tokens),
            (0, 0, None, None, 0)
        );
        let events: Vec<(Uuid, Option<String>)> = sqlx::query_as(
            "SELECT idea_id, from_status FROM idea_events
             WHERE reason_code = 'retention' AND to_status = 'purged' ORDER BY idea_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let mut expected = vec![
            (old, Some("withdrawn".to_owned())),
            (duplicate, Some("merged".to_owned())),
        ];
        expected.sort();
        assert_eq!(events, expected);

        // Withdrawn 11 months ago: kept for the admin.
        let (status, text): (String, Option<String>) =
            sqlx::query_as("SELECT status, original_text FROM ideas WHERE id = $1")
                .bind(recent)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "withdrawn");
        assert!(text.is_some());
    }

    /// A report on `idea`, in `status`, created `months` ago, with details, a contact and a
    /// tracking token.
    async fn report_on(pool: &PgPool, idea: Uuid, status: &str, months: i32) -> Uuid {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO reports (idea_id, reason, details, status, reporter_name, created_at)
             VALUES ($1, 'contenu_problematique', 'Le texte vise mon voisin Paul Martin.', $2,
                     'Paul', now() - make_interval(months => $3))
             RETURNING id",
        )
        .bind(idea)
        .bind(status)
        .bind(months)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
             VALUES (sha256(uuid_send($1)), 'report', $1)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn report_traces(pool: &PgPool, id: Uuid) -> (Option<String>, Option<String>, i64) {
        sqlx::query_as(
            "SELECT details, reporter_name,
                    (SELECT count(*) FROM tracking_tokens WHERE target_id = $1)
             FROM reports WHERE id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// PRIV-6: reports that never reached a decision do not keep their details and token
    /// forever, neither with their purged idea nor on their own.
    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn purges_reports_stuck_before_their_review(pool: PgPool) {
        let hidden = hidden_idea(&pool, "ancienne", 13).await;
        let stuck_on_hidden = report_on(&pool, hidden, "accepted", 1).await;
        let published = crate::test_support::published_idea(&pool, "publiee").await;
        let stale = report_on(&pool, published, "accepted", 7).await;
        let recent = report_on(&pool, published, "accepted", 1).await;

        let report = run(&pool).await.unwrap();
        assert_eq!((report.hidden_idea_reports, report.stale_reports), (1, 1));
        assert_eq!(report_traces(&pool, stuck_on_hidden).await, (None, None, 0));
        let (details, _, tokens) = report_traces(&pool, stale).await;
        assert_eq!((details, tokens), (None, 0));
        let (details, name, tokens) = report_traces(&pool, recent).await;
        assert!(details.is_some() && name.is_some());
        assert_eq!(tokens, 1);
    }
}
