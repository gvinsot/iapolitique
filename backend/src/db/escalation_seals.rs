//! Sealed copies of escalated content (`escalation_seals`, owner decision D3, AGENTS.md §7.3,
//! §7.5).
//!
//! When a report review escalates an idea, [`seal`] copies, in the transaction that creates the
//! escalation, the masked original text and what was published from it, the current analysis,
//! the review and the reports that raised it (reason and masked details; never the reporter's
//! contact). The copy has no foreign key: it outlives the withdrawal or deletion of the idea by
//! its author and the 12-month purge, and is deleted one year after the escalation by the daily
//! purge ([`purge`]). Admin only: every read is logged in `admin_actions` by the caller; never
//! public, never exported.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Seals the content of escalation `escalation_id` (one copy per escalation). Returns whether a
/// copy was written.
pub async fn seal(conn: &mut PgConnection, escalation_id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "INSERT INTO escalation_seals (escalation_id, idea_id, escalated_at, purge_after, content)
         SELECT e.id, e.idea_id, e.created_at, e.created_at + interval '1 year',
                jsonb_build_object(
                    'idea', jsonb_build_object(
                        'status', i.status, 'slug', i.slug, 'title', i.title,
                        'summary', i.summary, 'original_text', i.original_text,
                        'submitted_territory', i.submitted_territory,
                        'submitted_admin_level', i.submitted_admin_level,
                        'submitted_at', i.created_at, 'published_at', i.published_at,
                        'hidden_at', i.hidden_at, 'hidden_reason', i.hidden_reason),
                    'analysis', (SELECT a.content FROM analyses a WHERE a.id = i.current_analysis_id),
                    'review', (SELECT jsonb_build_object(
                                   'outcome', rr.outcome, 'escalate', rr.escalate,
                                   'explanation', rr.explanation, 'model', rr.model,
                                   'prompt_version', rr.prompt_version,
                                   'reviewed_at', rr.created_at)
                               FROM report_reviews rr WHERE rr.id = e.review_id),
                    'reports', coalesce((SELECT jsonb_agg(jsonb_build_object(
                                             'reason', r.reason, 'details', r.details,
                                             'reported_at', r.created_at)
                                             ORDER BY r.created_at, r.id)
                                         FROM reports r
                                         WHERE e.review_id IS NOT NULL
                                           AND r.review_id = e.review_id),
                                        '[]'::jsonb))
         FROM escalations e JOIN ideas i ON i.id = e.idea_id
         WHERE e.id = $1
         ON CONFLICT (escalation_id) DO NOTHING",
    )
    .bind(escalation_id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// A sealed copy without its content (listing).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SealSummaryRow {
    pub escalation_id: Uuid,
    pub idea_id: Uuid,
    pub escalated_at: DateTime<Utc>,
    pub purge_after: DateTime<Utc>,
    pub sealed_at: DateTime<Utc>,
    /// `pending`, `done`, or `None` once the escalation row is gone.
    pub escalation_status: Option<String>,
    /// `None` once the idea row is gone.
    pub idea_status: Option<String>,
}

/// Sealed copies, most recent escalation first, with their total.
pub async fn list(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<SealSummaryRow>, i64)> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM escalation_seals")
        .fetch_one(pool)
        .await?;
    let rows = sqlx::query_as(
        "SELECT s.escalation_id, s.idea_id, s.escalated_at, s.purge_after,
                s.created_at AS sealed_at, e.status AS escalation_status, i.status AS idea_status
         FROM escalation_seals s
         LEFT JOIN escalations e ON e.id = s.escalation_id
         LEFT JOIN ideas i ON i.id = s.idea_id
         ORDER BY s.escalated_at DESC, s.id
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}

/// A sealed copy with its content.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SealRow {
    pub id: Uuid,
    pub escalation_id: Uuid,
    pub idea_id: Uuid,
    pub escalated_at: DateTime<Utc>,
    pub purge_after: DateTime<Utc>,
    pub sealed_at: DateTime<Utc>,
    pub content: Value,
}

/// The sealed copy of escalation `escalation_id`, if it is still kept.
pub async fn get(conn: &mut PgConnection, escalation_id: Uuid) -> sqlx::Result<Option<SealRow>> {
    sqlx::query_as(
        "SELECT id, escalation_id, idea_id, escalated_at, purge_after, created_at AS sealed_at,
                content
         FROM escalation_seals WHERE escalation_id = $1",
    )
    .bind(escalation_id)
    .fetch_optional(conn)
    .await
}

/// Deletes the copies one year after their escalation. Returns how many.
pub async fn purge(conn: &mut PgConnection) -> sqlx::Result<i64> {
    let done = sqlx::query("DELETE FROM escalation_seals WHERE purge_after <= now()")
        .execute(conn)
        .await?;
    Ok(done.rows_affected() as i64)
}
