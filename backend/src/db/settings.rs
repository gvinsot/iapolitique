//! `settings`: runtime settings adjustable from the admin (AGENTS.md §8), overriding the
//! defaults of `prompts/` and of the environment. Values are JSON; keys are documented in
//! AGENTS.md §13.6.

use serde_json::Value;
use sqlx::PgExecutor;

/// Key of the screening threshold overrides:
/// `{ "<section>": { "<criterion key>": { "reject_below"?: f64, "accept_above"?: f64 } } }`.
pub const SCREENING_THRESHOLDS: &str = "screening.thresholds";

/// Key of the cosine similarity from which an accepted idea is merged into the nearest
/// published one (AGENTS.md §5.4), a number between 0 and 1 overriding
/// `DEDUP_SIMILARITY_THRESHOLD`.
pub const DEDUP_SIMILARITY_THRESHOLD: &str = "dedup.similarity_threshold";

/// Effective duplicate threshold: the setting when it is a number in [0, 1], `default` (the
/// environment) otherwise; an unusable setting is ignored with a warning (§13.6).
pub async fn dedup_similarity_threshold<'e>(
    executor: impl PgExecutor<'e>,
    default: f64,
) -> sqlx::Result<f64> {
    Ok(match get(executor, DEDUP_SIMILARITY_THRESHOLD).await? {
        None => default,
        Some(value) => match value.as_f64() {
            Some(threshold) if (0.0..=1.0).contains(&threshold) => threshold,
            _ => {
                tracing::warn!(
                    key = DEDUP_SIMILARITY_THRESHOLD,
                    "invalid setting ignored, default applied"
                );
                default
            }
        },
    })
}

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

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn dedup_threshold_overrides_the_environment(pool: PgPool) {
        let threshold =
            |pool: PgPool| async move { dedup_similarity_threshold(&pool, 0.95).await.unwrap() };
        assert_eq!(threshold(pool.clone()).await, 0.95);
        for (value, expected) in [
            (serde_json::json!(0.9), 0.9),
            (serde_json::json!(1.5), 0.95),
            (serde_json::json!("0.9"), 0.95),
            (serde_json::json!(1), 1.0),
        ] {
            set(&pool, DEDUP_SIMILARITY_THRESHOLD, &value)
                .await
                .unwrap();
            assert_eq!(threshold(pool.clone()).await, expected, "{value}");
        }
    }
}
