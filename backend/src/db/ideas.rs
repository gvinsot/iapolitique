//! Changes of an idea that touch several columns or rows at once (AGENTS.md §5.2, §5.4): the
//! status change and the columns that its CHECK constraints tie to it are written by the same
//! statement, the history is appended in the same transaction.

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
