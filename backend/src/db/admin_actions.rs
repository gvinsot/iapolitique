//! Journal of the administrator's actions (`admin_actions`, AGENTS.md §8): every mutation of
//! the admin API is recorded in the transaction that applies it. Details hold internal
//! identifiers, settings values and the administrator's own notes, never a visitor's text.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};

/// An action to record. `action` is a dotted snake_case name (`idea.hide`, `job.retry`…).
#[derive(Debug, Clone, PartialEq)]
pub struct NewAdminAction {
    pub action: &'static str,
    pub target_kind: Option<&'static str>,
    pub target_id: Option<String>,
    /// JSON object.
    pub details: Value,
}

impl NewAdminAction {
    pub fn new(action: &'static str) -> Self {
        debug_assert!(
            action
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'),
            "admin action names are dotted snake_case"
        );
        Self {
            action,
            target_kind: None,
            target_id: None,
            details: Value::Object(serde_json::Map::new()),
        }
    }

    pub fn target(mut self, kind: &'static str, id: impl ToString) -> Self {
        self.target_kind = Some(kind);
        self.target_id = Some(id.to_string());
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        debug_assert!(details.is_object(), "admin action details are JSON objects");
        self.details = details;
        self
    }
}

/// Records an action in the caller's transaction.
pub async fn record(conn: &mut PgConnection, action: &NewAdminAction) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO admin_actions (action, target_kind, target_id, details)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(action.action)
    .bind(action.target_kind)
    .bind(action.target_id.as_deref())
    .bind(&action.details)
    .execute(conn)
    .await
    .map(|_| ())
}

/// A recorded action.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct AdminActionRow {
    pub id: i64,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

/// Most recent actions first, optionally of one action name or of one target.
pub async fn list(
    pool: &PgPool,
    action: Option<&str>,
    target_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<AdminActionRow>, i64)> {
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM admin_actions
         WHERE ($1::text IS NULL OR action = $1) AND ($2::text IS NULL OR target_id = $2)",
    )
    .bind(action)
    .bind(target_id)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query_as(
        "SELECT id, action, target_kind, target_id, details, created_at FROM admin_actions
         WHERE ($1::text IS NULL OR action = $1) AND ($2::text IS NULL OR target_id = $2)
         ORDER BY id DESC LIMIT $3 OFFSET $4",
    )
    .bind(action)
    .bind(target_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows, total))
}
