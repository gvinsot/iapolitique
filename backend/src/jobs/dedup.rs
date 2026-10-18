//! Duplicate detection of an accepted idea (AGENTS.md §5.4), job `dedup_idea`.
//!
//! `accepted → dedup`, then the (masked) text is embedded with `mistral-embed` (an embedding
//! already computed from the same text and model is reused) and stored, and the nearest
//! published idea is searched by cosine distance (HNSW index):
//!
//! - similarity ≥ threshold (setting `dedup.similarity_threshold`, else
//!   `DEDUP_SIMILARITY_THRESHOLD`): `dedup → merged`, the idea becomes an accepted `doublon`
//!   contribution of the published idea, whose proposal counter goes up and whose re-analysis is
//!   queued (trigger `merge`); the author's tracking link then shows the parent idea;
//! - otherwise: `dedup → queued_analysis` and the first analysis is queued.
//!
//! In doubt, no merge: when the embedding cannot be obtained (non transient Mistral error, or
//! the last attempt of the job failed), the idea goes on to its analysis without deduplication.
//! An idea deleted by its author meanwhile, or no longer waiting for deduplication, is left
//! alone.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::guard::{self, Gate};
use super::pipeline::{self, IdeaPayload};
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobError, JobKind, JobOutcome, queue};
use crate::ai::mistral::{CallContext, MistralClient, MistralError};
use crate::db::status::{self, Change};
use crate::db::{embeddings, ideas, settings};
use crate::domain::status::IdeaStatus;
use crate::domain::types::{Actor, AiPurpose, AnalysisTrigger, TargetKind};

/// What the nearest neighbour search concluded.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    /// Merge into this published idea.
    Duplicate { parent: Uuid, similarity: f64 },
    /// Analyse as a new idea; similarity of the nearest published idea, if any.
    New { nearest: Option<f64> },
    /// The embedding could not be obtained: analyse without deduplication.
    Skipped,
}

/// Handler of [`JobKind::DedupIdea`].
#[derive(Debug, Default, Clone, Copy)]
pub struct DedupHandler;

impl JobHandler for DedupHandler {
    fn kind(&self) -> JobKind {
        JobKind::DedupIdea
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let IdeaPayload { idea_id } = job.payload_as()?;
            let row: Option<(String, Option<String>)> =
                sqlx::query_as("SELECT status, original_text FROM ideas WHERE id = $1")
                    .bind(idea_id)
                    .fetch_optional(&ctx.pool)
                    .await?;
            let Some((status, Some(text))) = row else {
                return Ok(JobOutcome::Done);
            };
            if status != IdeaStatus::Accepted.as_str() && status != IdeaStatus::Dedup.as_str() {
                return Ok(JobOutcome::Done);
            }
            let client = match guard::mistral_gate(ctx, self.kind()).await? {
                Gate::Ready(client) => client,
                Gate::Postpone(outcome) => return Ok(outcome),
            };
            if !start(&ctx.pool, idea_id).await? {
                return Ok(JobOutcome::Done);
            }

            let outcome = match search(ctx, client, idea_id, &text, job.id).await {
                Ok(outcome) => outcome,
                Err(err) if err.is_transient() && job.attempts < job.max_attempts => {
                    return Err(JobError::Transient(err.to_string()));
                }
                Err(err) => {
                    tracing::warn!(
                        idea_id = %idea_id,
                        error = %err,
                        "no embedding: idea analysed without duplicate detection"
                    );
                    Outcome::Skipped
                }
            };
            settle(&ctx.pool, idea_id, outcome).await?;
            Ok(JobOutcome::Done)
        })
    }
}

/// `accepted → dedup` (or already `dedup` after an interrupted run). Returns whether the
/// deduplication should go on.
async fn start(pool: &PgPool, idea_id: Uuid) -> Result<bool, JobError> {
    let mut tx = pool.begin().await?;
    let go_on = match status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?
    {
        Some(IdeaStatus::Accepted) => {
            status::transition(
                &mut tx,
                idea_id,
                IdeaStatus::Dedup,
                &Change::by(Actor::Worker),
            )
            .await
            .map_err(status_error)?;
            true
        }
        Some(IdeaStatus::Dedup) => true,
        _ => false,
    };
    tx.commit().await?;
    Ok(go_on)
}

fn status_error(err: status::StatusError) -> JobError {
    match err {
        status::StatusError::Database(err) => err.into(),
        other => JobError::Permanent(other.to_string()),
    }
}

/// Embedding (stored) and nearest published idea, compared with the threshold.
async fn search(
    ctx: &JobContext,
    client: &MistralClient,
    idea_id: Uuid,
    text: &str,
    job_id: i64,
) -> Result<Outcome, MistralError> {
    let pool = &ctx.pool;
    let model = &ctx.config.mistral.embed_model;
    let hash = embeddings::content_hash(text);
    let vector = match embeddings::current(pool, idea_id, model, &hash).await? {
        Some(vector) => vector,
        None => {
            let call = CallContext {
                target: Some((TargetKind::Idea, idea_id)),
                idea_id: Some(idea_id),
                job_id: Some(job_id),
                ..CallContext::new(AiPurpose::Embedding)
            };
            let completion = client.embed(pool, &call, &[text]).await?;
            let vector = completion
                .value
                .into_iter()
                .next()
                .ok_or_else(|| MistralError::InvalidOutput("no embedding".into()))?;
            if vector.iter().any(|x| !x.is_finite()) {
                return Err(MistralError::InvalidOutput("non finite embedding".into()));
            }
            embeddings::upsert(pool, idea_id, model, &vector, &hash).await?;
            vector
        }
    };
    let threshold =
        settings::dedup_similarity_threshold(pool, ctx.config.dedup_similarity_threshold).await?;
    Ok(
        match embeddings::nearest_published(pool, idea_id, model, &vector).await? {
            Some((parent, similarity)) if similarity >= threshold => {
                Outcome::Duplicate { parent, similarity }
            }
            nearest => Outcome::New {
                nearest: nearest.map(|(_, similarity)| similarity),
            },
        },
    )
}

/// Applies the outcome in one transaction, if the idea is still in `dedup`.
async fn settle(pool: &PgPool, idea_id: Uuid, outcome: Outcome) -> Result<(), JobError> {
    let mut tx = pool.begin().await?;
    if status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?
        != Some(IdeaStatus::Dedup)
    {
        return Ok(());
    }
    let mut outcome = outcome;
    if let Outcome::Duplicate { parent, similarity } = outcome {
        // The parent may have been hidden or deleted since the search.
        let parent_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM ideas WHERE id = $1 FOR UPDATE")
                .bind(parent)
                .fetch_optional(&mut *tx)
                .await?;
        if parent_status.as_deref() != Some(IdeaStatus::Published.as_str()) {
            outcome = Outcome::New {
                nearest: Some(similarity),
            };
        }
    }
    match outcome {
        Outcome::Duplicate { parent, similarity } => {
            let change = Change::by(Actor::Worker).details(json!({
                "parent_idea_id": parent,
                "similarity": similarity,
            }));
            ideas::merge(&mut tx, idea_id, parent, &change)
                .await
                .map_err(status_error)?;
            queue::enqueue(
                &mut *tx,
                &pipeline::analyze_job(parent, AnalysisTrigger::Merge),
            )
            .await?;
            tracing::info!(idea_id = %idea_id, parent_id = %parent, "duplicate idea merged");
        }
        Outcome::New { .. } | Outcome::Skipped => {
            let details = match outcome {
                Outcome::New { nearest } => json!({ "nearest_similarity": nearest }),
                _ => json!({ "dedup": "skipped" }),
            };
            status::transition(
                &mut tx,
                idea_id,
                IdeaStatus::QueuedAnalysis,
                &Change::by(Actor::Worker).details(details),
            )
            .await
            .map_err(status_error)?;
            queue::enqueue(
                &mut *tx,
                &pipeline::analyze_job(idea_id, AnalysisTrigger::Initial),
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use wiremock::matchers::path;
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::test_support::{TestApp, idea_with_status, published_idea, unit_vector};

    async fn job_for(pool: &PgPool, idea_id: Uuid) -> Job {
        queue::enqueue(pool, &pipeline::dedup_job(idea_id))
            .await
            .unwrap();
        queue::claim(pool, "test-worker", &[JobKind::DedupIdea])
            .await
            .unwrap()
            .expect("a due job")
    }

    async fn status_of(pool: &PgPool, id: Uuid) -> String {
        sqlx::query_scalar("SELECT status FROM ideas WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A published idea whose stored embedding is `unit_vector(theta)`.
    async fn published_with_embedding(pool: &PgPool, slug: &str, theta: f64) -> Uuid {
        let id = published_idea(pool, slug).await;
        embeddings::upsert(
            pool,
            id,
            "mistral-embed",
            &unit_vector(theta),
            &embeddings::content_hash(slug),
        )
        .await
        .unwrap();
        id
    }

    async fn analyze_jobs(pool: &PgPool) -> Vec<Value> {
        sqlx::query_scalar(
            "SELECT payload FROM jobs WHERE kind = 'analyze_idea' AND status = 'queued' ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn a_new_idea_goes_to_analysis(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[]).await;
        let ctx = app.job_context();
        published_with_embedding(&pool, "lointaine", 1.0).await;
        let idea = idea_with_status(&pool, "accepted").await;
        app.embedding(unit_vector(0.0)).await;

        let job = job_for(&pool, idea).await;
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(status_of(&pool, idea).await, "queued_analysis");
        assert_eq!(
            app.history(TargetKind::Idea, idea).await,
            ["dedup", "queued_analysis"]
        );
        assert_eq!(
            analyze_jobs(&pool).await,
            [json!({ "idea_id": idea, "trigger": "initial" })]
        );
        let (model, call_job): (String, Option<i64>) = sqlx::query_as(
            "SELECT model, job_id FROM ai_calls WHERE purpose = 'embedding' AND idea_id = $1",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((model.as_str(), call_job), ("mistral-embed", Some(job.id)));
        let details: Value = sqlx::query_scalar(
            "SELECT details FROM idea_events WHERE idea_id = $1 AND to_status = 'queued_analysis'",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!((details["nearest_similarity"].as_f64().unwrap() - 1f64.cos()).abs() < 1e-5);

        // The stored embedding is reused: a second run (after a crash) calls nothing.
        sqlx::query("UPDATE ideas SET status = 'dedup' WHERE id = $1")
            .bind(idea)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM jobs WHERE kind = 'analyze_idea'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        let server = app.mistral.as_ref().unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn a_duplicate_is_merged_into_the_nearest_published_idea(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[]).await;
        let ctx = app.job_context();
        let parent = published_with_embedding(&pool, "proche", 0.05).await;
        published_with_embedding(&pool, "lointaine", 1.0).await;
        let idea = idea_with_status(&pool, "accepted").await;
        app.embedding(unit_vector(0.0)).await;

        let job = job_for(&pool, idea).await;
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(status_of(&pool, idea).await, "merged");
        assert_eq!(
            analyze_jobs(&pool).await,
            [json!({ "idea_id": parent, "trigger": "merge" })]
        );
        let (count,): (i32,) = sqlx::query_as("SELECT proposal_count FROM ideas WHERE id = $1")
            .bind(parent)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let (kind, status): (String, String) = sqlx::query_as(
            "SELECT kind, status FROM contributions WHERE idea_id = $1 AND source_idea_id = $2",
        )
        .bind(parent)
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((kind.as_str(), status.as_str()), ("doublon", "accepted"));

        // The author's tracking link shows the parent idea.
        let view = crate::db::tracked::view(
            &pool,
            crate::moderation::Target::new(TargetKind::Idea, idea),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(view.status, "merged");
        assert_eq!(view.idea_slug.as_deref(), Some("proche"));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn the_threshold_setting_prevails(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[("DEDUP_SIMILARITY_THRESHOLD", "0.5")]).await;
        let ctx = app.job_context();
        published_with_embedding(&pool, "proche", 0.3).await;
        settings::set(&pool, settings::DEDUP_SIMILARITY_THRESHOLD, &json!(0.999))
            .await
            .unwrap();
        let idea = idea_with_status(&pool, "accepted").await;
        app.embedding(unit_vector(0.0)).await;
        let job = job_for(&pool, idea).await;
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(status_of(&pool, idea).await, "queued_analysis");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn mistral_failures(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[]).await;
        let ctx = app.job_context();
        let server = app.mistral.as_ref().unwrap();
        let idea = idea_with_status(&pool, "accepted").await;
        let mut job = job_for(&pool, idea).await;

        // Invalid output (wrong dimensions): transient, retried by the queue.
        Mock::given(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(crate::test_support::embedding_response(&[vec![0.5; 3]])),
            )
            .up_to_n_times(2)
            .mount(server)
            .await;
        assert!(matches!(
            DedupHandler.handle(&ctx, &job).await,
            Err(JobError::Transient(_))
        ));
        assert_eq!(status_of(&pool, idea).await, "dedup");
        assert!(analyze_jobs(&pool).await.is_empty());

        // Rejected request (4xx): analysed without deduplication.
        Mock::given(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(400))
            .up_to_n_times(1)
            .mount(server)
            .await;
        job.attempts = 2;
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(status_of(&pool, idea).await, "queued_analysis");
        let details: Value = sqlx::query_scalar(
            "SELECT details FROM idea_events WHERE idea_id = $1 AND to_status = 'queued_analysis'",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(details, json!({ "dedup": "skipped" }));

        // Transient failure on the last attempt: no merge either.
        let other = idea_with_status(&pool, "accepted").await;
        let mut job = job_for(&pool, other).await;
        Mock::given(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(503))
            .mount(server)
            .await;
        job.attempts = job.max_attempts;
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(status_of(&pool, other).await, "queued_analysis");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn budget_key_and_stale_ideas(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[("MISTRAL_DAILY_BUDGET_EUR", "1")]).await;
        let ctx = app.job_context();
        let idea = idea_with_status(&pool, "accepted").await;
        let job = job_for(&pool, idea).await;

        // No key: rescheduled, nothing changed.
        let mut without_key = ctx.clone();
        without_key.mistral = None;
        assert!(matches!(
            DedupHandler.handle(&without_key, &job).await,
            Ok(JobOutcome::RetryAt(_))
        ));
        // Budget reached: postponed to the next Paris midnight, nothing changed.
        sqlx::query(
            "INSERT INTO ai_calls (provider, purpose, model, cost_micro_eur, latency_ms, status)
             VALUES ('mistral', 'analysis_research', 'm', 2000000, 10, 'success')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            DedupHandler.handle(&ctx, &job).await,
            Ok(JobOutcome::RetryAt(_))
        ));
        assert_eq!(status_of(&pool, idea).await, "accepted");
        assert!(
            app.mistral
                .as_ref()
                .unwrap()
                .received_requests()
                .await
                .unwrap()
                .is_empty()
        );

        // Deleted or already handled ideas: nothing to do.
        let rejected = idea_with_status(&pool, "queued_analysis").await;
        let job = job_for(&pool, rejected).await;
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        let mut job = job;
        job.payload = json!({ "idea_id": Uuid::nil() });
        assert_eq!(DedupHandler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
    }
}
