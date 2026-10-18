//! Settling rules of the moderation, against a real database and a mocked decision service.

use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::test_support::{TestApp, published_idea};

async fn contribution(pool: &PgPool, idea: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text, cgu_version, cgu_accepted_at)
         VALUES ($1, 'objection', 'Le coût de coordination est sous-estimé.', '1.0', current_date)
         RETURNING id",
    )
    .bind(idea)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn report(pool: &PgPool, idea: Uuid, reason: &str, details: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, details) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(idea)
    .bind(reason)
    .bind(details)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn screen_now(app: &TestApp, target: Target) -> Step {
    screen(
        &app.pool,
        &app.state.screener,
        &app.state.prompts,
        target,
        None,
        Actor::Worker,
        None,
    )
    .await
    .unwrap()
}

/// Seconds before the queued job with this key is due, by the database clock (negative or
/// zero: due).
async fn due_in(pool: &PgPool, key: &str) -> Option<f64> {
    sqlx::query_scalar(
        "SELECT extract(epoch FROM run_after - now())::float8 FROM jobs
         WHERE dedupe_key = $1 AND status = 'queued'",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn accepted_contribution_requests_a_reanalysis(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Contribution, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    let id = contribution(&pool, idea).await;

    let step = screen_now(&app, Target::new(TargetKind::Contribution, id)).await;
    assert_eq!(step, Step::Accepted);
    assert_eq!(
        app.history(TargetKind::Contribution, id).await,
        ["screening", "accepted"]
    );
    let (kind, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT kind, payload FROM jobs WHERE dedupe_key = $1")
            .bind(format!("analyze_idea:{idea}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "analyze_idea");
    assert_eq!(payload["trigger"], "contribution");

    // The state sent to the decision model frames the contribution with the idea summary.
    let requests = app.decision.received_requests().await.unwrap();
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        sent["state"],
        json!({
            "idee": "Résumé neutre de l'idée.",
            "type": "objection",
            "contribution": "Le coût de coordination est sous-estimé."
        })
    );

    // A second screening of the same text changes nothing.
    assert_eq!(
        screen_now(&app, Target::new(TargetKind::Contribution, id)).await,
        Step::Stale
    );
    assert_eq!(app.decision.received_requests().await.unwrap().len(), 1);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn reports_without_details_are_accepted_without_screening(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    let id = report(&pool, idea, "erreur_chiffrage", None).await;

    let step = screen_now(&app, Target::new(TargetKind::Report, id)).await;
    assert_eq!(step, Step::Accepted);
    assert!(app.decision.received_requests().await.unwrap().is_empty());
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM screenings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        due_in(&pool, &format!("review_reports:{idea}"))
            .await
            .unwrap()
            <= 0.0
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn report_reviews_are_grouped_per_day_except_problematic_content(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Report, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    sqlx::query(
        "UPDATE ideas SET last_report_review_at = now() - interval '2 hours' WHERE id = $1",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    let key = format!("review_reports:{idea}");

    // Reviewed two hours ago: the next review waits 22 hours.
    let first = report(
        &pool,
        idea,
        "source_erronee",
        Some("La source date de 2010."),
    )
    .await;
    assert_eq!(
        screen_now(&app, Target::new(TargetKind::Report, first)).await,
        Step::Accepted
    );
    let wait = due_in(&pool, &key).await.unwrap();
    assert!(wait > 21.0 * 3600.0 && wait <= 22.0 * 3600.0, "{wait}");
    // A second report joins the same review.
    let second = report(&pool, idea, "autre", Some("Le titre est trompeur.")).await;
    screen_now(&app, Target::new(TargetKind::Report, second)).await;
    let unchanged = due_in(&pool, &key).await.unwrap();
    assert!((wait - unchanged).abs() < 60.0, "{wait} {unchanged}");
    // Problematic content brings the review forward.
    let urgent = report(
        &pool,
        idea,
        "contenu_problematique",
        Some("Le texte accuse une personne."),
    )
    .await;
    screen_now(&app, Target::new(TargetKind::Report, urgent)).await;
    assert!(due_in(&pool, &key).await.unwrap() <= 0.0);
    let (jobs,): (i64,) = sqlx::query_as("SELECT count(*) FROM jobs WHERE kind = 'review_reports'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 1);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn rejected_report_and_gray_contribution(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;

    app.decision_answers(TargetKind::Report, &[("relevant", 0.1)])
        .await;
    let id = report(&pool, idea, "autre", Some("Je ne suis pas d'accord.")).await;
    let step = screen_now(&app, Target::new(TargetKind::Report, id)).await;
    assert_eq!(
        step,
        Step::Rejected {
            reason_code: "irrelevant".into()
        }
    );
    let (reason,): (Option<String>,) =
        sqlx::query_as("SELECT rejection_reason_code FROM reports WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason.as_deref(), Some("irrelevant"));
    let statement = statement::load(
        &pool,
        &app.state.prompts,
        Target::new(TargetKind::Report, id),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(statement.rule.article, "4.8");
    assert!(statement.reason_message.starts_with("Votre signalement"));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn hidden_ideas_have_a_statement(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    let target = Target::new(TargetKind::Idea, idea);
    assert_eq!(
        statement::load(&pool, &app.state.prompts, target)
            .await
            .unwrap(),
        None
    );
    sqlx::query(
        "UPDATE ideas SET status = 'hidden', hidden_at = now(), hidden_reason = 'report_review'
         WHERE id = $1",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    let statement = statement::load(&pool, &app.state.prompts, target)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(statement.measure, statement::Measure::Hidden);
    assert_eq!(statement.rule.url, "/regles#mesures");
    assert!(statement.automated);
    // The text is purged by the first daily purge 12 months after the masking (§7.3).
    let (in_a_year,): (chrono::NaiveDate,) =
        sqlx::query_as("SELECT ((now() + interval '12 months') AT TIME ZONE 'Europe/Paris')::date")
            .fetch_one(&pool)
            .await
            .unwrap();
    let purge_on = statement.purge_on.unwrap();
    assert!(
        purge_on == in_a_year || purge_on == in_a_year.succ_opt().unwrap(),
        "{purge_on}"
    );
    // A pending human review holds the purge.
    sqlx::query("INSERT INTO review_requests (target_kind, target_id) VALUES ('idea', $1)")
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    let statement = statement::load(&pool, &app.state.prompts, target)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(statement.purge_on, None);
}
