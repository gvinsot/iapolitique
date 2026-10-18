//! Persisted status changes (AGENTS.md §5.2).
//!
//! Every status change of an idea, a contribution or a report goes through [`transition`]: the
//! row is locked, the change is validated by [`StateMachine::transition`] (the single source of
//! truth of the allowed transitions), the status and `status_changed_at` are updated and the
//! change is appended to the history table (`idea_events`, `contribution_events`,
//! `report_events`). Callers pass a connection of a transaction, so that the change, its
//! history and the side effects that depend on it (jobs, rejection fields…) commit together.
//!
//! Table and column names come from the [`StatusTable`] constants, never from input, hence the
//! [`AssertSqlSafe`] wrappers.

use std::str::FromStr;

use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

use crate::domain::UnknownVariant;
use crate::domain::status::{
    ContributionStatus, IdeaStatus, InvalidTransition, ReportStatus, StateMachine,
};
use crate::domain::types::{Actor, TargetKind};

/// A status enum stored in a table with a history table.
pub trait StatusTable: StateMachine + FromStr<Err = UnknownVariant> + Send + Sync {
    /// Table holding the `status` column.
    const TABLE: &'static str;
    /// History table.
    const EVENTS: &'static str;
    /// Column of the history table referencing the entity.
    const FOREIGN_KEY: &'static str;
    /// Kind of the entity, for tracking links and screenings.
    const TARGET: TargetKind;
}

impl StatusTable for IdeaStatus {
    const TABLE: &'static str = "ideas";
    const EVENTS: &'static str = "idea_events";
    const FOREIGN_KEY: &'static str = "idea_id";
    const TARGET: TargetKind = TargetKind::Idea;
}

impl StatusTable for ContributionStatus {
    const TABLE: &'static str = "contributions";
    const EVENTS: &'static str = "contribution_events";
    const FOREIGN_KEY: &'static str = "contribution_id";
    const TARGET: TargetKind = TargetKind::Contribution;
}

impl StatusTable for ReportStatus {
    const TABLE: &'static str = "reports";
    const EVENTS: &'static str = "report_events";
    const FOREIGN_KEY: &'static str = "report_id";
    const TARGET: TargetKind = TargetKind::Report;
}

/// Who changes the status, why, and internal details (identifiers only, never user text).
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub actor: Actor,
    pub reason_code: Option<String>,
    /// JSON object.
    pub details: Value,
}

impl Change {
    pub fn by(actor: Actor) -> Self {
        Self {
            actor,
            reason_code: None,
            details: json!({}),
        }
    }

    pub fn reason(mut self, code: impl Into<String>) -> Self {
        self.reason_code = Some(code.into());
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        debug_assert!(details.is_object(), "event details are JSON objects");
        self.details = details;
        self
    }
}

/// A status change that could not be applied.
#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("{entity} {id} not found")]
    NotFound { entity: &'static str, id: Uuid },
    #[error(transparent)]
    Invalid(#[from] InvalidTransition),
    /// The database holds a status the code does not know (newer schema).
    #[error("{0}")]
    Unknown(#[from] UnknownVariant),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Locks the row of the entity (`FOR UPDATE`) and returns its status, `None` when it does not
/// exist.
pub async fn lock<S: StatusTable>(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<S>, StatusError> {
    let row: Option<(String,)> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT status FROM {} WHERE id = $1 FOR UPDATE",
        S::TABLE
    )))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some((status,)) => Ok(Some(status.parse()?)),
        None => Ok(None),
    }
}

async fn append_event<S: StatusTable>(
    conn: &mut PgConnection,
    id: Uuid,
    from: Option<S>,
    to: S,
    change: &Change,
) -> sqlx::Result<()> {
    sqlx::query(AssertSqlSafe(format!(
        "INSERT INTO {} ({}, from_status, to_status, actor, reason_code, details)
         VALUES ($1, $2, $3, $4, $5, $6)",
        S::EVENTS,
        S::FOREIGN_KEY
    )))
    .bind(id)
    .bind(from.map(StateMachine::label))
    .bind(to.label())
    .bind(change.actor.as_str())
    .bind(change.reason_code.as_deref())
    .bind(&change.details)
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Records the creation of an entity, in its initial status, in the history.
pub async fn record_creation<S: StatusTable>(
    conn: &mut PgConnection,
    id: Uuid,
    change: &Change,
) -> sqlx::Result<()> {
    append_event::<S>(conn, id, None, S::INITIAL, change).await
}

/// Moves the entity to `to` and records the change. Returns the previous status.
pub async fn transition<S: StatusTable>(
    conn: &mut PgConnection,
    id: Uuid,
    to: S,
    change: &Change,
) -> Result<S, StatusError> {
    let from = lock::<S>(conn, id).await?.ok_or(StatusError::NotFound {
        entity: S::ENTITY,
        id,
    })?;
    from.transition(to)?;
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE {} SET status = $2, status_changed_at = now() WHERE id = $1",
        S::TABLE
    )))
    .bind(id)
    .bind(to.label())
    .execute(&mut *conn)
    .await?;
    append_event(conn, id, Some(from), to, change).await?;
    Ok(from)
}

/// Applies the transitions of `path` in order (e.g. `submitted → screening → accepted`), each
/// recorded in the history.
pub async fn transition_path<S: StatusTable>(
    conn: &mut PgConnection,
    id: Uuid,
    path: &[S],
    change: &Change,
) -> Result<(), StatusError> {
    for &to in path {
        transition(conn, id, to, change).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn new_idea(pool: &PgPool) -> Uuid {
        let mut tx = pool.begin().await.unwrap();
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO ideas (original_text) VALUES ('Mutualiser les achats.') RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        record_creation::<IdeaStatus>(&mut tx, id, &Change::by(Actor::Author))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        id
    }

    async fn events(pool: &PgPool, table: &str, fk: &str, id: Uuid) -> Vec<Value> {
        let (rows,): (Value,) = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT coalesce(jsonb_agg(jsonb_build_object('from', from_status, 'to', to_status,
                 'actor', actor, 'reason', reason_code, 'details', details) ORDER BY id), '[]')
             FROM {table} WHERE {fk} = $1"
        )))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
        rows.as_array().unwrap().clone()
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn transitions_are_validated_and_recorded(pool: PgPool) {
        let id = new_idea(&pool).await;
        let mut tx = pool.begin().await.unwrap();
        transition_path(
            &mut tx,
            id,
            &[IdeaStatus::Screening, IdeaStatus::Rejected],
            &Change::by(Actor::Api)
                .reason("off_topic")
                .details(json!({ "screening_id": "x" })),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (status,): (String,) = sqlx::query_as("SELECT status FROM ideas WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "rejected");
        assert_eq!(
            events(&pool, "idea_events", "idea_id", id).await,
            vec![
                json!({ "from": null, "to": "submitted", "actor": "author", "reason": null, "details": {} }),
                json!({ "from": "submitted", "to": "screening", "actor": "api", "reason": "off_topic",
                        "details": { "screening_id": "x" } }),
                json!({ "from": "screening", "to": "rejected", "actor": "api", "reason": "off_topic",
                        "details": { "screening_id": "x" } }),
            ]
        );

        // A forbidden transition changes nothing.
        let mut tx = pool.begin().await.unwrap();
        let err = transition(
            &mut tx,
            id,
            IdeaStatus::Published,
            &Change::by(Actor::Admin),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, StatusError::Invalid(_)), "{err}");
        drop(tx);
        assert_eq!(events(&pool, "idea_events", "idea_id", id).await.len(), 3);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn missing_entities_are_reported(pool: PgPool) {
        let mut conn = pool.acquire().await.unwrap();
        let id = Uuid::new_v4();
        assert_eq!(lock::<ReportStatus>(&mut conn, id).await.unwrap(), None);
        let err = transition(
            &mut conn,
            id,
            ContributionStatus::Screening,
            &Change::by(Actor::Api),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, StatusError::NotFound { entity: "contribution", id: missing } if missing == id)
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn contributions_and_reports_have_a_history(pool: PgPool) {
        let idea = new_idea(&pool).await;
        let mut tx = pool.begin().await.unwrap();
        let (contribution,): (Uuid,) = sqlx::query_as(
            "INSERT INTO contributions (idea_id, kind, text) VALUES ($1, 'objection', 'Texte')
             RETURNING id",
        )
        .bind(idea)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        record_creation::<ContributionStatus>(&mut tx, contribution, &Change::by(Actor::Author))
            .await
            .unwrap();
        transition(
            &mut tx,
            contribution,
            ContributionStatus::Screening,
            &Change::by(Actor::Worker),
        )
        .await
        .unwrap();
        let (report,): (Uuid,) = sqlx::query_as(
            "INSERT INTO reports (idea_id, reason) VALUES ($1, 'autre') RETURNING id",
        )
        .bind(idea)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        record_creation::<ReportStatus>(&mut tx, report, &Change::by(Actor::Author))
            .await
            .unwrap();
        let previous = transition(
            &mut tx,
            report,
            ReportStatus::Screening,
            &Change::by(Actor::Api),
        )
        .await
        .unwrap();
        assert_eq!(previous, ReportStatus::Submitted);
        tx.commit().await.unwrap();

        let contribution_events = events(
            &pool,
            "contribution_events",
            "contribution_id",
            contribution,
        )
        .await;
        assert_eq!(contribution_events.len(), 2);
        assert_eq!(contribution_events[1]["to"], "screening");
        assert_eq!(contribution_events[1]["actor"], "worker");
        let report_events = events(&pool, "report_events", "report_id", report).await;
        assert_eq!(report_events[0]["to"], "submitted");
        assert_eq!(report_events[1]["from"], "submitted");

        // History goes with the entity.
        sqlx::query("DELETE FROM ideas WHERE id = $1")
            .bind(idea)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            events(&pool, "report_events", "report_id", report)
                .await
                .is_empty()
        );
    }
}
