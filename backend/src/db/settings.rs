//! `settings`: runtime settings adjustable from the admin (AGENTS.md §8), overriding the
//! defaults of `prompts/` and of the environment. Values are JSON; keys are documented in
//! AGENTS.md §13.6.

use serde_json::Value;
use sqlx::PgExecutor;

/// Key of the screening threshold overrides:
/// `{ "<section>": { "<criterion key>": { "reject_below"?: f64, "accept_above"?: f64 } } }`.
pub const SCREENING_THRESHOLDS: &str = "screening.thresholds";

/// Value of a setting, `None` when it was never set.
pub async fn get<'e>(executor: impl PgExecutor<'e>, key: &str) -> sqlx::Result<Option<Value>> {
    let row: Option<(Value,)> = sqlx::query_as("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(executor)
        .await?;
    Ok(row.map(|(value,)| value))
}

/// Creates or replaces a setting.
pub async fn set<'e>(executor: impl PgExecutor<'e>, key: &str, value: &Value) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(value)
    .execute(executor)
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn get_and_set(pool: PgPool) {
        assert_eq!(get(&pool, SCREENING_THRESHOLDS).await.unwrap(), None);
        let value = serde_json::json!({ "idea": {} });
        set(&pool, SCREENING_THRESHOLDS, &value).await.unwrap();
        set(&pool, SCREENING_THRESHOLDS, &value).await.unwrap();
        assert_eq!(get(&pool, SCREENING_THRESHOLDS).await.unwrap(), Some(value));
    }
}
