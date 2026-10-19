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

/// Most rights of reply listed for an idea (they are written by the administrator only).
pub const MAX_REPLIES: i64 = 200;

/// Content of a right of reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyContent<'a> {
    pub author_label: &'a str,
    pub body: &'a str,
    pub requested_on: NaiveDate,
}

/// Replies of an idea, oldest first, at most [`MAX_REPLIES`] (§14.5: every list is bounded).
pub async fn of_idea<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<Vec<ReplyRow>> {
    sqlx::query_as(
        "SELECT id, idea_id, author_label, body, requested_on, published_at, updated_at
         FROM rights_of_reply WHERE idea_id = $1 ORDER BY published_at, id LIMIT $2",
    )
    .bind(idea_id)
    .bind(MAX_REPLIES)
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reply_lists_are_bounded(pool: PgPool) {
        let idea = crate::test_support::published_idea(&pool, "idee-bornee").await;
        sqlx::query(
            "INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on)
             SELECT $1, 'Auteur ' || n, 'Réponse ' || n, current_date
             FROM generate_series(1, $2) AS n",
        )
        .bind(idea)
        .bind(i32::try_from(MAX_REPLIES).unwrap() + 5)
        .execute(&pool)
        .await
        .unwrap();
        let admin = of_idea(&pool, idea).await.unwrap();
        assert_eq!(admin.len() as i64, MAX_REPLIES);
        let public = crate::db::portal::replies(&pool, idea).await.unwrap();
        assert_eq!(public.len() as i64, MAX_REPLIES);
    }
}
