//! Human moderation queues of the admin (AGENTS.md §5.6, §7.1, §7.5, §8): reports, escalations
//! of serious content and human review requests.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::status::ReportStatus;
use crate::domain::types::{
    EscalationOutcome, ReportReason, ReviewRequestStatus, ReviewResolution,
};

// ---------------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------------

/// A report with its idea and the review that handled it.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ReportRow {
    pub id: Uuid,
    pub idea_id: Uuid,
    pub idea_slug: Option<String>,
    pub idea_title: Option<String>,
    pub idea_status: String,
    pub reason: String,
    /// Masked details (`None` once purged or when none were given).
    pub details: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub rejection_reason_code: Option<String>,
    pub reporter_name: Option<String>,
    pub reporter_email: Option<String>,
    pub good_faith: bool,
    pub review_id: Option<Uuid>,
    pub review_outcome: Option<String>,
    pub review_escalate: Option<bool>,
    pub review_explanation: Option<String>,
    pub review_corrections: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

/// Filters of the report list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportFilter {
    pub status: Option<ReportStatus>,
    pub reason: Option<ReportReason>,
    pub idea_id: Option<Uuid>,
}

macro_rules! report_filter {
    () => {
        " WHERE ($1::text IS NULL OR r.status = $1) AND ($2::text IS NULL OR r.reason = $2)
            AND ($3::uuid IS NULL OR r.idea_id = $3)"
    };
}

/// Reports, most recent first, with their total.
pub async fn reports(
    pool: &PgPool,
    filter: ReportFilter,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<ReportRow>, i64)> {
    let status = filter.status.map(|s| s.as_str());
    let reason = filter.reason.map(ReportReason::as_str);
    let total: i64 =
        sqlx::query_scalar(concat!("SELECT count(*) FROM reports r", report_filter!()))
            .bind(status)
            .bind(reason)
            .bind(filter.idea_id)
            .fetch_one(pool)
            .await?;
    let rows = sqlx::query_as(concat!(
        "SELECT r.id, r.idea_id, i.slug AS idea_slug, i.title AS idea_title,
                i.status AS idea_status, r.reason, r.details, r.status, r.created_at,
                r.rejection_reason_code, r.reporter_name, r.reporter_email, r.good_faith,
                rr.id AS review_id, rr.outcome AS review_outcome, rr.escalate AS review_escalate,
                rr.explanation AS review_explanation, rr.corrections AS review_corrections,
                rr.created_at AS reviewed_at
         FROM reports r JOIN ideas i ON i.id = r.idea_id
         LEFT JOIN report_reviews rr ON rr.id = r.review_id",
        report_filter!(),
        " ORDER BY r.created_at DESC, r.id DESC LIMIT $4 OFFSET $5"
    ))
    .bind(status)
    .bind(reason)
    .bind(filter.idea_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}

// ---------------------------------------------------------------------------------------------
// Escalations
// ---------------------------------------------------------------------------------------------

/// An escalation of serious content with its idea and the review that raised it.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct EscalationRow {
    pub id: Uuid,
    pub idea_id: Uuid,
    pub idea_slug: Option<String>,
    pub idea_title: Option<String>,
    pub idea_status: String,
    pub status: String,
    pub outcome: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub handled_at: Option<DateTime<Utc>>,
    pub review_id: Option<Uuid>,
    pub review_explanation: Option<String>,
}

macro_rules! escalation_select {
    () => {
        "SELECT e.id, e.idea_id, i.slug AS idea_slug, i.title AS idea_title,
                i.status AS idea_status, e.status, e.outcome, e.note, e.created_at, e.handled_at,
                rr.id AS review_id, rr.explanation AS review_explanation
         FROM escalations e JOIN ideas i ON i.id = e.idea_id
         LEFT JOIN report_reviews rr ON rr.id = e.review_id "
    };
}

/// Escalations, pending ones first (oldest first: the 24-hour target runs), then handled ones
/// (most recent first), with their total.
pub async fn escalations(
    pool: &PgPool,
    status: Option<ReviewRequestStatus>,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<EscalationRow>, i64)> {
    let status = status.map(ReviewRequestStatus::as_str);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM escalations e WHERE ($1::text IS NULL OR e.status = $1)",
    )
    .bind(status)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query_as(concat!(
        escalation_select!(),
        "WHERE ($1::text IS NULL OR e.status = $1)
         ORDER BY (e.status = 'pending') DESC,
                  CASE WHEN e.status = 'pending' THEN e.created_at END ASC,
                  e.handled_at DESC NULLS LAST, e.id
         LIMIT $2 OFFSET $3"
    ))
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}

pub async fn escalation(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<EscalationRow>> {
    sqlx::query_as(concat!(escalation_select!(), "WHERE e.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Why an escalation could not be closed.
#[derive(Debug, PartialEq, Eq)]
pub enum CloseError {
    NotFound,
    AlreadyDone,
}

/// Closes a pending escalation with its outcome and the administrator's note.
pub async fn close_escalation(
    conn: &mut PgConnection,
    id: Uuid,
    outcome: EscalationOutcome,
    note: Option<&str>,
) -> sqlx::Result<Result<(), CloseError>> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM escalations WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    match status.as_deref() {
        None => return Ok(Err(CloseError::NotFound)),
        Some(status) if status != ReviewRequestStatus::Pending.as_str() => {
            return Ok(Err(CloseError::AlreadyDone));
        }
        Some(_) => {}
    }
    sqlx::query(
        "UPDATE escalations SET status = 'done', handled_at = now(), outcome = $2, note = $3
         WHERE id = $1",
    )
    .bind(id)
    .bind(outcome.as_str())
    .bind(note)
    .execute(&mut *conn)
    .await?;
    Ok(Ok(()))
}

// ---------------------------------------------------------------------------------------------
// Human review requests
// ---------------------------------------------------------------------------------------------

/// A human review request with the state of the text it concerns.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ReviewRequestRow {
    pub id: Uuid,
    pub target_kind: String,
    pub target_id: Uuid,
    /// Masked message of the author.
    pub message: Option<String>,
    pub status: String,
    pub resolution: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub handled_at: Option<DateTime<Utc>>,
    pub idea_id: Option<Uuid>,
    pub idea_slug: Option<String>,
    pub idea_title: Option<String>,
    pub idea_status: Option<String>,
    /// Current status of the text (`None` if it no longer exists).
    pub target_status: Option<String>,
    /// Masked text concerned (idea, contribution, report details).
    pub text: Option<String>,
    pub rejection_reason_code: Option<String>,
    pub hidden_reason: Option<String>,
}

macro_rules! review_request_select {
    () => {
        "SELECT q.id, q.target_kind, q.target_id, q.message, q.status, q.resolution, q.note,
                q.created_at, q.handled_at, p.id AS idea_id, p.slug AS idea_slug,
                p.title AS idea_title, p.status AS idea_status,
                coalesce(i.status, c.status, r.status) AS target_status,
                coalesce(i.original_text, c.text, r.details) AS text,
                coalesce(i.rejection_reason_code, c.rejection_reason_code,
                         r.rejection_reason_code) AS rejection_reason_code,
                i.hidden_reason
         FROM review_requests q
         LEFT JOIN ideas i ON q.target_kind = 'idea' AND i.id = q.target_id
         LEFT JOIN contributions c ON q.target_kind = 'contribution' AND c.id = q.target_id
         LEFT JOIN reports r ON q.target_kind = 'report' AND r.id = q.target_id
         LEFT JOIN ideas p ON p.id = coalesce(i.id, c.idea_id, r.idea_id) "
    };
}

/// Review requests, pending ones first (oldest first), with their total.
pub async fn review_requests(
    pool: &PgPool,
    status: Option<ReviewRequestStatus>,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<ReviewRequestRow>, i64)> {
    let status = status.map(ReviewRequestStatus::as_str);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM review_requests q WHERE ($1::text IS NULL OR q.status = $1)",
    )
    .bind(status)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query_as(concat!(
        review_request_select!(),
        "WHERE ($1::text IS NULL OR q.status = $1)
         ORDER BY (q.status = 'pending') DESC,
                  CASE WHEN q.status = 'pending' THEN q.created_at END ASC,
                  q.handled_at DESC NULLS LAST, q.id
         LIMIT $2 OFFSET $3"
    ))
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}

pub async fn review_request(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<ReviewRequestRow>> {
    sqlx::query_as(concat!(review_request_select!(), "WHERE q.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Locks a review request: its target and whether it is still pending.
pub async fn lock_review_request(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<(String, Uuid, bool)>> {
    sqlx::query_as(
        "SELECT target_kind, target_id, status = 'pending' FROM review_requests
         WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Closes a locked pending review request.
pub async fn resolve_review_request(
    conn: &mut PgConnection,
    id: Uuid,
    resolution: ReviewResolution,
    note: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE review_requests SET status = 'done', handled_at = now(), resolution = $2,
             note = $3
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(resolution.as_str())
    .bind(note)
    .execute(conn)
    .await
    .map(|_| ())
}
