//! Worker side of the moderation (AGENTS.md §5.2, §5.3): screening retries and Mistral second
//! opinions. Both delegate to [`crate::moderation`], which checks the status of the text under
//! a row lock: a job about a text settled meanwhile (by the api, another job or its author's
//! deletion) succeeds without doing anything.

use super::pipeline::{self, TargetPayload};
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobError, JobKind, JobOutcome};
use crate::domain::types::{Actor, TargetKind};
use crate::moderation::second_opinion::{self, SecondOpinionError};
use crate::moderation::{self, Step, Target};

/// Payload of a moderation job, checked against the kind of the job.
fn target_of(job: &Job, expected: Option<JobKind>) -> Result<Target, JobError> {
    let payload: TargetPayload = job.payload_as()?;
    if let Some(expected) = expected
        && pipeline::screen_kind(payload.target_kind) != expected
    {
        return Err(JobError::Permanent(format!(
            "{} job about a {}",
            job.kind, payload.target_kind
        )));
    }
    Ok(Target::new(payload.target_kind, payload.target_id))
}

/// Screening retry of a `submitted` text ([`JobKind::ScreenIdea`], [`JobKind::ScreenContribution`],
/// [`JobKind::ScreenReport`]): enqueued with every new text, it only works when the synchronous
/// screening of the api could not complete. While the decision service is unavailable it fails
/// transiently and the queue retries it with backoff.
#[derive(Debug, Clone, Copy)]
pub struct ScreenHandler(TargetKind);

impl ScreenHandler {
    pub const fn new(kind: TargetKind) -> Self {
        Self(kind)
    }
}

impl JobHandler for ScreenHandler {
    fn kind(&self) -> JobKind {
        pipeline::screen_kind(self.0)
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let target = target_of(job, Some(self.kind()))?;
            let step = moderation::screen(
                &ctx.pool,
                &ctx.screener,
                &ctx.prompts,
                target,
                Some(job.id),
                Actor::Worker,
                None,
            )
            .await
            .map_err(|err| JobError::Transient(err.to_string()))?;
            match step {
                Step::Pending => Err(JobError::Transient(
                    "decision service unavailable".to_owned(),
                )),
                _ => Ok(JobOutcome::Done),
            }
        })
    }
}

/// Mistral second opinion of a text in the gray zone ([`JobKind::SecondOpinion`]). Claimed only
/// when `MISTRAL_API_KEY` is set, postponed by the worker while the daily budget is reached.
#[derive(Debug, Default, Clone, Copy)]
pub struct SecondOpinionHandler;

impl JobHandler for SecondOpinionHandler {
    fn kind(&self) -> JobKind {
        JobKind::SecondOpinion
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let target = target_of(job, None)?;
            let Some(mistral) = &ctx.mistral else {
                return Err(JobError::Transient("MISTRAL_API_KEY is not set".to_owned()));
            };
            match second_opinion::run(
                &ctx.pool,
                &ctx.prompts,
                mistral,
                target,
                Some(job.id),
                Actor::Worker,
            )
            .await
            {
                Ok(_) => Ok(JobOutcome::Done),
                Err(SecondOpinionError::Mistral(err)) if !err.is_transient() => {
                    Err(JobError::Permanent(err.to_string()))
                }
                Err(SecondOpinionError::Render(err)) => Err(JobError::Permanent(err.to_string())),
                Err(err) => Err(JobError::Transient(err.to_string())),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};
    use sqlx::PgPool;
    use uuid::Uuid;
    use wiremock::matchers::path;
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::db::submissions::{self, NewIdea};
    use crate::jobs::queue;
    use crate::test_support::TestApp;

    async fn new_idea(pool: &PgPool) -> Uuid {
        let idea = NewIdea {
            text: "Mutualiser les achats de fournitures entre les communes voisines.".to_owned(),
            admin_level: None,
            territory: None,
        };
        submissions::insert_idea(pool, &idea).await.unwrap().0
    }

    /// Makes the queued job of `kind` due and claims it.
    async fn claim(pool: &PgPool, kind: JobKind) -> Job {
        sqlx::query("UPDATE jobs SET run_after = now() WHERE kind = $1 AND status = 'queued'")
            .bind(kind.as_str())
            .execute(pool)
            .await
            .unwrap();
        queue::claim(pool, "test-worker", &[kind])
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

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn screening_is_retried_until_the_service_answers(pool: PgPool) {
        let app = TestApp::new(pool.clone(), false, &[]).await;
        let ctx = app.job_context();
        let idea = new_idea(&pool).await;
        let handler = ScreenHandler::new(TargetKind::Idea);
        assert_eq!(handler.kind(), JobKind::ScreenIdea);

        // Service down: transient failure, the idea stays submitted.
        Mock::given(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(502))
            .up_to_n_times(1)
            .mount(&app.decision)
            .await;
        let job = claim(&pool, JobKind::ScreenIdea).await;
        assert_eq!(job.max_attempts, pipeline::SCREEN_MAX_ATTEMPTS);
        assert!(matches!(
            handler.handle(&ctx, &job).await,
            Err(JobError::Transient(_))
        ));
        assert_eq!(status_of(&pool, idea).await, "submitted");
        queue::fail(&pool, &job, "test-worker", "down", false)
            .await
            .unwrap();

        // Service back: accepted, dedup queued, the job id is recorded with the call.
        app.decision_answers(TargetKind::Idea, &[]).await;
        let job = claim(&pool, JobKind::ScreenIdea).await;
        assert_eq!(handler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(status_of(&pool, idea).await, "accepted");
        assert_eq!(
            app.history(TargetKind::Idea, idea).await,
            ["submitted", "screening", "accepted"]
        );
        let (actor,): (String,) =
            sqlx::query_as("SELECT actor FROM idea_events WHERE to_status = 'accepted'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(actor, "worker");
        let jobs: Vec<Option<i64>> = sqlx::query_scalar("SELECT job_id FROM ai_calls ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, [Some(job.id), Some(job.id)]);
        let (dedup,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM jobs WHERE kind = 'dedup_idea'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(dedup, 1);

        // Running it again (crash after the commit) does nothing.
        assert_eq!(handler.handle(&ctx, &job).await, Ok(JobOutcome::Done));
        assert_eq!(app.decision.received_requests().await.unwrap().len(), 2);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn screening_jobs_check_their_payload(pool: PgPool) {
        let app = TestApp::new(pool.clone(), false, &[]).await;
        let ctx = app.job_context();
        let mut job = claim_new(&pool, pipeline::screen_job(TargetKind::Report, Uuid::nil())).await;
        // A report payload in an idea screening job.
        job.kind = JobKind::ScreenIdea;
        assert!(matches!(
            ScreenHandler::new(TargetKind::Idea)
                .handle(&ctx, &job)
                .await,
            Err(JobError::Permanent(_))
        ));
        // A text that no longer exists: nothing to do.
        job.kind = JobKind::ScreenReport;
        assert_eq!(
            ScreenHandler::new(TargetKind::Report)
                .handle(&ctx, &job)
                .await,
            Ok(JobOutcome::Done)
        );
    }

    async fn claim_new(pool: &PgPool, job: crate::jobs::NewJob) -> Job {
        let kind = job.kind;
        queue::enqueue(pool, &job).await.unwrap();
        claim(pool, kind).await
    }

    /// An idea waiting for its second opinion, as left by the api without Mistral.
    async fn gray_idea(app: &TestApp) -> Uuid {
        app.decision_answers(TargetKind::Idea, &[("realism", 0.4)])
            .await;
        let idea = new_idea(&app.pool).await;
        let step = moderation::screen(
            &app.pool,
            &app.state.screener,
            &app.state.prompts,
            Target::new(TargetKind::Idea, idea),
            None,
            Actor::Api,
            None,
        )
        .await
        .unwrap();
        assert_eq!(step, Step::SecondOpinion);
        idea
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn second_opinion_job_settles_the_text(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[]).await;
        let ctx = app.job_context();
        let idea = gray_idea(&app).await;
        app.second_opinion(
            json!({
                "decision": "reject",
                "reason_code": "unrealistic",
                "explanation": "Mesure irréalisable en l'état."
            }),
            Duration::ZERO,
        )
        .await;

        let job = claim(&pool, JobKind::SecondOpinion).await;
        assert_eq!(
            job.payload,
            json!({ "target_kind": "idea", "target_id": idea.to_string() })
        );
        assert_eq!(
            SecondOpinionHandler.handle(&ctx, &job).await,
            Ok(JobOutcome::Done)
        );
        assert_eq!(status_of(&pool, idea).await, "rejected");
        let screening: Value =
            sqlx::query_scalar("SELECT to_jsonb(s) FROM screenings s WHERE outcome = 'gray_zone'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(screening["second_opinion_decision"], "reject");
        assert_eq!(screening["second_opinion_reason_code"], "unrealistic");
        assert_eq!(screening["final_decision"], "reject");
        let (job_id,): (Option<i64>,) =
            sqlx::query_as("SELECT job_id FROM ai_calls WHERE provider = 'mistral'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(job_id, Some(job.id));

        // Already settled: no new Mistral call.
        assert_eq!(
            SecondOpinionHandler.handle(&ctx, &job).await,
            Ok(JobOutcome::Done)
        );
        assert_eq!(
            app.mistral
                .as_ref()
                .unwrap()
                .received_requests()
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn second_opinion_failures_are_classified(pool: PgPool) {
        let app = TestApp::new(pool.clone(), true, &[]).await;
        let ctx = app.job_context();
        gray_idea(&app).await;
        let server = app.mistral.as_ref().unwrap();
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(server)
            .await;
        let job = claim(&pool, JobKind::SecondOpinion).await;
        assert!(matches!(
            SecondOpinionHandler.handle(&ctx, &job).await,
            Err(JobError::Transient(_))
        ));
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400))
            .mount(server)
            .await;
        assert!(matches!(
            SecondOpinionHandler.handle(&ctx, &job).await,
            Err(JobError::Permanent(_))
        ));

        let mut without_key = ctx.clone();
        without_key.mistral = None;
        assert!(matches!(
            SecondOpinionHandler.handle(&without_key, &job).await,
            Err(JobError::Transient(_))
        ));
    }
}
