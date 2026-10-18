//! What the tracking page reads and does (AGENTS.md §7.1): the state of a tracked text, its
//! deletion by its author and the human review request.
//!
//! # Deletion
//!
//! The deletion erases at once, in one transaction:
//!
//! - **idea**: the idea row with everything attached to it (history, analyses, embedding,
//!   contributions and reports of other visitors, duplicates merged into it, with their
//!   tracking tokens, which then answer like unknown tokens); a merged duplicate also leaves the
//!   idea it was merged into (whose proposal counter goes down). A published or masked idea
//!   leaves the site at once and the open data exports are regenerated; copies already
//!   downloaded cannot be recalled (§7.4).
//! - **contribution**: the contribution row and its history; its neutral summary is removed from
//!   the analyses of the idea, and the exports are regenerated if it had been taken into account.
//! - **report**: the report row, its history and the reporter's contact.
//!
//! In every case the pending review request, the queued jobs about the erased texts and the
//! second opinion explanations (which may paraphrase them) go too; the decision model scores
//! stay, without text, for calibration (§7.3). The author's token becomes a tombstone that
//! confirms the deletion for 30 days.

use chrono::NaiveDate;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::tracking::{self, TokenHash};
use crate::domain::types::{ReviewOutcome, ReviewRequestStatus, TargetKind};
use crate::jobs::{JobKind, NewJob, queue};
use crate::moderation::Target;

/// State of a tracked text.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TrackedView {
    pub status: String,
    /// Day of the submission (Europe/Paris).
    pub submitted_on: NaiveDate,
    /// Published idea concerned: the idea itself, the idea a duplicate was merged into, or the
    /// idea of a contribution or a report. `None` while it is not published.
    pub idea_slug: Option<String>,
    pub idea_title: Option<String>,
    /// Outcome of the review of a report.
    pub report_outcome: Option<String>,
}

impl TrackedView {
    pub fn report_outcome(&self) -> Option<ReviewOutcome> {
        self.report_outcome.as_deref().and_then(|o| o.parse().ok())
    }
}

const IDEA_VIEW_SQL: &str = "SELECT i.status,
        (i.created_at AT TIME ZONE 'Europe/Paris')::date AS submitted_on,
        coalesce(CASE WHEN i.status = 'published' THEN i.slug END, p.slug) AS idea_slug,
        coalesce(CASE WHEN i.status = 'published' THEN i.title END, p.title) AS idea_title,
        NULL::text AS report_outcome
    FROM ideas i
    LEFT JOIN ideas p ON p.id = i.parent_idea_id AND p.status = 'published'
    WHERE i.id = $1";

const CONTRIBUTION_VIEW_SQL: &str = "SELECT c.status,
        (c.created_at AT TIME ZONE 'Europe/Paris')::date AS submitted_on,
        CASE WHEN i.status = 'published' THEN i.slug END AS idea_slug,
        CASE WHEN i.status = 'published' THEN i.title END AS idea_title,
        NULL::text AS report_outcome
    FROM contributions c JOIN ideas i ON i.id = c.idea_id
    WHERE c.id = $1";

const REPORT_VIEW_SQL: &str = "SELECT r.status,
        (r.created_at AT TIME ZONE 'Europe/Paris')::date AS submitted_on,
        CASE WHEN i.status = 'published' THEN i.slug END AS idea_slug,
        CASE WHEN i.status = 'published' THEN i.title END AS idea_title,
        rr.outcome AS report_outcome
    FROM reports r
    JOIN ideas i ON i.id = r.idea_id
    LEFT JOIN report_reviews rr ON rr.id = r.review_id
    WHERE r.id = $1";

/// State of a tracked text, `None` when it no longer exists.
pub async fn view(pool: &PgPool, target: Target) -> sqlx::Result<Option<TrackedView>> {
    let sql = match target.kind {
        TargetKind::Idea => IDEA_VIEW_SQL,
        TargetKind::Contribution => CONTRIBUTION_VIEW_SQL,
        TargetKind::Report => REPORT_VIEW_SQL,
    };
    sqlx::query_as(sql)
        .bind(target.id)
        .fetch_optional(pool)
        .await
}

/// The human review request of a text: day of the request and state.
pub async fn review_request(
    pool: &PgPool,
    target: Target,
) -> sqlx::Result<Option<(NaiveDate, ReviewRequestStatus)>> {
    let row: Option<(NaiveDate, String)> = sqlx::query_as(
        "SELECT (created_at AT TIME ZONE 'Europe/Paris')::date, status FROM review_requests
         WHERE target_kind = $1 AND target_id = $2",
    )
    .bind(target.kind.as_str())
    .bind(target.id)
    .fetch_optional(pool)
    .await?;
    row.map(|(day, status)| {
        let status = status
            .parse()
            .map_err(|err: crate::domain::UnknownVariant| sqlx::Error::Decode(err.into()))?;
        Ok((day, status))
    })
    .transpose()
}

/// Stores a human review request; `None` when one already exists for the text. The caller
/// checks that the text is the subject of an automated decision and masks the message.
pub async fn insert_review_request(
    pool: &PgPool,
    target: Target,
    message: Option<&str>,
) -> sqlx::Result<Option<NaiveDate>> {
    sqlx::query_scalar(
        "INSERT INTO review_requests (target_kind, target_id, message) VALUES ($1, $2, $3)
         ON CONFLICT (target_kind, target_id) DO NOTHING
         RETURNING (created_at AT TIME ZONE 'Europe/Paris')::date",
    )
    .bind(target.kind.as_str())
    .bind(target.id)
    .bind(message)
    .fetch_optional(pool)
    .await
}

/// Erases what refers to texts about to be deleted: review requests, second opinion
/// explanations and queued jobs. `ids` are of kind `kind`.
async fn erase_traces(conn: &mut PgConnection, kind: TargetKind, ids: &[Uuid]) -> sqlx::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query("DELETE FROM review_requests WHERE target_kind = $1 AND target_id = ANY($2)")
        .bind(kind.as_str())
        .bind(ids)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "UPDATE screenings SET second_opinion_explanation = NULL
         WHERE target_kind = $1 AND target_id = ANY($2)
           AND second_opinion_explanation IS NOT NULL",
    )
    .bind(kind.as_str())
    .bind(ids)
    .execute(&mut *conn)
    .await?;
    let texts: Vec<String> = ids.iter().map(Uuid::to_string).collect();
    sqlx::query(
        "DELETE FROM jobs WHERE status = 'queued' AND payload->>'target_kind' = $1
           AND payload->>'target_id' = ANY($2)",
    )
    .bind(kind.as_str())
    .bind(&texts)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Deletes the tokens of texts deleted with another one (their authors did not ask: their
/// tokens answer like unknown ones).
async fn delete_tokens(
    conn: &mut PgConnection,
    kind: TargetKind,
    ids: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tracking_tokens WHERE target_kind = $1 AND target_id = ANY($2)")
        .bind(kind.as_str())
        .bind(ids)
        .execute(&mut *conn)
        .await
        .map(|_| ())
}

/// Regeneration of the open data exports, after the removal of published content (§7.4).
fn exports_job() -> NewJob {
    NewJob::new(JobKind::GenerateExports).dedupe_key("generate_exports")
}

async fn ids(conn: &mut PgConnection, sql: &'static str, id: Uuid) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(sql).bind(id).fetch_all(&mut *conn).await
}

async fn delete_idea(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    let row: Option<(String, Option<Uuid>)> =
        sqlx::query_as("SELECT status, parent_idea_id FROM ideas WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((status, parent)) = row else {
        return Ok(());
    };
    let contributions = ids(conn, "SELECT id FROM contributions WHERE idea_id = $1", id).await?;
    let reports = ids(conn, "SELECT id FROM reports WHERE idea_id = $1", id).await?;
    let duplicates = ids(conn, "SELECT id FROM ideas WHERE parent_idea_id = $1", id).await?;
    let mut idea_ids = duplicates.clone();
    idea_ids.push(id);

    erase_traces(conn, TargetKind::Idea, &idea_ids).await?;
    erase_traces(conn, TargetKind::Contribution, &contributions).await?;
    erase_traces(conn, TargetKind::Report, &reports).await?;
    sqlx::query("DELETE FROM jobs WHERE status = 'queued' AND payload->>'idea_id' = $1")
        .bind(id.to_string())
        .execute(&mut *conn)
        .await?;
    delete_tokens(conn, TargetKind::Idea, &duplicates).await?;
    delete_tokens(conn, TargetKind::Contribution, &contributions).await?;
    delete_tokens(conn, TargetKind::Report, &reports).await?;

    // Foreign keys without cascade first: the contributions (including the duplicates merged
    // into this idea, and the one this idea became if it was merged), then the duplicates.
    sqlx::query("DELETE FROM contributions WHERE idea_id = $1 OR source_idea_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM ideas WHERE parent_idea_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM ideas WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if let Some(parent) = parent {
        sqlx::query(
            "UPDATE ideas SET proposal_count = greatest(proposal_count - 1, 1) WHERE id = $1",
        )
        .bind(parent)
        .execute(&mut *conn)
        .await?;
    }
    if matches!(status.as_str(), "published" | "hidden") || parent.is_some() {
        queue::enqueue(&mut *conn, &exports_job()).await?;
    }
    Ok(())
}

async fn delete_contribution(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    let row: Option<(String, Uuid)> =
        sqlx::query_as("SELECT status, idea_id FROM contributions WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((status, idea_id)) = row else {
        return Ok(());
    };
    erase_traces(conn, TargetKind::Contribution, &[id]).await?;
    // The neutral summary written by an analysis goes with the contribution.
    sqlx::query(
        "UPDATE analyses SET content = jsonb_set(content, '{contributions_considered}',
             coalesce((SELECT jsonb_agg(e) FROM jsonb_array_elements(content->'contributions_considered') e
                       WHERE e->>'contribution_id' IS DISTINCT FROM $2), '[]'::jsonb))
         WHERE idea_id = $1 AND jsonb_typeof(content->'contributions_considered') = 'array'",
    )
    .bind(idea_id)
    .bind(id.to_string())
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM contributions WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if status == "integrated" {
        queue::enqueue(&mut *conn, &exports_job()).await?;
    }
    Ok(())
}

async fn delete_report(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    erase_traces(conn, TargetKind::Report, &[id]).await?;
    sqlx::query("DELETE FROM reports WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map(|_| ())
}

/// Deletes a text at its author's request (see the module documentation) and turns the token
/// `hash` into a tombstone.
pub async fn delete(pool: &PgPool, target: Target, hash: &TokenHash) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    match target.kind {
        TargetKind::Idea => delete_idea(&mut tx, target.id).await?,
        TargetKind::Contribution => delete_contribution(&mut tx, target.id).await?,
        TargetKind::Report => delete_report(&mut tx, target.id).await?,
    }
    tracking::mark_deleted(&mut *tx, hash).await?;
    tx.commit().await?;
    tracing::info!(
        target_kind = target.kind.as_str(),
        target_id = %target.id,
        "text deleted by its author"
    );
    Ok(())
}
