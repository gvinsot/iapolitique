//! Rights of reply (`rights_of_reply`, LCEN art. 1-1 III, AGENTS.md §7.5): written by the
//! administrator, published under the idea, erased with it.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A right of reply as edited by the administrator.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ReplyRow {
    pub id: Uuid,
    pub idea_id: Uuid,
    pub author_label: String,
    pub body: String,
    pub requested_on: NaiveDate,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Content of a right of reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyContent<'a> {
    pub author_label: &'a str,
    pub body: &'a str,
    pub requested_on: NaiveDate,
}

/// Replies of an idea, oldest first.
pub async fn of_idea<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<Vec<ReplyRow>> {
    sqlx::query_as(
        "SELECT id, idea_id, author_label, body, requested_on, published_at, updated_at
         FROM rights_of_reply WHERE idea_id = $1 ORDER BY published_at, id",
    )
    .bind(idea_id)
    .fetch_all(executor)
    .await
}

/// Publishes a reply under an idea.
pub async fn insert(
    conn: &mut PgConnection,
    idea_id: Uuid,
    content: &ReplyContent<'_>,
) -> sqlx::Result<ReplyRow> {
    sqlx::query_as(
        "INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on)
         VALUES ($1, $2, $3, $4)
         RETURNING id, idea_id, author_label, body, requested_on, published_at, updated_at",
    )
    .bind(idea_id)
    .bind(content.author_label)
    .bind(content.body)
    .bind(content.requested_on)
    .fetch_one(conn)
    .await
}

/// Replaces the content of a reply; `None` when it does not exist.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    content: &ReplyContent<'_>,
) -> sqlx::Result<Option<ReplyRow>> {
    sqlx::query_as(
        "UPDATE rights_of_reply SET author_label = $2, body = $3, requested_on = $4
         WHERE id = $1
         RETURNING id, idea_id, author_label, body, requested_on, published_at, updated_at",
    )
    .bind(id)
    .bind(content.author_label)
    .bind(content.body)
    .bind(content.requested_on)
    .fetch_optional(conn)
    .await
}

/// Deletes a reply; returns the idea it was attached to, `None` when it did not exist.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("DELETE FROM rights_of_reply WHERE id = $1 RETURNING idea_id")
        .bind(id)
        .fetch_optional(conn)
        .await
}
