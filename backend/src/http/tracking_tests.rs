//! Integration tests of the tracking page API: real database, mocked decision service.

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::tracking::{self, TrackingToken};
use crate::domain::types::TargetKind;
use crate::test_support::{TestApp, published_idea};

const IDEA_TEXT: &str = "Mutualiser les achats de fournitures entre les communes voisines \
    pour obtenir de meilleurs prix.";

async fn submit_idea(app: &TestApp) -> String {
    let (status, body) = app
        .post(
            "/api/ideas",
            &json!({
                "text": IDEA_TEXT,
                "accept_licence": true,
                "accept_publication": true,
                "altcha": app.altcha(),
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["tracking_token"].as_str().unwrap().to_owned()
}

async fn track(app: &TestApp, token: &str) -> (StatusCode, Value) {
    app.post("/api/track", &json!({ "token": token })).await
}

async fn today(pool: &PgPool) -> String {
    let day: chrono::NaiveDate =
        sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Paris')::date")
            .fetch_one(pool)
            .await
            .unwrap();
    day.to_string()
}

/// A token for an existing text, as issued at its submission.
async fn token_for(pool: &PgPool, kind: TargetKind, id: Uuid) -> String {
    let token = TrackingToken::generate();
    tracking::insert(pool, &token, kind, id).await.unwrap();
    token.into_inner()
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn tracking_an_accepted_idea(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[]).await;
    let token = submit_idea(&app).await;

    let response = app
        .post_response("/api/track", &json!({ "token": token }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[axum::http::header::CACHE_CONTROL],
        "no-store"
    );
    let (_, body) = track(&app, &token).await;
    assert_eq!(
        body,
        json!({
            "kind": "idea",
            "status": "accepted",
            "submitted_on": today(&pool).await,
            "idea": null,
            "decision": null,
            "report_outcome": null,
            "review_request": null,
        })
    );
    // No internal identifier leaks.
    let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM ideas")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!body.to_string().contains(&id.to_string()));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn unknown_and_malformed_tokens_look_alike(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let unknown = TrackingToken::generate().into_inner();
    let mut answers = Vec::new();
    for token in [
        unknown.as_str(),
        "",
        "pas-un-jeton",
        &unknown[..42],
        "' OR '1'='1' -- AAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        for uri in ["/api/track", "/api/track/delete"] {
            let (status, body) = app.post(uri, &json!({ "token": token })).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri} {token:?}");
            answers.push(body);
        }
        let (status, body) = app
            .post("/api/track/review-request", &json!({ "token": token }))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        answers.push(body);
    }
    assert!(answers.windows(2).all(|w| w[0] == w[1]), "{answers:?}");
    assert_eq!(answers[0]["error"]["code"], "tracking_not_found");

    // Strict bodies.
    let (status, body) = app
        .post("/api/track", &json!({ "token": unknown, "id": 1 }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "invalid_body");

    // An expired token is unknown.
    let app_token = TrackingToken::generate();
    tracking::insert(&pool, &app_token, TargetKind::Idea, Uuid::new_v4())
        .await
        .unwrap();
    sqlx::query("UPDATE tracking_tokens SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        track(&app, app_token.expose()).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn tracking_requests_are_rate_limited(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    for _ in 0..crate::antispam::rate_limit::TRACKING_PER_HOUR {
        assert_eq!(track(&app, "x").await.0, StatusCode::NOT_FOUND);
    }
    let (status, body) = track(&app, "x").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], "rate_limited");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn rejected_idea_shows_its_statement_and_accepts_one_review_request(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("realism", 0.05)])
        .await;
    let token = submit_idea(&app).await;

    let (_, body) = track(&app, &token).await;
    assert_eq!(body["status"], "rejected");
    let decision = &body["decision"];
    assert_eq!(decision["measure"], "rejected");
    assert_eq!(decision["reason_code"], "unrealistic");
    assert_eq!(decision["rule"]["url"], "/regles#critere-realisme");
    assert_eq!(decision["automated"], true);
    assert!(decision["purge_on"].is_string());

    let (status, body) = app
        .post(
            "/api/track/review-request",
            &json!({
                "token": token,
                "message": "La mesure existe déjà ailleurs ; écrivez-moi à moi@exemple.fr"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body,
        json!({ "requested_on": today(&pool).await, "status": "pending" })
    );
    let (kind, message): (String, String) =
        sqlx::query_as("SELECT target_kind, message FROM review_requests")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "idea");
    assert!(message.ends_with("[donnée masquée]"), "{message}");

    // Once per text.
    let (status, body) = app
        .post("/api/track/review-request", &json!({ "token": token }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "review_already_requested");

    // The pending review holds the purge.
    let (_, body) = track(&app, &token).await;
    assert_eq!(body["review_request"]["status"], "pending");
    assert_eq!(body["decision"]["purge_on"], Value::Null);

    // Message bounds.
    let (status, body) = app
        .post(
            "/api/track/review-request",
            &json!({ "token": token, "message": "x".repeat(1_001) }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "text_too_long");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn reviews_are_only_for_automated_decisions(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[]).await;
    let token = submit_idea(&app).await;
    let (status, body) = app
        .post("/api/track/review-request", &json!({ "token": token }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "review_not_available");

    // A hidden idea can be reviewed.
    let idea = published_idea(&pool, "mutualiser-achats").await;
    sqlx::query(
        "UPDATE ideas SET status = 'hidden', hidden_at = now(), hidden_reason = 'report_review'
         WHERE id = $1",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    let token = token_for(&pool, TargetKind::Idea, idea).await;
    let (_, body) = track(&app, &token).await;
    assert_eq!(body["status"], "hidden");
    assert_eq!(body["decision"]["measure"], "hidden");
    // No link to a masked idea.
    assert_eq!(body["idea"], Value::Null);
    let (status, _) = app
        .post("/api/track/review-request", &json!({ "token": token }))
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn contributions_and_reports_link_to_their_idea(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    let contribution: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text) VALUES ($1, 'amelioration', 'Texte')
         RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO report_reviews (idea_id, outcome, explanation, model, prompt_version)
         VALUES ($1, 'maintain', 'Analyse correcte.', 'm', '1') RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    let report: Uuid = sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, status, review_id) VALUES ($1, 'autre', 'reviewed', $2)
         RETURNING id",
    )
    .bind(idea)
    .bind(review)
    .fetch_one(&pool)
    .await
    .unwrap();

    let link = json!({ "slug": "mutualiser-achats", "title": "Mutualiser les achats" });
    let token = token_for(&pool, TargetKind::Contribution, contribution).await;
    let (_, body) = track(&app, &token).await;
    assert_eq!(body["kind"], "contribution");
    assert_eq!(body["status"], "submitted");
    assert_eq!(body["idea"], link);
    let token = token_for(&pool, TargetKind::Report, report).await;
    let (_, body) = track(&app, &token).await;
    assert_eq!(body["kind"], "report");
    assert_eq!(body["status"], "reviewed");
    assert_eq!(body["report_outcome"], "maintain");
    assert_eq!(body["idea"], link);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn deleting_a_pending_idea_erases_it_and_leaves_a_tombstone(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3)])
        .await;
    let token = submit_idea(&app).await;
    let (_, body) = track(&app, &token).await;
    assert_eq!(body["status"], "second_opinion");
    sqlx::query("UPDATE screenings SET second_opinion_explanation = 'paraphrase'")
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .post_response("/api/track/delete", &json!({ "token": token }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[axum::http::header::CACHE_CONTROL],
        "no-store"
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM ideas").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM idea_events").await, 0);
    // The queued second opinion is gone; the scores stay for calibration, without paraphrase.
    assert_eq!(count(&pool, "SELECT count(*) FROM jobs").await, 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM screenings WHERE second_opinion_explanation IS NULL"
        )
        .await,
        1
    );

    let (status, body) = track(&app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "deleted");
    assert_eq!(body["kind"], "idea");
    assert_eq!(body["submitted_on"], today(&pool).await);
    // Idempotent; nothing left to review.
    let (status, body) = app
        .post("/api/track/delete", &json!({ "token": token }))
        .await;
    assert_eq!(
        (status, &body["status"]),
        (StatusCode::OK, &json!("deleted"))
    );
    let (status, body) = app
        .post("/api/track/review-request", &json!({ "token": token }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "review_not_available");
    // The tombstone expires after 30 days.
    let days: f64 = sqlx::query_scalar(
        "SELECT (extract(epoch FROM expires_at - now()) / 86400)::float8 FROM tracking_tokens",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((29.9..=30.0).contains(&days), "{days}");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn deleting_a_published_idea_unpublishes_everything_attached(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    let author = token_for(&pool, TargetKind::Idea, idea).await;
    let contribution: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text) VALUES ($1, 'objection', 'Texte')
         RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    let contributor = token_for(&pool, TargetKind::Contribution, contribution).await;
    let report: Uuid = sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, details, reporter_email)
         VALUES ($1, 'autre', 'Précision', 'r@exemple.fr') RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    let reporter = token_for(&pool, TargetKind::Report, report).await;
    let duplicate: Uuid = sqlx::query_scalar(
        "INSERT INTO ideas (original_text, status, parent_idea_id)
         VALUES ('Même idée, autrement dite.', 'merged', $1) RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, source_idea_id) VALUES ($1, 'doublon', $2)",
    )
    .bind(idea)
    .bind(duplicate)
    .execute(&pool)
    .await
    .unwrap();
    let duplicate_author = token_for(&pool, TargetKind::Idea, duplicate).await;
    sqlx::query(
        "INSERT INTO jobs (kind, payload, dedupe_key) VALUES
             ('analyze_idea', jsonb_build_object('idea_id', $1::text), 'a'),
             ('screen_report', jsonb_build_object('target_kind', 'report', 'target_id', $2::text), 'b')",
    )
    .bind(idea)
    .bind(report)
    .execute(&pool)
    .await
    .unwrap();

    // The duplicate's author sees the idea it was merged into.
    let (_, body) = track(&app, &duplicate_author).await;
    assert_eq!(body["status"], "merged");
    assert_eq!(body["idea"]["slug"], "mutualiser-achats");

    let (status, _) = app
        .post("/api/track/delete", &json!({ "token": author }))
        .await;
    assert_eq!(status, StatusCode::OK);
    for (table, left) in [
        ("SELECT count(*) FROM ideas", 0),
        ("SELECT count(*) FROM analyses", 0),
        ("SELECT count(*) FROM contributions", 0),
        ("SELECT count(*) FROM reports", 0),
        ("SELECT count(*) FROM tracking_tokens", 1),
        (
            "SELECT count(*) FROM jobs WHERE kind <> 'generate_exports'",
            0,
        ),
        (
            "SELECT count(*) FROM jobs WHERE kind = 'generate_exports'",
            1,
        ),
    ] {
        assert_eq!(count(&pool, table).await, left, "{table}");
    }
    assert_eq!(track(&app, &author).await.1["status"], "deleted");
    for other in [contributor, reporter, duplicate_author] {
        assert_eq!(track(&app, &other).await.0, StatusCode::NOT_FOUND);
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn deleting_a_merged_duplicate_or_a_contribution(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    sqlx::query("UPDATE ideas SET proposal_count = 2 WHERE id = $1")
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    let duplicate: Uuid = sqlx::query_scalar(
        "INSERT INTO ideas (original_text, status, parent_idea_id)
         VALUES ('Même idée.', 'merged', $1) RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, source_idea_id) VALUES ($1, 'doublon', $2)",
    )
    .bind(idea)
    .bind(duplicate)
    .execute(&pool)
    .await
    .unwrap();
    let token = token_for(&pool, TargetKind::Idea, duplicate).await;
    assert_eq!(
        app.post("/api/track/delete", &json!({ "token": token }))
            .await
            .0,
        StatusCode::OK
    );
    let (count_left, proposals): (i64, i32) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM contributions), proposal_count FROM ideas WHERE id = $1",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((count_left, proposals), (0, 1));

    // An integrated contribution leaves the analyses and the exports.
    let analysis: Uuid = sqlx::query_scalar("SELECT current_analysis_id FROM ideas WHERE id = $1")
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
    let contribution: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text, status, integrated_in_analysis_id,
                                    neutral_summary, integrated_at)
         VALUES ($1, 'objection', 'Texte', 'integrated', $2, 'Résumé', now()) RETURNING id",
    )
    .bind(idea)
    .bind(analysis)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE analyses SET content = jsonb_build_object('contributions_considered',
             jsonb_build_array(
                 jsonb_build_object('contribution_id', $1::text, 'summary', 'Résumé'),
                 jsonb_build_object('contribution_id', 'autre', 'summary', 'Autre')))
         WHERE id = $2",
    )
    .bind(contribution)
    .bind(analysis)
    .execute(&pool)
    .await
    .unwrap();
    let token = token_for(&pool, TargetKind::Contribution, contribution).await;
    assert_eq!(
        app.post("/api/track/delete", &json!({ "token": token }))
            .await
            .0,
        StatusCode::OK
    );
    let considered: Value =
        sqlx::query_scalar("SELECT content->'contributions_considered' FROM analyses")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        considered,
        json!([{ "contribution_id": "autre", "summary": "Autre" }])
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM contributions").await, 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind = 'generate_exports'"
        )
        .await,
        1
    );
    assert_eq!(track(&app, &token).await.1["status"], "deleted");
}

/// AUTHZ-1 (phase 1): an author who deletes an idea escalated for serious content leaves a
/// trace in the admin journal (identifiers only), so that the PHAROS queue is not silently
/// emptied.
#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn deleting_an_escalated_idea_leaves_a_trace_for_the_admin(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    sqlx::query(
        "UPDATE ideas SET status = 'hidden', hidden_at = now(), hidden_reason = 'escalation'
         WHERE id = $1",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    let escalation: Uuid =
        sqlx::query_scalar("INSERT INTO escalations (idea_id) VALUES ($1) RETURNING id")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    let author = token_for(&pool, TargetKind::Idea, idea).await;

    let (status, _) = app
        .post("/api/track/delete", &json!({ "token": author }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(count(&pool, "SELECT count(*) FROM ideas").await, 0);
    let actions: Vec<(String, Option<String>, Option<String>, Value)> = sqlx::query_as(
        "SELECT action, target_kind, target_id, details FROM admin_actions ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    let (action, kind, target, details) = &actions[0];
    assert_eq!(action, "escalation.deleted_by_author");
    assert_eq!(kind.as_deref(), Some("idea"));
    assert_eq!(target.as_deref(), Some(idea.to_string().as_str()));
    assert_eq!(details["escalation_id"], json!(escalation));
    assert!(details["escalation_created_at"].is_string());
    assert!(!details.to_string().contains("Mutualiser"));

    // Without a pending escalation, a deletion leaves no admin trace.
    let other = published_idea(&pool, "autre-idee").await;
    let other_author = token_for(&pool, TargetKind::Idea, other).await;
    app.post("/api/track/delete", &json!({ "token": other_author }))
        .await;
    assert_eq!(count(&pool, "SELECT count(*) FROM admin_actions").await, 1);
}

/// PRIV-7: the author of a merged duplicate who deletes it also removes the neutral summary of
/// its text from every analysis of the parent idea.
#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn deleting_a_merged_duplicate_removes_its_summary_from_the_parent(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "mutualiser-achats").await;
    let duplicate: Uuid = sqlx::query_scalar(
        "INSERT INTO ideas (original_text, status, parent_idea_id)
         VALUES ('Même idée.', 'merged', $1) RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    let doublon: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, source_idea_id) VALUES ($1, 'doublon', $2)
         RETURNING id",
    )
    .bind(idea)
    .bind(duplicate)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Two analysis versions mention it.
    let first: Uuid = sqlx::query_scalar("SELECT current_analysis_id FROM ideas WHERE id = $1")
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO analyses
         SELECT (jsonb_populate_record(NULL::analyses, to_jsonb(a)
                 || jsonb_build_object('id', gen_random_uuid(), 'version', a.version + 1))).*
         FROM analyses a WHERE a.id = $1",
    )
    .bind(first)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE analyses SET content = jsonb_build_object('contributions_considered',
             jsonb_build_array(
                 jsonb_build_object('contribution_id', $1::text, 'summary', 'Même idée.'),
                 jsonb_build_object('contribution_id', 'autre', 'summary', 'Autre')))
         WHERE idea_id = $2",
    )
    .bind(doublon)
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();

    let token = token_for(&pool, TargetKind::Idea, duplicate).await;
    assert_eq!(
        app.post("/api/track/delete", &json!({ "token": token }))
            .await
            .0,
        StatusCode::OK
    );
    let considered: Vec<Value> = sqlx::query_scalar(
        "SELECT content->'contributions_considered' FROM analyses WHERE idea_id = $1",
    )
    .bind(idea)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(considered.len(), 2);
    for list in considered {
        assert_eq!(
            list,
            json!([{ "contribution_id": "autre", "summary": "Autre" }])
        );
    }
}
