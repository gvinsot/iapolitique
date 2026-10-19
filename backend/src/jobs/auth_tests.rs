//! A Mistral API key refused (HTTP 401/403) is a configuration error (AGENTS.md §6.4, §13.1):
//! the jobs wait without consuming an attempt, nothing fails nor is skipped, one job per period
//! checks the key, the alert is shown on the admin dashboard and the api leaves the second
//! opinion to its job. The failed analyses of before can be queued again in bulk.

use std::time::Duration as StdDuration;

use axum::http::{Method, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{Mock, ResponseTemplate};

use super::analysis::AnalyzeHandler;
use super::dedup::DedupHandler;
use super::moderation::SecondOpinionHandler;
use super::reports::ReviewReportsHandler;
use super::worker::JobHandler;
use super::{Job, JobKind, JobOutcome, pipeline, queue};
use crate::db::provider_alerts::{self, AUTH_RETRY_MIN, MISTRAL_AUTH};
use crate::domain::types::{AnalysisTrigger, TargetKind};
use crate::test_support::{
    TestApp, admin_actions, admin_call, admin_get, idea_with_status, published_idea, unit_vector,
};

async fn claim(pool: &PgPool, job: &super::NewJob, kind: JobKind) -> Job {
    queue::enqueue(pool, job).await.unwrap();
    sqlx::query("UPDATE jobs SET run_after = now() - interval '1 hour' WHERE kind = $1")
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

async fn mistral_calls(app: &TestApp) -> usize {
    app.mistral
        .as_ref()
        .unwrap()
        .received_requests()
        .await
        .unwrap()
        .len()
}

/// Every Mistral endpoint answers `status`.
async fn refuse(app: &TestApp, status: u16) {
    let server = app.mistral.as_ref().unwrap();
    server.reset().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "message": "Unauthorized", "request_id": "x"
        })))
        .mount(server)
        .await;
}

/// The next job may check the key again.
async fn probe_due(pool: &PgPool) {
    sqlx::query("UPDATE provider_alerts SET probed_at = now() - interval '2 hours'")
        .execute(pool)
        .await
        .unwrap();
}

/// A postponement of at least [`AUTH_RETRY_MIN`], rescheduled by the queue without consuming
/// the attempt.
async fn assert_postponed(pool: &PgPool, job: &Job, outcome: JobOutcome) {
    let JobOutcome::RetryAt(at) = outcome else {
        panic!("not postponed: {outcome:?}");
    };
    assert!(
        at >= Utc::now() + AUTH_RETRY_MIN - Duration::seconds(30),
        "{at}"
    );
    assert!(
        queue::reschedule(pool, job.id, "test-worker", at)
            .await
            .unwrap()
    );
    let (status, attempts): (String, i32) =
        sqlx::query_as("SELECT status, attempts FROM jobs WHERE id = $1")
            .bind(job.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), attempts), ("queued", job.attempts - 1));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_refused_key_postpones_analysis_and_dedup_without_failing(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    refuse(&app, 401).await;

    // First analysis, even on the last attempt of its job: postponed, never analysis_failed.
    let idea = idea_with_status(&pool, "queued_analysis").await;
    let mut job = claim(
        &pool,
        &pipeline::analyze_job(idea, AnalysisTrigger::Initial),
        JobKind::AnalyzeIdea,
    )
    .await;
    job.attempts = job.max_attempts;
    let outcome = AnalyzeHandler.handle(&ctx, &job).await.unwrap();
    job.attempts = 1;
    assert_postponed(&pool, &job, outcome).await;
    assert_eq!(status_of(&pool, idea).await, "analyzing");
    assert_eq!(
        mistral_calls(&app).await,
        1,
        "one attempt, no retry of a 401"
    );
    let alert = provider_alerts::get(&pool, MISTRAL_AUTH)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((alert.last_http_status, alert.failures), (Some(401), 1));

    // Other jobs meanwhile wait without calling Mistral.
    let accepted = idea_with_status(&pool, "accepted").await;
    let dedup = claim(&pool, &pipeline::dedup_job(accepted), JobKind::DedupIdea).await;
    let outcome = DedupHandler.handle(&ctx, &dedup).await.unwrap();
    assert_postponed(&pool, &dedup, outcome).await;
    assert_eq!(mistral_calls(&app).await, 1);
    assert_eq!(status_of(&pool, accepted).await, "accepted");

    // A period later, the dedup checks the key (403 now): it waits instead of analysing
    // without duplicate detection, even on its last attempt.
    refuse(&app, 403).await;
    probe_due(&pool).await;
    let mut dedup = queue::claim(&pool, "test-worker", &[JobKind::DedupIdea])
        .await
        .unwrap();
    assert!(dedup.is_none(), "rescheduled in the future");
    sqlx::query("UPDATE jobs SET run_after = now() WHERE kind = 'dedup_idea'")
        .execute(&pool)
        .await
        .unwrap();
    dedup = queue::claim(&pool, "test-worker", &[JobKind::DedupIdea])
        .await
        .unwrap();
    let mut dedup = dedup.unwrap();
    dedup.attempts = dedup.max_attempts;
    let outcome = DedupHandler.handle(&ctx, &dedup).await.unwrap();
    dedup.attempts = 1;
    assert_postponed(&pool, &dedup, outcome).await;
    assert_eq!(status_of(&pool, accepted).await, "dedup");
    let analyses: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'analyze_idea' AND payload->>'idea_id' = $1",
    )
    .bind(accepted.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(analyses, 0, "no analysis without duplicate detection");
    let alert = provider_alerts::get(&pool, MISTRAL_AUTH)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((alert.last_http_status, alert.failures), (Some(403), 2));

    // The key works again: the next check succeeds, the alert ends, the pipeline goes on.
    app.mistral.as_ref().unwrap().reset().await;
    app.embedding(unit_vector(0.0)).await;
    probe_due(&pool).await;
    sqlx::query("UPDATE jobs SET run_after = now() WHERE kind = 'dedup_idea'")
        .execute(&pool)
        .await
        .unwrap();
    let dedup = queue::claim(&pool, "test-worker", &[JobKind::DedupIdea])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        DedupHandler.handle(&ctx, &dedup).await,
        Ok(JobOutcome::Done)
    );
    assert_eq!(status_of(&pool, accepted).await, "queued_analysis");
    assert!(
        provider_alerts::get(&pool, MISTRAL_AUTH)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_refused_key_postpones_second_opinions_and_report_reviews(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    refuse(&app, 403).await;

    let gray = idea_with_status(&pool, "second_opinion").await;
    let job = claim(
        &pool,
        &pipeline::second_opinion_job(TargetKind::Idea, gray),
        JobKind::SecondOpinion,
    )
    .await;
    let outcome = SecondOpinionHandler.handle(&ctx, &job).await.unwrap();
    assert_postponed(&pool, &job, outcome).await;
    assert_eq!(status_of(&pool, gray).await, "second_opinion");

    probe_due(&pool).await;
    let idea = published_idea(&pool, "idee-signalee").await;
    let report: Uuid = sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, status, good_faith)
         VALUES ($1, 'erreur_chiffrage', 'accepted', true) RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    let job = claim(
        &pool,
        &pipeline::review_reports_job(idea, Utc::now() - Duration::hours(1)),
        JobKind::ReviewReports,
    )
    .await;
    let outcome = ReviewReportsHandler.handle(&ctx, &job).await.unwrap();
    assert_postponed(&pool, &job, outcome).await;
    let status: String = sqlx::query_scalar("SELECT status FROM reports WHERE id = $1")
        .bind(report)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "accepted");
    assert_eq!(mistral_calls(&app).await, 2);
    let dead: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status = 'dead'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dead, 0);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_api_leaves_the_second_opinion_to_its_job_while_the_key_is_refused(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3)])
        .await;
    refuse(&app, 401).await;
    let body = |app: &TestApp| {
        json!({
            "text": "Mutualiser les achats de fournitures entre les communes voisines pour \
                     obtenir de meilleurs prix.",
            "accept_licence": true,
            "accept_publication": true,
            "altcha": app.altcha(),
        })
    };
    let (status, answer) = app.post("/api/ideas", &body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    assert_eq!(answer["status"], "second_opinion");
    // The inline call was refused: the alert is recorded, the job takes over.
    tokio::time::sleep(StdDuration::from_millis(50)).await;
    assert!(
        provider_alerts::get(&pool, MISTRAL_AUTH)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(mistral_calls(&app).await, 1);

    // While the alert lasts, no inline call at all.
    let (status, answer) = app.post("/api/ideas", &body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    assert_eq!(answer["status"], "second_opinion");
    assert_eq!(mistral_calls(&app).await, 1);
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'second_opinion' AND status = 'queued'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(jobs, 2);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_dashboard_shows_the_alert_and_failed_analyses_are_queued_again(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let (status, dashboard) = admin_get(&app, "/api/admin/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(dashboard["alerts"], json!([]));

    provider_alerts::record_failure(&pool, MISTRAL_AUTH, 401)
        .await
        .unwrap();
    let (_, dashboard) = admin_get(&app, "/api/admin/dashboard").await;
    let alert = &dashboard["alerts"][0];
    assert_eq!(alert["code"], "mistral_auth");
    assert_eq!(alert["http_status"], 401);
    assert_eq!(alert["failures"], 1);
    let message = alert["message"].as_str().unwrap();
    assert!(
        message.starts_with("Clé Mistral refusée depuis le ") && message.contains("(HTTP 401)"),
        "{message}"
    );
    assert!(!message.contains("test-key"));

    // Ideas that failed before the fix go back to the queue, in one logged action.
    let failed = [
        idea_with_status(&pool, "analysis_failed").await,
        idea_with_status(&pool, "analysis_failed").await,
    ];
    let published = published_idea(&pool, "deja-publiee").await;
    let (status, body) = admin_call(
        &app,
        Method::POST,
        "/api/admin/ideas/reanalyze-failed",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body, json!({ "requeued": 2, "remaining": 0 }));
    for id in failed {
        assert_eq!(status_of(&pool, id).await, "queued_analysis");
        let trigger: String = sqlx::query_scalar(
            "SELECT payload->>'trigger' FROM jobs WHERE kind = 'analyze_idea'
               AND payload->>'idea_id' = $1",
        )
        .bind(id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(trigger, "admin");
    }
    assert_eq!(status_of(&pool, published).await, "published");
    assert_eq!(
        admin_actions(&pool).await,
        [("idea.reanalyze_failed".to_owned(), None)]
    );
    let details: Value = sqlx::query_scalar("SELECT details FROM admin_actions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(details["count"], 2);
    assert_eq!(details["idea_ids"].as_array().unwrap().len(), 2);

    // Nothing left: an empty action, still logged.
    let (status, body) = admin_call(
        &app,
        Method::POST,
        "/api/admin/ideas/reanalyze-failed",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body, json!({ "requeued": 0, "remaining": 0 }));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_dashboard_shows_a_model_that_refuses_reasoning_effort(pool: PgPool) {
    // A stale refusal (model no longer configured) is not an alert.
    provider_alerts::record_reasoning_refusal(&pool, "mistral-small-2402", 400)
        .await
        .unwrap();
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let (_, dashboard) = admin_get(&app, "/api/admin/dashboard").await;
    assert_eq!(dashboard["alerts"], json!([]));

    provider_alerts::record_reasoning_refusal(&pool, "mistral-large-latest", 400)
        .await
        .unwrap();
    let (status, dashboard) = admin_get(&app, "/api/admin/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    let alerts = dashboard["alerts"].as_array().unwrap();
    assert_eq!(alerts.len(), 1, "{alerts:?}");
    assert_eq!(alerts[0]["code"], "mistral_reasoning_unsupported");
    assert_eq!(alerts[0]["http_status"], 400);
    assert_eq!(alerts[0]["failures"], 1);
    let message = alerts[0]["message"].as_str().unwrap();
    assert!(
        message.starts_with(
            "Le modèle mistral-large-latest refuse le paramètre reasoning_effort depuis le "
        ) && message.contains("au lieu de « high »")
            && message.contains("définir MISTRAL_ANALYSIS_MODEL")
            && message.contains("mistral-large-4-0"),
        "{message}"
    );

    // Only the review model sends an effort: the advice names its variable.
    let app = TestApp::new(
        pool.clone(),
        true,
        &[
            ("MISTRAL_ANALYSIS_MODEL", "mistral-large-4-0"),
            ("MISTRAL_REVIEW_MODEL", "mistral-large-latest"),
        ],
    )
    .await;
    let (_, dashboard) = admin_get(&app, "/api/admin/dashboard").await;
    let message = dashboard["alerts"][0]["message"].as_str().unwrap();
    assert!(
        message.contains("« none »") && message.contains("définir MISTRAL_REVIEW_MODEL"),
        "{message}"
    );

    // No effort configured for the model any more: nothing to advise.
    let app = TestApp::new(
        pool,
        true,
        &[
            ("MISTRAL_ANALYSIS_REASONING_EFFORT", ""),
            ("MISTRAL_REVIEW_REASONING_EFFORT", ""),
        ],
    )
    .await;
    let (_, dashboard) = admin_get(&app, "/api/admin/dashboard").await;
    assert_eq!(dashboard["alerts"], json!([]));
}

/// Run-after of the queued job of `kind`, relative to now (seconds).
async fn due_in(pool: &PgPool, kind: JobKind) -> i64 {
    sqlx::query_scalar(
        "SELECT extract(epoch FROM run_after - now())::bigint FROM jobs
         WHERE kind = $1 AND status = 'queued'",
    )
    .bind(kind.as_str())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_admin_checks_the_key_now(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let check = "/api/admin/provider-alerts/mistral/check";
    // Still refused: the alert opens (or grows), nothing is released.
    refuse(&app, 401).await;
    let (status, body) = admin_call(&app, Method::POST, check, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"], "refused");
    assert_eq!(body["http_status"], 401);
    assert_eq!(body["alert_cleared"], false);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .starts_with("Clé Mistral toujours refusée (HTTP 401)")
    );
    let alert = provider_alerts::get(&pool, MISTRAL_AUTH)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alert.last_http_status, Some(401));

    // Jobs waiting for the key (15 min), and one waiting for something else (24 h).
    let idea = idea_with_status(&pool, "queued_analysis").await;
    queue::enqueue(
        &pool,
        &pipeline::analyze_job(idea, AnalysisTrigger::Initial)
            .run_after(Utc::now() + AUTH_RETRY_MIN),
    )
    .await
    .unwrap();
    let published = published_idea(&pool, "signalee").await;
    queue::enqueue(
        &pool,
        &pipeline::review_reports_job(published, Utc::now() + Duration::hours(24)),
    )
    .await
    .unwrap();

    // Mistral cannot conclude: nothing changes.
    let server = app.mistral.as_ref().unwrap();
    server.reset().await;
    Mock::given(wiremock::matchers::path("/v1/models"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(server)
        .await;
    let (_, body) = admin_call(&app, Method::POST, check, None).await;
    assert_eq!(body["result"], "unavailable");
    assert_eq!(body["http_status"], 503);
    assert!(
        provider_alerts::get(&pool, MISTRAL_AUTH)
            .await
            .unwrap()
            .is_some()
    );

    // Accepted: the alert closes and the jobs that waited for the key are due now.
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(server)
        .await;
    let (status, body) = admin_call(&app, Method::POST, check, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "result": "ok", "http_status": 200, "alert_cleared": true, "jobs_due": 1,
            "message": "Clé Mistral acceptée : alerte close, 1 job IA en attente repart maintenant."
        })
    );
    assert!(
        provider_alerts::get(&pool, MISTRAL_AUTH)
            .await
            .unwrap()
            .is_none()
    );
    assert!(due_in(&pool, JobKind::AnalyzeIdea).await <= 0);
    assert!(due_in(&pool, JobKind::ReviewReports).await > 23 * 3600);
    // A check is not a billable AI call.
    let (calls,): (i64,) = sqlx::query_as("SELECT count(*) FROM ai_calls")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 0);
    // Every check is journaled.
    let actions: Vec<(String, Value)> =
        sqlx::query_as("SELECT action, details FROM admin_actions ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let results: Vec<(&str, &Value)> = actions
        .iter()
        .map(|(action, details)| (action.as_str(), &details["result"]))
        .collect();
    assert_eq!(
        results,
        [
            ("provider_alert.mistral_check", &json!("refused")),
            ("provider_alert.mistral_check", &json!("unavailable")),
            ("provider_alert.mistral_check", &json!("ok")),
        ]
    );
    assert_eq!(actions[2].1["jobs_due"], 1);

    // Without a key for the api: nothing to check.
    let without = TestApp::new(pool, false, &[]).await;
    let (status, body) = admin_call(&without, Method::POST, check, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "mistral_key_missing");
}
