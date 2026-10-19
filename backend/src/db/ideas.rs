//! Changes of an idea that touch several columns or rows at once (AGENTS.md §5.2, §5.4): the
//! status change and the columns that its CHECK constraints tie to it are written by the same
//! statement, the history is appended in the same transaction.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::status::{self, Change, StatusError};
use crate::domain::status::{ContributionStatus, IdeaStatus, StateMachine};
use crate::domain::types::{ContributionKind, HiddenReason};

/// Merges a duplicate into the published idea `parent` (`dedup → merged`, §5.4): the idea
/// points to its parent, becomes an accepted contribution of kind `doublon` of the parent (its
/// text stays on the merged idea and is given to the next analysis of the parent), and the
/// parent's proposal counter goes up. Returns the id of the contribution.
///
/// The caller holds the lock of the parent and checked that it is published.
pub async fn merge(
    conn: &mut PgConnection,
    idea_id: Uuid,
    parent_id: Uuid,
    change: &Change,
) -> Result<Uuid, StatusError> {
    let from = status::lock::<IdeaStatus>(conn, idea_id)
        .await?
        .ok_or(StatusError::NotFound {
            entity: IdeaStatus::ENTITY,
            id: idea_id,
        })?;
    let to = from.transition(IdeaStatus::Merged)?;
    // `ideas_merged_has_parent`: the status and the parent change together.
    sqlx::query(
        "UPDATE ideas SET status = $3, status_changed_at = now(), parent_idea_id = $2
         WHERE id = $1",
    )
    .bind(idea_id)
    .bind(parent_id)
    .bind(to.as_str())
    .execute(&mut *conn)
    .await?;
    status::append_event(conn, idea_id, Some(from), to, change).await?;

    let contribution_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, status, source_idea_id)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(parent_id)
    .bind(ContributionKind::Duplicate.as_str())
    .bind(ContributionStatus::Accepted.as_str())
    .bind(idea_id)
    .fetch_one(&mut *conn)
    .await?;
    status::record_creation_as(conn, contribution_id, ContributionStatus::Accepted, change).await?;
    sqlx::query("UPDATE ideas SET proposal_count = proposal_count + 1 WHERE id = $1")
        .bind(parent_id)
        .execute(&mut *conn)
        .await?;
    Ok(contribution_id)
}

/// Hides a published idea (`published → hidden`) with its reason; the day of the masking is
/// kept for the statement of reasons and the 12-month purge (§7.3). Returns `false` (nothing
/// changed) when the idea is not published.
pub async fn hide(
    conn: &mut PgConnection,
    idea_id: Uuid,
    reason: HiddenReason,
    change: &Change,
) -> Result<bool, StatusError> {
    let Some(from) = status::lock::<IdeaStatus>(conn, idea_id).await? else {
        return Ok(false);
    };
    if from != IdeaStatus::Published {
        return Ok(false);
    }
    let to = from.transition(IdeaStatus::Hidden)?;
    sqlx::query(
        "UPDATE ideas SET status = $2, status_changed_at = now(), hidden_at = now(),
             hidden_reason = $3
         WHERE id = $1",
    )
    .bind(idea_id)
    .bind(to.as_str())
    .bind(reason.as_str())
    .execute(&mut *conn)
    .await?;
    status::append_event(conn, idea_id, Some(from), to, change).await?;
    Ok(true)
}

/// Republishes a hidden idea (`hidden → published`, §8): the masking fields are erased, the
/// idea and its current analysis are visible again. Returns `false` (nothing changed) when the
/// idea is not hidden.
pub async fn republish(
    conn: &mut PgConnection,
    idea_id: Uuid,
    change: &Change,
) -> Result<bool, StatusError> {
    if status::lock::<IdeaStatus>(conn, idea_id).await? != Some(IdeaStatus::Hidden) {
        return Ok(false);
    }
    status::transition(conn, idea_id, IdeaStatus::Published, change).await?;
    sqlx::query("UPDATE ideas SET hidden_at = NULL, hidden_reason = NULL WHERE id = $1")
        .bind(idea_id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// An idea as listed by the admin (any status).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct AdminIdeaRow {
    pub id: Uuid,
    pub status: String,
    pub slug: Option<String>,
    pub title: Option<String>,
    /// Beginning of the masked original text (`None` once purged).
    pub excerpt: Option<String>,
    pub submitted_admin_level: Option<String>,
    pub created_at: DateTime<Utc>,
    pub status_changed_at: DateTime<Utc>,
    pub rejection_reason_code: Option<String>,
    pub hidden_reason: Option<String>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub parent_idea_id: Option<Uuid>,
    pub proposal_count: i32,
    pub contribution_count: i32,
    pub score_eur: Option<i64>,
    pub reports_pending: i64,
    pub review_request_pending: bool,
    pub escalation_pending: bool,
    pub withdrawn_at: Option<DateTime<Utc>>,
}

macro_rules! admin_idea_select {
    () => {
        "SELECT i.id, i.status, i.slug, i.title, left(i.original_text, 280) AS excerpt,
                i.submitted_admin_level, i.created_at, i.status_changed_at,
                i.rejection_reason_code, i.hidden_reason, i.hidden_at, i.parent_idea_id,
                i.proposal_count, i.contribution_count, i.score_eur,
                (SELECT count(*) FROM reports r WHERE r.idea_id = i.id AND r.status = 'accepted')
                    AS reports_pending,
                EXISTS (SELECT 1 FROM review_requests q WHERE q.target_kind = 'idea'
                        AND q.target_id = i.id AND q.status = 'pending') AS review_request_pending,
                EXISTS (SELECT 1 FROM escalations e WHERE e.idea_id = i.id
                        AND e.status = 'pending') AS escalation_pending,
                i.withdrawn_at
         FROM ideas i "
    };
}

/// `$1` status, `$2` the search (matched with `strpos`, no metacharacter, INJ-5), `$3` the
/// search as a UUID.
macro_rules! admin_idea_filter {
    () => {
        "WHERE ($1::text IS NULL OR i.status = $1)
           AND ($2::text IS NULL OR i.id = $3
                OR strpos(unaccent(lower(concat_ws(' ', i.slug, i.title, i.summary,
                                                   i.original_text))),
                          unaccent(lower($2))) > 0)"
    };
}

/// Ideas of a status and/or matching a search (identifier, slug, title, summary or text),
/// most recently changed first, with their total.
pub async fn admin_list(
    pool: &sqlx::PgPool,
    status: Option<IdeaStatus>,
    search: Option<&str>,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<AdminIdeaRow>, i64)> {
    let status = status.map(IdeaStatus::as_str);
    let id = search.and_then(|s| Uuid::parse_str(s.trim()).ok());
    let total: i64 = sqlx::query_scalar(concat!(
        "SELECT count(*) FROM ideas i ",
        admin_idea_filter!()
    ))
    .bind(status)
    .bind(search)
    .bind(id)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query_as(concat!(
        admin_idea_select!(),
        admin_idea_filter!(),
        " ORDER BY i.status_changed_at DESC, i.id DESC LIMIT $4 OFFSET $5"
    ))
    .bind(status)
    .bind(search)
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}

/// One idea as shown by the admin, `None` when it does not exist.
pub async fn admin_get<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<Option<AdminIdeaRow>> {
    sqlx::query_as(concat!(admin_idea_select!(), "WHERE i.id = $1"))
        .bind(idea_id)
        .fetch_optional(executor)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::Actor;
    use crate::test_support::{idea_with_status, published_idea};
    use serde_json::json;
    use sqlx::PgPool;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn merges_a_duplicate_into_its_parent(pool: PgPool) {
        let parent = published_idea(&pool, "parente").await;
        let idea = idea_with_status(&pool, "dedup").await;
        let change = Change::by(Actor::Worker).details(json!({ "parent_idea_id": parent }));
        let mut tx = pool.begin().await.unwrap();
        let contribution = merge(&mut tx, idea, parent, &change).await.unwrap();
        tx.commit().await.unwrap();

        let (status, parent_of): (String, Option<Uuid>) =
            sqlx::query_as("SELECT status, parent_idea_id FROM ideas WHERE id = $1")
                .bind(idea)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((status.as_str(), parent_of), ("merged", Some(parent)));
        let (count,): (i32,) = sqlx::query_as("SELECT proposal_count FROM ideas WHERE id = $1")
            .bind(parent)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let row: (Uuid, String, String, Option<Uuid>, Option<String>) = sqlx::query_as(
            "SELECT idea_id, kind, status, source_idea_id, text FROM contributions WHERE id = $1",
        )
        .bind(contribution)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                parent,
                "doublon".to_owned(),
                "accepted".to_owned(),
                Some(idea),
                None
            )
        );
        let events: Vec<(Option<String>, String)> = sqlx::query_as(
            "SELECT from_status, to_status FROM contribution_events WHERE contribution_id = $1",
        )
        .bind(contribution)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(events, [(None, "accepted".to_owned())]);
        let (from, to): (Option<String>, String) = sqlx::query_as(
            "SELECT from_status, to_status FROM idea_events WHERE idea_id = $1 ORDER BY id DESC",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((from.as_deref(), to.as_str()), (Some("dedup"), "merged"));

        // Hiding: only a published idea.
        let mut tx = pool.begin().await.unwrap();
        let hidden = hide(&mut tx, parent, HiddenReason::ReportReview, &change)
            .await
            .unwrap();
        assert!(hidden);
        assert!(
            !hide(&mut tx, parent, HiddenReason::Admin, &change)
                .await
                .unwrap()
        );
        assert!(
            !hide(&mut tx, Uuid::nil(), HiddenReason::Admin, &change)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let (status, reason, at): (String, Option<String>, bool) = sqlx::query_as(
            "SELECT status, hidden_reason, hidden_at IS NOT NULL FROM ideas WHERE id = $1",
        )
        .bind(parent)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (status.as_str(), reason.as_deref(), at),
            ("hidden", Some("report_review"), true)
        );

        // Only from `dedup`.
        let other = idea_with_status(&pool, "accepted").await;
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(
            merge(&mut tx, other, parent, &change).await,
            Err(StatusError::Invalid(_))
        ));
    }
}
