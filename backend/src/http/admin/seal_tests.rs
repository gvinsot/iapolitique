//! Sealed copy of escalated content (owner decision D3, AGENTS.md §7.3, §7.5): written with the
//! escalation, kept one year whatever happens to the idea (withdrawal by its author, 12-month
//! purge), read by the admin only with every read logged, never public nor exported, purged one
//! year after the escalation.

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::tracking::{self, TrackingToken};
use crate::domain::types::TargetKind;
use crate::jobs::reports::ReviewReportsHandler;
use crate::jobs::worker::JobHandler;
use crate::jobs::{JobKind, JobOutcome, pipeline, purge, queue};
use crate::test_support::{TestApp, admin_actions, admin_get, get_json, published_idea};

const DETAILS: &str = "Le texte menace nommément un agent.";

/// A published idea escalated by a report review: its id and its escalation.
async fn escalated(app: &TestApp) -> (Uuid, Uuid) {
    let pool = &app.pool;
    let idea = published_idea(pool, "idee-grave").await;
    sqlx::query(
        "INSERT INTO reports (idea_id, reason, details, reporter_name, reporter_email, status,
                              good_faith)
         VALUES ($1, 'contenu_problematique', $2, 'Jean Témoin', 'temoin@exemple.fr',
                 'accepted', true)",
    )
    .bind(idea)
    .bind(DETAILS)
    .execute(pool)
    .await
    .unwrap();
    app.structured(&json!({
        "outcome": "hide",
        "escalate": true,
        "explanation": "Menace caractérisée.",
        "corrections": ""
    }))
    .await;
    queue::enqueue(
        pool,
        &pipeline::review_reports_job(idea, chrono::Utc::now() - chrono::Duration::hours(1)),
    )
    .await
    .unwrap();
    let job = queue::claim(pool, "test-worker", &[JobKind::ReviewReports])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ReviewReportsHandler.handle(&app.job_context(), &job).await,
        Ok(JobOutcome::Done)
    );
    let escalation: Uuid = sqlx::query_scalar("SELECT id FROM escalations WHERE idea_id = $1")
        .bind(idea)
        .fetch_one(pool)
        .await
        .unwrap();
    (idea, escalation)
}

async fn seal_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM escalation_seals")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn an_escalation_is_sealed_and_read_by_the_admin_only(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let (idea, escalation) = escalated(&app).await;

    let (escalated_at, purge_after): (
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as("SELECT escalated_at, purge_after FROM escalation_seals")
        .fetch_one(&pool)
        .await
        .unwrap();
    let created: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT created_at FROM escalations WHERE id = $1")
            .bind(escalation)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(escalated_at, created);
    assert!(purge_after > escalated_at + chrono::Duration::days(364));

    // Listed without content and without a log.
    let (status, page) = admin_get(&app, "/api/admin/escalation-seals").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    let item = &page["items"][0];
    assert_eq!(item["escalation_id"], json!(escalation));
    assert_eq!(item["escalation_status"], "pending");
    assert_eq!(item["idea_status"], "hidden");
    assert!(item.get("content").is_none());
    assert!(admin_actions(&pool).await.is_empty());

    // Read: the content, logged in the admin journal.
    let uri = format!("/api/admin/escalations/{escalation}/seal");
    let (status, seal) = admin_get(&app, &uri).await;
    assert_eq!(status, StatusCode::OK, "{seal}");
    let content = &seal["content"];
    assert_eq!(seal["idea_id"], json!(idea));
    assert_eq!(
        content["idea"]["original_text"],
        "Mutualiser les achats de fournitures entre communes voisines."
    );
    assert_eq!(content["idea"]["title"], "Mutualiser les achats");
    assert_eq!(content["idea"]["status"], "hidden");
    assert_eq!(content["review"]["explanation"], "Menace caractérisée.");
    assert_eq!(content["review"]["escalate"], true);
    assert_eq!(content["reports"][0]["details"], DETAILS);
    assert_eq!(content["reports"][0]["reason"], "contenu_problematique");
    // Never the reporter's contact.
    assert!(!content.to_string().contains("temoin@exemple.fr"));
    assert!(!content.to_string().contains("Jean Témoin"));
    assert_eq!(
        admin_actions(&pool).await,
        [(
            "escalation.seal_read".to_owned(),
            Some(escalation.to_string())
        )]
    );
    admin_get(&app, &uri).await;
    assert_eq!(admin_actions(&pool).await.len(), 2, "every read is logged");
    let (status, body) = admin_get(
        &app,
        &format!("/api/admin/escalations/{}/seal", Uuid::nil()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "seal_not_found");
    assert_eq!(
        admin_actions(&pool).await.len(),
        2,
        "nothing read, nothing logged"
    );

    // Never on the public port, never exported.
    let (status, _, _) = get_json(&app.router, &uri, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get_json(&app.router, "/api/admin/escalation-seals", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let exported: Vec<Vec<u8>> = sqlx::query_scalar("SELECT content FROM exports")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        exported
            .iter()
            .all(|c| !String::from_utf8_lossy(c).contains(DETAILS))
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_seal_survives_the_withdrawal_and_the_purge_for_one_year(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let (idea, escalation) = escalated(&app).await;

    // The author deletes the escalated idea: withdrawn (D2), the seal stays.
    let token = TrackingToken::generate();
    tracking::insert(&pool, &token, TargetKind::Idea, idea)
        .await
        .unwrap();
    let (status, _) = app
        .post("/api/track/delete", &json!({ "token": token.expose() }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(seal_count(&pool).await, 1);

    // The withdrawn idea is purged (12 months after a withdrawal dated back here): the seal,
    // one year after the escalation only, stays readable.
    sqlx::query("UPDATE ideas SET withdrawn_at = now() - interval '13 months' WHERE id = $1")
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    let report = purge::run(&pool).await.unwrap();
    assert_eq!((report.withdrawn_ideas, report.escalation_seals), (1, 0));
    let text: Option<String> = sqlx::query_scalar("SELECT original_text FROM ideas WHERE id = $1")
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(text, None);
    let uri = format!("/api/admin/escalations/{escalation}/seal");
    let (status, seal) = admin_get(&app, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        seal["content"]["idea"]["original_text"],
        "Mutualiser les achats de fournitures entre communes voisines."
    );
    let (_, page) = admin_get(&app, "/api/admin/escalation-seals").await;
    assert_eq!(page["items"][0]["idea_status"], "purged");

    // Even without the idea nor the escalation rows (no foreign key).
    sqlx::query("DELETE FROM escalations")
        .execute(&pool)
        .await
        .unwrap();
    let (_, page) = admin_get(&app, "/api/admin/escalation-seals").await;
    assert_eq!(page["items"][0]["escalation_status"], Value::Null);
    assert_eq!(admin_get(&app, &uri).await.0, StatusCode::OK);

    // One year after the escalation, the daily purge deletes it.
    sqlx::query(
        "UPDATE escalation_seals SET escalated_at = now() - interval '13 months',
             purge_after = (now() - interval '13 months') + interval '1 year'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let report = purge::run(&pool).await.unwrap();
    assert_eq!(report.escalation_seals, 1);
    assert_eq!(seal_count(&pool).await, 0);
    assert_eq!(admin_get(&app, &uri).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_second_review_of_a_pending_escalation_seals_nothing_new(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    let (_, escalation) = escalated(&app).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(
        !crate::db::escalation_seals::seal(&mut tx, escalation)
            .await
            .unwrap()
    );
    assert!(
        !crate::db::escalation_seals::seal(&mut tx, Uuid::nil())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(seal_count(&pool).await, 1);
}
