//! `ai_calls`: one row per call to the decision service or to Mistral (AGENTS.md §6.4), the
//! basis of the daily budget and of the cost dashboard. No text, no prompt, only metadata.

use std::time::Duration;

use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::domain::money::MicroEur;
use crate::domain::types::{AiCallStatus, AiProvider, AiPurpose, TargetKind};

/// A call to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAiCall {
    pub provider: AiProvider,
    pub purpose: AiPurpose,
    pub model: String,
    /// Screened or analysed entity.
    pub target: Option<(TargetKind, Uuid)>,
    /// Idea concerned (directly or through a contribution or a report).
    pub idea_id: Option<Uuid>,
    pub job_id: Option<i64>,
    pub prompt_version: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub web_search_count: u64,
    pub cost: MicroEur,
    pub latency: Duration,
    pub status: AiCallStatus,
    pub http_status: Option<u16>,
    /// Short machine-readable code (`timeout`, `http_503`…), never provider text.
    pub error_code: Option<String>,
}

impl NewAiCall {
    /// A call with empty usage, to complete with the struct update syntax.
    pub fn new(provider: AiProvider, purpose: AiPurpose, model: impl Into<String>) -> Self {
        Self {
            provider,
            purpose,
            model: model.into(),
            target: None,
            idea_id: None,
            job_id: None,
            prompt_version: None,
            input_tokens: 0,
            output_tokens: 0,
            web_search_count: 0,
            cost: MicroEur::ZERO,
            latency: Duration::ZERO,
            status: AiCallStatus::Success,
            http_status: None,
            error_code: None,
        }
    }
}

fn clamp_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Inserts the call and returns its id. `idea_id` is kept only if the idea still exists: an
/// author may delete an idea while a paid call about it runs, and the call must still be
/// recorded (its cost counts in the daily budget, §6.4) — without the idea.
pub async fn insert<'e>(executor: impl PgExecutor<'e>, call: &NewAiCall) -> sqlx::Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO ai_calls (provider, purpose, model, target_kind, target_id, idea_id, job_id,
             prompt_version, input_tokens, output_tokens, web_search_count, cost_micro_eur,
             latency_ms, status, http_status, error_code)
         VALUES ($1, $2, $3, $4, $5, (SELECT id FROM ideas WHERE id = $6::uuid), $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         RETURNING id",
    )
    .bind(call.provider.as_str())
    .bind(call.purpose.as_str())
    .bind(&call.model)
    .bind(call.target.map(|(kind, _)| kind.as_str()))
    .bind(call.target.map(|(_, id)| id))
    .bind(call.idea_id)
    .bind(call.job_id)
    .bind(call.prompt_version.as_deref())
    .bind(clamp_i32(call.input_tokens))
    .bind(clamp_i32(call.output_tokens))
    .bind(clamp_i32(call.web_search_count))
    .bind(call.cost.0.max(0))
    .bind(clamp_i32(
        u64::try_from(call.latency.as_millis()).unwrap_or(u64::MAX),
    ))
    .bind(call.status.as_str())
    .bind(
        call.http_status
            .map(|s| i16::try_from(s).unwrap_or(i16::MAX)),
    )
    .bind(call.error_code.as_deref())
    .fetch_one(executor)
    .await?;
    Ok(id)
}

/// [`insert`] outside a transaction, retried once without the idea when a concurrent deletion
/// of the idea makes the insert fail on its foreign key (`23503`), so that the cost of a paid
/// call is never lost (BUD-1).
pub async fn record(pool: &PgPool, call: &NewAiCall) -> sqlx::Result<i64> {
    match insert(pool, call).await {
        Err(sqlx::Error::Database(err))
            if call.idea_id.is_some() && err.code().as_deref() == Some("23503") =>
        {
            insert(
                pool,
                &NewAiCall {
                    idea_id: None,
                    ..call.clone()
                },
            )
            .await
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn inserts_a_call(pool: PgPool) {
        let target = Uuid::new_v4();
        let id = insert(
            &pool,
            &NewAiCall {
                target: Some((TargetKind::Idea, target)),
                prompt_version: Some("1.0.0".into()),
                input_tokens: 1_200,
                output_tokens: u64::MAX,
                cost: MicroEur(3_000),
                latency: Duration::from_millis(1_234),
                status: AiCallStatus::Timeout,
                http_status: Some(504),
                error_code: Some("timeout".into()),
                ..NewAiCall::new(AiProvider::Mistral, AiPurpose::Embedding, "mistral-embed")
            },
        )
        .await
        .unwrap();
        let (row,): (serde_json::Value,) =
            sqlx::query_as("SELECT to_jsonb(c) FROM ai_calls c WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        for (column, expected) in [
            ("provider", serde_json::json!("mistral")),
            ("purpose", serde_json::json!("embedding")),
            ("model", serde_json::json!("mistral-embed")),
            ("target_kind", serde_json::json!("idea")),
            ("target_id", serde_json::json!(target)),
            ("prompt_version", serde_json::json!("1.0.0")),
            ("input_tokens", serde_json::json!(1_200)),
            ("output_tokens", serde_json::json!(i32::MAX)),
            ("cost_micro_eur", serde_json::json!(3_000)),
            ("latency_ms", serde_json::json!(1_234)),
            ("status", serde_json::json!("timeout")),
            ("http_status", serde_json::json!(504)),
            ("error_code", serde_json::json!("timeout")),
        ] {
            assert_eq!(row[column], expected, "{column}");
        }
    }

    /// BUD-1: a call about an idea deleted in the meantime is still recorded (its cost counts
    /// in the daily budget), without the idea.
    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn a_call_about_a_deleted_idea_is_recorded_without_it(pool: PgPool) {
        let call = NewAiCall {
            idea_id: Some(Uuid::new_v4()),
            cost: MicroEur(5_000),
            ..NewAiCall::new(
                AiProvider::Mistral,
                AiPurpose::AnalysisResearch,
                "mistral-large",
            )
        };
        let id = record(&pool, &call).await.unwrap();
        let (idea_id, cost): (Option<Uuid>, i64) =
            sqlx::query_as("SELECT idea_id, cost_micro_eur FROM ai_calls WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((idea_id, cost), (None, 5_000));

        // An existing idea is kept.
        let idea = crate::test_support::idea_with_status(&pool, "analyzing").await;
        let id = record(
            &pool,
            &NewAiCall {
                idea_id: Some(idea),
                ..call
            },
        )
        .await
        .unwrap();
        let (idea_id,): (Option<Uuid>,) =
            sqlx::query_as("SELECT idea_id FROM ai_calls WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(idea_id, Some(idea));
    }
}
