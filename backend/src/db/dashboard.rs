//! Aggregates of the admin dashboard (AGENTS.md §8): screening outcomes, AI costs of the day
//! and of the month (Europe/Paris), and the queues waiting for the administrator.

use sqlx::PgPool;

/// Screening outcomes of one kind of text over a window.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ScreeningCounts {
    pub target_kind: String,
    /// Screenings recorded (decision service errors included).
    pub total: i64,
    pub accepted: i64,
    pub rejected: i64,
    pub gray_zone: i64,
    /// Calls to the decision service that failed.
    pub errors: i64,
    pub second_opinion_accept: i64,
    pub second_opinion_reject: i64,
    /// Screenings labelled by the administrator.
    pub labeled: i64,
}

/// Outcomes of the screenings of the last `window_days` days, per kind of text.
pub async fn screening_counts(
    pool: &PgPool,
    window_days: i32,
) -> sqlx::Result<Vec<ScreeningCounts>> {
    sqlx::query_as(
        "SELECT target_kind, count(*) AS total,
                count(*) FILTER (WHERE outcome = 'accepted') AS accepted,
                count(*) FILTER (WHERE outcome = 'rejected') AS rejected,
                count(*) FILTER (WHERE outcome = 'gray_zone') AS gray_zone,
                count(*) FILTER (WHERE error_code IS NOT NULL AND outcome IS NULL) AS errors,
                count(*) FILTER (WHERE second_opinion_decision = 'accept')
                    AS second_opinion_accept,
                count(*) FILTER (WHERE second_opinion_decision = 'reject')
                    AS second_opinion_reject,
                count(*) FILTER (WHERE human_label IS NOT NULL) AS labeled
         FROM screenings
         WHERE created_at >= now() - make_interval(days => $1)
         GROUP BY target_kind ORDER BY target_kind",
    )
    .bind(window_days)
    .fetch_all(pool)
    .await
}

/// AI calls and estimated costs (micro-euros) of one purpose, today and this month.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurposeCosts {
    pub purpose: String,
    pub provider: String,
    pub calls_today: i64,
    pub calls_month: i64,
    pub cost_today_micro_eur: i64,
    pub cost_month_micro_eur: i64,
}

/// Costs of the current Paris month, by purpose and provider.
pub async fn costs_by_purpose(pool: &PgPool) -> sqlx::Result<Vec<PurposeCosts>> {
    sqlx::query_as(
        "WITH bounds AS (
             SELECT date_trunc('day', now() AT TIME ZONE 'Europe/Paris')
                        AT TIME ZONE 'Europe/Paris' AS day_start,
                    date_trunc('month', now() AT TIME ZONE 'Europe/Paris')
                        AT TIME ZONE 'Europe/Paris' AS month_start
         )
         SELECT c.purpose, c.provider,
                count(*) FILTER (WHERE c.created_at >= b.day_start) AS calls_today,
                count(*) AS calls_month,
                coalesce(sum(c.cost_micro_eur) FILTER (WHERE c.created_at >= b.day_start), 0)
                    ::bigint AS cost_today_micro_eur,
                coalesce(sum(c.cost_micro_eur), 0)::bigint AS cost_month_micro_eur
         FROM ai_calls c CROSS JOIN bounds b
         WHERE c.created_at >= b.month_start
         GROUP BY c.purpose, c.provider
         ORDER BY c.purpose, c.provider",
    )
    .fetch_all(pool)
    .await
}

/// What waits for the administrator.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AdminQueues {
    pub escalations_pending: i64,
    pub review_requests_pending: i64,
    pub ideas_analysis_failed: i64,
    /// Texts waiting for a second opinion (ideas, contributions, reports).
    pub texts_second_opinion: i64,
    /// Texts still `submitted` (decision service unavailable) for more than an hour.
    pub texts_submitted_stale: i64,
    pub ideas_hidden: i64,
    /// Accepted reports waiting for their grouped review.
    pub reports_awaiting_review: i64,
    pub jobs_dead: i64,
}

pub async fn queues(pool: &PgPool) -> sqlx::Result<AdminQueues> {
    sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM escalations WHERE status = 'pending') AS escalations_pending,
            (SELECT count(*) FROM review_requests WHERE status = 'pending')
                AS review_requests_pending,
            (SELECT count(*) FROM ideas WHERE status = 'analysis_failed')
                AS ideas_analysis_failed,
            (SELECT count(*) FROM ideas WHERE status = 'second_opinion')
              + (SELECT count(*) FROM contributions WHERE status = 'second_opinion')
              + (SELECT count(*) FROM reports WHERE status = 'second_opinion')
                AS texts_second_opinion,
            (SELECT count(*) FROM ideas
             WHERE status = 'submitted' AND status_changed_at < now() - interval '1 hour')
              + (SELECT count(*) FROM contributions
                 WHERE status = 'submitted' AND status_changed_at < now() - interval '1 hour')
              + (SELECT count(*) FROM reports
                 WHERE status = 'submitted' AND status_changed_at < now() - interval '1 hour')
                AS texts_submitted_stale,
            (SELECT count(*) FROM ideas WHERE status = 'hidden') AS ideas_hidden,
            (SELECT count(*) FROM reports WHERE status = 'accepted') AS reports_awaiting_review,
            (SELECT count(*) FROM jobs WHERE status = 'dead') AS jobs_dead",
    )
    .fetch_one(pool)
    .await
}
