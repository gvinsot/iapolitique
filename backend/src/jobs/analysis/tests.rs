use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::path;
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::ai::prompts::test_prompts;
use crate::test_support::{
    TestApp, analysis_output, chat_response, idea_with_status, published_idea, research_response,
};

async fn job_for(pool: &PgPool, idea_id: Uuid, trigger: AnalysisTrigger) -> Job {
    queue::enqueue(pool, &pipeline::analyze_job(idea_id, trigger))
        .await
        .unwrap();
    queue::claim(pool, "test-worker", &[JobKind::AnalyzeIdea])
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

/// Bodies of the requests received by the Mistral mock on `endpoint`.
async fn requests(app: &TestApp, endpoint: &str) -> Vec<Value> {
    app.mistral
        .as_ref()
        .unwrap()
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == endpoint)
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

/// An idea waiting for its first analysis, with the level and territory given by its author.
async fn queued_idea(pool: &PgPool) -> Uuid {
    let id = idea_with_status(pool, "queued_analysis").await;
    sqlx::query(
        "UPDATE ideas SET submitted_admin_level = 'securite_sociale',
             submitted_territory = 'Auvergne' WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    id
}

const SLUG: &str = "mutualiser-les-achats-courants-des-hopitaux-publics";

#[test]
fn contributions_and_schema_version() {
    let prompts = test_prompts();
    let id = Uuid::nil();
    let lines = contributions_for_prompt(
        &prompts.taxonomy,
        &[
            PendingContribution {
                id,
                kind: "amelioration".into(),
                text: "Étendre\naux  régions.".into(),
            },
            PendingContribution {
                id,
                kind: "doublon".into(),
                text: "Même idée.".into(),
            },
        ],
    );
    assert_eq!(
        lines,
        format!(
            "{id} — Amélioration — Étendre aux régions.\n{id} — {DUPLICATE_LABEL} — Même idée."
        )
    );
    let version = schema_version(prompts.analysis.output_schema.as_ref().unwrap());
    assert!(
        version.starts_with("sha256:") && version.len() == 19,
        "{version}"
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_first_analysis_publishes_the_idea(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    // The natural slug is taken: the new idea gets a suffix.
    published_idea(&pool, SLUG).await;
    let idea = queued_idea(&pool).await;
    app.research("Mémo : achats hospitaliers de 25 Md€ [S1].")
        .await;
    app.structured(&analysis_output()).await;

    let job = job_for(&pool, idea, AnalysisTrigger::Initial).await;
    assert_eq!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    assert_eq!(status_of(&pool, idea).await, "published");
    assert_eq!(
        app.history(TargetKind::Idea, idea).await,
        ["analyzing", "published"]
    );

    let row: Value = sqlx::query_scalar("SELECT to_jsonb(i) FROM ideas i WHERE id = $1")
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row["slug"], format!("{SLUG}-2"));
    assert_eq!(
        row["title"],
        "Mutualiser les achats courants des hôpitaux publics"
    );
    assert_eq!(row["category"], "maladie");
    assert_eq!(row["admin_level"], "securite_sociale");
    assert_eq!(row["territory_name"], Value::Null);
    let expected = scoring::compute(&ScoreInput {
        annual_gain_median_eur: 300_000_000,
        implementation_cost_median_eur: 10_000_000,
        feasibility: 4,
        confidence: crate::domain::types::Confidence::Medium,
        time_to_effect_months: 18,
    })
    .unwrap();
    assert_eq!(row["score_eur"], expected.score);
    assert_eq!(row["net_annual_eur"], expected.net_annual_eur);
    assert_eq!(row["scoring_version"], scoring::SCORING_VERSION);
    assert!(row["published_at"].is_string() && row["first_published_at"].is_string());
    assert!(row["last_analyzed_at"].is_string());

    let analysis: Value =
        sqlx::query_scalar("SELECT to_jsonb(a) - 'content' FROM analyses a WHERE idea_id = $1")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    let prompts = test_prompts();
    assert_eq!(analysis["id"], row["current_analysis_id"]);
    assert_eq!(analysis["version"], 1);
    assert_eq!(analysis["trigger"], "initial");
    assert_eq!(analysis["model"], "mistral-medium-test");
    assert_eq!(analysis["prompt_version"], prompts.analysis.version);
    assert_eq!(
        analysis["research_prompt_version"],
        prompts.analysis_research.version
    );
    assert_eq!(
        analysis["schema_version"],
        schema_version(prompts.analysis.output_schema.as_ref().unwrap())
    );
    assert_eq!(analysis["scoring_version"], scoring::SCORING_VERSION);
    assert_eq!(analysis["web_search_count"], 2);
    assert_eq!(analysis["sources"][0]["id"], "S1");
    assert_eq!(analysis["annual_gain_high_eur"], 500_000_000);

    let calls: Vec<(String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT purpose, job_id, prompt_version FROM ai_calls WHERE idea_id = $1 ORDER BY id",
    )
    .bind(idea)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        calls,
        [
            (
                "analysis_research".to_owned(),
                Some(job.id),
                Some(prompts.analysis_research.version.clone())
            ),
            (
                "analysis_structuring".to_owned(),
                Some(job.id),
                Some(prompts.analysis.version.clone())
            ),
        ]
    );
    // What the prompts received.
    let research = &requests(&app, "/v1/conversations").await[0];
    let inputs = research["inputs"].as_str().unwrap();
    assert!(inputs.contains("Mutualiser les achats de fournitures"));
    assert!(inputs.contains("Niveau administratif indiqué par l'auteur : Sécurité sociale"));
    assert!(inputs.contains("Territoire indiqué par l'auteur : Auvergne"));
    assert!(inputs.contains("<contributions>\n(aucune)\n</contributions>"));
    let chat = &requests(&app, "/v1/chat/completions").await[0];
    let user = chat["messages"][1]["content"].as_str().unwrap();
    assert!(user.contains("Mémo : achats hospitaliers de 25 Md€ [S1]."));
    assert!(user.contains("maladie"));
    assert_eq!(chat["response_format"]["json_schema"]["strict"], true);
    // The exports will be regenerated.
    assert!(
        app.jobs()
            .await
            .iter()
            .any(|(kind, key, _)| kind == "generate_exports"
                && key.as_deref() == Some(pipeline::EXPORTS_KEY))
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_reanalysis_integrates_contributions_and_corrections(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let parent = published_idea(&pool, "parente").await;
    let (previous,): (Uuid,) =
        sqlx::query_as("SELECT current_analysis_id FROM ideas WHERE id = $1")
            .bind(parent)
            .fetch_one(&pool)
            .await
            .unwrap();
    let improvement: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, status, text)
         VALUES ($1, 'amelioration', 'accepted', 'Étendre la mesure aux achats informatiques.')
         RETURNING id",
    )
    .bind(parent)
    .fetch_one(&pool)
    .await
    .unwrap();
    let duplicate_idea = idea_with_status(&pool, "dedup").await;
    let mut tx = pool.begin().await.unwrap();
    let duplicate =
        crate::db::ideas::merge(&mut tx, duplicate_idea, parent, &Change::by(Actor::Worker))
            .await
            .unwrap();
    tx.commit().await.unwrap();
    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO report_reviews (idea_id, outcome, explanation, model, prompt_version,
                                     corrections)
         VALUES ($1, 'update', 'Montant erroné.', 'm', '1.1.0', 'Corriger le montant des achats.')
         RETURNING id",
    )
    .bind(parent)
    .fetch_one(&pool)
    .await
    .unwrap();

    let mut output = analysis_output();
    output["contributions_considered"] = json!([{
        "contribution_id": improvement.to_string(),
        "summary": "Étendre la mutualisation aux achats informatiques.",
        "taken_into_account": true,
        "response": "Intégrée au périmètre [S1]."
    }]);
    output["changes_since_previous"] = json!("Périmètre étendu.");
    app.research("Mémo de mise à jour [S1].").await;
    app.structured(&output).await;

    let job = job_for(&pool, parent, AnalysisTrigger::Contribution).await;
    assert_eq!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    assert_eq!(status_of(&pool, parent).await, "published");
    let (current, slug, count, proposals): (Uuid, String, i32, i32) = sqlx::query_as(
        "SELECT current_analysis_id, slug, contribution_count, proposal_count
         FROM ideas WHERE id = $1",
    )
    .bind(parent)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(current, previous);
    assert_eq!((slug.as_str(), count, proposals), ("parente", 1, 2));
    let (version, trigger): (i32, String) =
        sqlx::query_as("SELECT version, trigger FROM analyses WHERE id = $1")
            .bind(current)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((version, trigger.as_str()), (2, "contribution"));
    // The history keeps the previous analysis.
    let (versions,): (i64,) = sqlx::query_as("SELECT count(*) FROM analyses WHERE idea_id = $1")
        .bind(parent)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(versions, 2);

    let contributions: Vec<(Uuid, String, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, status, neutral_summary, integrated_in_analysis_id FROM contributions
         WHERE idea_id = $1 ORDER BY created_at",
    )
    .bind(parent)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        contributions,
        [
            (
                improvement,
                "integrated".to_owned(),
                Some("Étendre la mutualisation aux achats informatiques.".to_owned()),
                Some(current)
            ),
            (duplicate, "integrated".to_owned(), None, Some(current)),
        ]
    );
    assert_eq!(
        app.history(TargetKind::Contribution, improvement).await,
        ["integrated"]
    );
    let applied: Option<Uuid> =
        sqlx::query_scalar("SELECT applied_in_analysis_id FROM report_reviews WHERE id = $1")
            .bind(review)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(applied, Some(current));

    let research = &requests(&app, "/v1/conversations").await[0];
    let inputs = research["inputs"].as_str().unwrap();
    assert!(inputs.contains(&format!(
        "{improvement} — Amélioration — Étendre la mesure aux achats informatiques."
    )));
    assert!(inputs.contains(&format!("{duplicate} — {DUPLICATE_LABEL} — Mutualiser")));
    assert!(inputs.contains("Corriger le montant des achats."));
    assert!(inputs.contains("<analyse_precedente>\n{}\n</analyse_precedente>"));

    // Nothing left to integrate: a new job does nothing.
    sqlx::query("UPDATE jobs SET status = 'succeeded', locked_at = NULL, locked_by = NULL, finished_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    let job = job_for(&pool, parent, AnalysisTrigger::Contribution).await;
    assert_eq!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
    assert_eq!(requests(&app, "/v1/conversations").await.len(), 1);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn contributions_arriving_during_the_analysis_run_it_again(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    // More contributions than one analysis takes.
    for _ in 0..=analyses::MAX_CONTRIBUTIONS_PER_ANALYSIS {
        sqlx::query(
            "INSERT INTO contributions (idea_id, kind, status, text)
             VALUES ($1, 'objection', 'accepted', 'Risque de dépendance aux fournisseurs.')",
        )
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    }
    app.research("Mémo [S1].").await;
    app.structured(&analysis_output()).await;
    let job = job_for(&pool, idea, AnalysisTrigger::Contribution).await;
    assert!(matches!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::RetryAt(_))
    ));
    let (left,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM contributions WHERE idea_id = $1 AND status = 'accepted'",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 1);
    assert_eq!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
}

/// Chat completion whose message content is `content`, verbatim.
fn raw_chat(content: &str) -> Value {
    let mut response = chat_response(&json!({}));
    response["choices"][0]["message"]["content"] = json!(content);
    response
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn invalid_outputs_fail_the_first_analysis(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let server = app.mistral.as_ref().unwrap();
    let mut schema_violation = analysis_output();
    schema_violation.as_object_mut().unwrap().remove("title");
    let mut wrong_category = analysis_output();
    wrong_category["category"] = json!("lune");
    let mut unsourced = analysis_output();
    unsourced["pros"] = json!(["Selon [S9]."]);
    let cases = [
        ("not json", raw_chat("pas du JSON")),
        ("schema", chat_response(&schema_violation)),
        ("category", chat_response(&wrong_category)),
        ("sources", chat_response(&unsourced)),
    ];
    for (case, answer) in cases {
        server.reset().await;
        app.research("Mémo [S1].").await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .mount(server)
            .await;
        let idea = queued_idea(&pool).await;
        let job = job_for(&pool, idea, AnalysisTrigger::Initial).await;
        assert!(
            matches!(
                AnalyzeHandler.handle(&ctx, &job).await,
                Err(JobError::Permanent(_))
            ),
            "{case}"
        );
        assert_eq!(status_of(&pool, idea).await, "analysis_failed", "{case}");
        let (invalid,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ai_calls WHERE idea_id = $1 AND status = 'invalid_output'",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        // Every attempt of the client (2 in the tests) is recorded.
        assert_eq!(invalid, 2, "{case}");
        let (reason,): (Option<String>,) = sqlx::query_as(
            "SELECT reason_code FROM idea_events WHERE idea_id = $1 AND to_status = 'analysis_failed'",
        )
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(reason.as_deref(), Some("analysis_failed"));
        sqlx::query("DELETE FROM jobs")
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_failed_reanalysis_keeps_the_published_analysis(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let idea = published_idea(&pool, "parente").await;
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, status, text)
         VALUES ($1, 'objection', 'accepted', 'Risque de dépendance aux fournisseurs.')",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    app.research("Mémo [S1].").await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(raw_chat("{")))
        .mount(app.mistral.as_ref().unwrap())
        .await;
    let job = job_for(&pool, idea, AnalysisTrigger::Contribution).await;
    assert!(matches!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Err(JobError::Permanent(_))
    ));
    assert_eq!(status_of(&pool, idea).await, "published");
    let (versions, accepted): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM analyses WHERE idea_id = $1),
                (SELECT count(*) FROM contributions WHERE idea_id = $1 AND status = 'accepted')",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((versions, accepted), (1, 1));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn transient_errors_are_retried_by_the_queue(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    Mock::given(path("/v1/conversations"))
        .respond_with(ResponseTemplate::new(503))
        .mount(app.mistral.as_ref().unwrap())
        .await;
    let idea = queued_idea(&pool).await;
    let mut job = job_for(&pool, idea, AnalysisTrigger::Initial).await;
    assert!(matches!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Err(JobError::Transient(_))
    ));
    assert_eq!(status_of(&pool, idea).await, "analyzing");
    // A run interrupted in `analyzing` goes on; the last attempt gives up.
    job.attempts = job.max_attempts;
    assert!(matches!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Err(JobError::Permanent(_))
    ));
    assert_eq!(status_of(&pool, idea).await, "analysis_failed");
    assert_eq!(
        app.history(TargetKind::Idea, idea).await,
        ["analyzing", "analysis_failed"]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_budget_and_the_key_postpone_the_analysis(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[("MISTRAL_DAILY_BUDGET_EUR", "1")]).await;
    let ctx = app.job_context();
    let idea = queued_idea(&pool).await;
    let job = job_for(&pool, idea, AnalysisTrigger::Initial).await;

    let mut without_key = ctx.clone();
    without_key.mistral = None;
    assert!(matches!(
        AnalyzeHandler.handle(&without_key, &job).await,
        Ok(JobOutcome::RetryAt(_))
    ));
    assert_eq!(status_of(&pool, idea).await, "queued_analysis");

    // The research costs 1.2 € (600 000 input tokens at 2 €/Mtok): the budget of 1 € is
    // reached between the two steps.
    Mock::given(path("/v1/conversations"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(research_response("Mémo [S1].", 600_000)),
        )
        .mount(app.mistral.as_ref().unwrap())
        .await;
    let next_reset = crate::ai::budget::status(&pool, ctx.config.mistral.daily_budget)
        .await
        .unwrap()
        .next_reset;
    assert_eq!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::RetryAt(next_reset))
    );
    assert_eq!(status_of(&pool, idea).await, "analyzing");
    assert!(requests(&app, "/v1/chat/completions").await.is_empty());

    // Budget still reached: postponed before any call.
    assert_eq!(
        AnalyzeHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::RetryAt(next_reset))
    );
    assert_eq!(requests(&app, "/v1/conversations").await.len(), 1);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn stale_jobs_do_nothing(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let ctx = app.job_context();
    let published = published_idea(&pool, "parente").await;
    let rejected = idea_with_status(&pool, "second_opinion").await;
    for (idea, trigger) in [
        (published, AnalysisTrigger::Contribution),
        (rejected, AnalysisTrigger::Initial),
        (Uuid::nil(), AnalysisTrigger::Initial),
    ] {
        let job = job_for(&pool, idea, trigger).await;
        assert_eq!(
            AnalyzeHandler.handle(&ctx, &job).await,
            Ok(JobOutcome::Done)
        );
        sqlx::query("DELETE FROM jobs")
            .execute(&pool)
            .await
            .unwrap();
    }
    assert!(
        app.mistral
            .as_ref()
            .unwrap()
            .received_requests()
            .await
            .unwrap()
            .is_empty()
    );
}
