use chrono::Duration;
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::path;
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::domain::types::TargetKind;
use crate::moderation::Target;
use crate::test_support::{TestApp, chat_response, published_idea};

async fn job_for(pool: &PgPool, idea_id: Uuid) -> Job {
    // Due in the past whatever the difference between the test and database clocks.
    let due = Utc::now() - Duration::hours(1);
    queue::enqueue(pool, &pipeline::review_reports_job(idea_id, due))
        .await
        .unwrap();
    queue::claim(pool, "test-worker", &[JobKind::ReviewReports])
        .await
        .unwrap()
        .expect("a due job")
}

/// An accepted report waiting for its review.
async fn report(pool: &PgPool, idea_id: Uuid, reason: &str, details: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, details, status, good_faith)
         VALUES ($1, $2, $3, 'accepted', true) RETURNING id",
    )
    .bind(idea_id)
    .bind(reason)
    .bind(details)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn review(outcome: &str, escalate: bool, corrections: &str) -> Value {
    json!({
        "outcome": outcome,
        "escalate": escalate,
        "explanation": "Les signalements ont été examinés.",
        "corrections": corrections
    })
}

async fn idea_row(pool: &PgPool, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(i) FROM ideas i WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn reviews(pool: &PgPool) -> Vec<Value> {
    sqlx::query_scalar("SELECT to_jsonb(r) FROM report_reviews r ORDER BY created_at")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Bodies of the chat requests received by the Mistral mock.
async fn chats(app: &TestApp) -> Vec<Value> {
    app.mistral
        .as_ref()
        .unwrap()
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/v1/chat/completions")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[test]
fn reports_and_timing() {
    let taxonomy = &crate::ai::prompts::test_prompts().taxonomy;
    let id = Uuid::nil();
    let lines = reports_for_prompt(
        taxonomy,
        &[
            PendingReport {
                id,
                reason: "erreur_chiffrage".into(),
                details: Some("Le montant\nest faux.".into()),
            },
            PendingReport {
                id,
                reason: "autre".into(),
                details: None,
            },
        ],
    );
    let first = Taxonomy::label(&taxonomy.report_reasons, "erreur_chiffrage").unwrap();
    let other = Taxonomy::label(&taxonomy.report_reasons, "autre").unwrap();
    assert_eq!(
        lines,
        format!("{id} — {first} — Le montant est faux.\n{id} — {other} — (aucune précision)")
    );
    let now = Utc::now();
    assert_eq!(due(None, now, false), now);
    assert_eq!(due(Some(now - Duration::hours(1)), now, true), now);
    assert_eq!(
        due(Some(now - Duration::hours(1)), now, false),
        now + Duration::hours(23)
    );
    assert_eq!(due(Some(now - Duration::hours(30)), now, false), now);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn maintain_marks_the_reports_reviewed(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    let first = report(
        &pool,
        idea,
        "erreur_chiffrage",
        Some("Le gain est surestimé."),
    )
    .await;
    let second = report(&pool, idea, "autre", None).await;
    app.structured(&review("maintain", false, "")).await;

    let job = job_for(&pool, idea).await;
    assert_eq!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    let stored = reviews(&pool).await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0]["outcome"], "maintain");
    assert_eq!(stored[0]["escalate"], false);
    assert_eq!(stored[0]["model"], "mistral-medium-test");
    assert_eq!(
        stored[0]["prompt_version"],
        ctx.prompts.report_review.version
    );
    let row = idea_row(&pool, idea).await;
    assert_eq!(row["status"], "published");
    assert_eq!(stored[0]["analysis_id"], row["current_analysis_id"]);
    assert!(row["last_report_review_at"].is_string());
    for id in [first, second] {
        assert_eq!(
            app.history(TargetKind::Report, id).await,
            ["reviewed"],
            "{id}"
        );
        let view = crate::db::tracked::view(&pool, Target::new(TargetKind::Report, id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(view.status, "reviewed");
        assert_eq!(view.report_outcome(), Some(ReviewOutcome::Maintain));
    }
    let user = chats(&app).await[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(user.contains(&format!("{first} — ")));
    assert!(user.contains("Le gain est surestimé."));
    assert!(user.contains(&format!("{second} — ")));
    assert!(user.contains("Mutualiser les achats de fournitures"));
    let (purpose, idea_of_call): (String, Option<Uuid>) =
        sqlx::query_as("SELECT purpose, idea_id FROM ai_calls")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (purpose.as_str(), idea_of_call),
        ("report_review", Some(idea))
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn update_queues_a_reanalysis_with_the_corrections(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    report(
        &pool,
        idea,
        "source_erronee",
        Some("La source S1 est morte."),
    )
    .await;
    app.structured(&review("update", false, " Remplacer la source [S1]. "))
        .await;
    let job = job_for(&pool, idea).await;
    assert_eq!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    let stored = reviews(&pool).await;
    assert_eq!(stored[0]["outcome"], "update");
    assert_eq!(stored[0]["corrections"], "Remplacer la source [S1].");
    let jobs: Vec<Value> =
        sqlx::query_scalar("SELECT payload FROM jobs WHERE kind = 'analyze_idea'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(jobs, [json!({ "idea_id": idea, "trigger": "report" })]);
    assert_eq!(idea_row(&pool, idea).await["status"], "published");
    // The re-analysis finds the corrections.
    let pending = crate::db::analyses::pending_corrections(&pool, idea)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn hide_and_escalate(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let server = app.mistral.as_ref().unwrap();

    // `hide`: masked, exports regenerated, no escalation.
    let idea = published_idea(&pool, "masquee").await;
    report(&pool, idea, "contenu_problematique", Some("Hors sujet.")).await;
    app.structured(&review("hide", false, "")).await;
    let job = job_for(&pool, idea).await;
    assert_eq!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    let row = idea_row(&pool, idea).await;
    assert_eq!(row["status"], "hidden");
    assert_eq!(row["hidden_reason"], "report_review");
    assert!(row["hidden_at"].is_string());
    let (reason,): (Option<String>,) = sqlx::query_as(
        "SELECT reason_code FROM idea_events WHERE idea_id = $1 AND to_status = 'hidden'",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reason.as_deref(), Some("report_review"));
    assert!(
        app.jobs()
            .await
            .iter()
            .any(|(kind, _, _)| kind == "generate_exports")
    );
    let (escalations,): (i64,) = sqlx::query_as("SELECT count(*) FROM escalations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(escalations, 0);
    // The author of the hidden idea sees an automated masking.
    let statement = crate::moderation::statement::load(
        &pool,
        &ctx.prompts,
        Target::new(TargetKind::Idea, idea),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(statement.automated);

    // `escalate` (even with an inconsistent `maintain`): masked at once and queued for a human.
    server.reset().await;
    let serious = published_idea(&pool, "grave").await;
    report(&pool, serious, "contenu_problematique", Some("Menace.")).await;
    app.structured(&review("maintain", true, "")).await;
    let job = job_for(&pool, serious).await;
    assert_eq!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    let row = idea_row(&pool, serious).await;
    assert_eq!(row["status"], "hidden");
    assert_eq!(row["hidden_reason"], "escalation");
    let (review_id, outcome, escalate): (Uuid, String, bool) =
        sqlx::query_as("SELECT id, outcome, escalate FROM report_reviews WHERE idea_id = $1")
            .bind(serious)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((outcome.as_str(), escalate), ("hide", true));
    let queued: (Uuid, Option<Uuid>, String) =
        sqlx::query_as("SELECT idea_id, review_id, status FROM escalations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued, (serious, Some(review_id), "pending".to_owned()));

    // A new escalation of the same idea while one is pending adds nothing.
    sqlx::query("DELETE FROM jobs WHERE kind = 'review_reports'")
        .execute(&pool)
        .await
        .unwrap();
    report(&pool, serious, "contenu_problematique", Some("Encore.")).await;
    let job = job_for(&pool, serious).await;
    assert_eq!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    let (escalations, reviews): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM escalations),
                (SELECT count(*) FROM report_reviews WHERE idea_id = $1)",
    )
    .bind(serious)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((escalations, reviews), (1, 2));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn reviews_wait_24_hours_unless_urgent(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    sqlx::query("UPDATE ideas SET last_report_review_at = now() - interval '1 hour' WHERE id = $1")
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    report(&pool, idea, "erreur_chiffrage", None).await;
    app.structured(&review("maintain", false, "")).await;
    let job = job_for(&pool, idea).await;
    match ReviewReportsHandler.handle(&ctx, &job).await {
        Ok(JobOutcome::RetryAt(at)) => {
            let wait = at - Utc::now();
            // Database and test clocks may differ slightly.
            let expected = Duration::hours(23);
            assert!((wait - expected).abs() < Duration::minutes(1), "{wait}");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(chats(&app).await.is_empty());

    // A problematic content report is reviewed at once, with the others.
    report(
        &pool,
        idea,
        "contenu_problematique",
        Some("Propos haineux."),
    )
    .await;
    assert_eq!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    let (reviewed,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM reports WHERE status = 'reviewed'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reviewed, 2);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn remaining_reports_are_reviewed_by_the_next_run(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    for _ in 0..=MAX_REPORTS_PER_REVIEW {
        report(&pool, idea, "autre", None).await;
    }
    app.structured(&review("maintain", false, "")).await;
    let job = job_for(&pool, idea).await;
    match ReviewReportsHandler.handle(&ctx, &job).await {
        Ok(JobOutcome::RetryAt(at)) => {
            assert!(at - Utc::now() > Duration::hours(23));
        }
        other => panic!("unexpected {other:?}"),
    }
    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM reports WHERE status = 'accepted'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn failures_budget_and_key(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[("MISTRAL_DAILY_BUDGET_EUR", "1")]).await;
    let ctx = app.job_context();
    let server = app.mistral.as_ref().unwrap();
    let idea = published_idea(&pool, "parente").await;
    report(&pool, idea, "autre", Some("Précision.")).await;
    let job = job_for(&pool, idea).await;

    // Invalid answers (not JSON, schema violation, update without corrections): transient.
    let mut no_escalate = review("maintain", false, "");
    no_escalate.as_object_mut().unwrap().remove("escalate");
    for answer in [
        {
            let mut response = chat_response(&json!({}));
            response["choices"][0]["message"]["content"] = json!("pas du JSON");
            response
        },
        chat_response(&no_escalate),
        chat_response(&review("update", false, "")),
    ] {
        server.reset().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .mount(server)
            .await;
        assert!(matches!(
            ReviewReportsHandler.handle(&ctx, &job).await,
            Err(JobError::Transient(_))
        ));
    }
    // Refused request: permanent.
    server.reset().await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400))
        .mount(server)
        .await;
    assert!(matches!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Err(JobError::Permanent(_))
    ));
    assert!(reviews(&pool).await.is_empty());

    // No key, then budget reached: rescheduled.
    let mut without_key = ctx.clone();
    without_key.mistral = None;
    assert!(matches!(
        ReviewReportsHandler.handle(&without_key, &job).await,
        Ok(JobOutcome::RetryAt(_))
    ));
    sqlx::query(
        "INSERT INTO ai_calls (provider, purpose, model, cost_micro_eur, latency_ms, status)
         VALUES ('mistral', 'report_review', 'm', 2000000, 10, 'success')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let calls = server.received_requests().await.unwrap().len();
    assert!(matches!(
        ReviewReportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::RetryAt(_))
    ));
    assert_eq!(server.received_requests().await.unwrap().len(), calls);
    let (status,): (String,) = sqlx::query_as("SELECT status FROM reports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "accepted");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn nothing_to_review(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    for id in [idea, Uuid::nil()] {
        let job = job_for(&pool, id).await;
        assert_eq!(
            ReviewReportsHandler.handle(&ctx, &job).await,
            Ok(JobOutcome::Done)
        );
        sqlx::query("DELETE FROM jobs")
            .execute(&pool)
            .await
            .unwrap();
    }
    assert!(chats(&app).await.is_empty());
}
