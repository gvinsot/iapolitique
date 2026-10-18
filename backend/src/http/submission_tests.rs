//! Integration tests of the public submissions: real database, mocked decision service and
//! Mistral API.

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::db::tracking::token_hash;
use crate::domain::types::TargetKind;
use crate::test_support::TestApp;

const IDEA_TEXT: &str = "Mutualiser les achats de fournitures entre les communes voisines \
    pour obtenir de meilleurs prix. Contact : jean.dupont@example.org";

fn idea_body(app: &TestApp) -> Value {
    json!({
        "text": IDEA_TEXT,
        "admin_level": "commune",
        "territory": "Métropole de Lyon",
        "accept_licence": true,
        "accept_publication": true,
        "altcha": app.altcha(),
    })
}

async fn only_idea(pool: &PgPool) -> (Uuid, Value) {
    let (id, row): (Uuid, Value) = sqlx::query_as("SELECT id, to_jsonb(i) FROM ideas i")
        .fetch_one(pool)
        .await
        .unwrap();
    (id, row)
}

fn assert_token(body: &Value) -> String {
    let token = body["tracking_token"].as_str().unwrap().to_owned();
    assert!(token_hash(&token).is_some(), "malformed token {token}");
    token
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn accepted_idea_is_stored_masked_and_queued_for_dedup(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[]).await;

    let (status, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["reason_code"], Value::Null);
    assert_eq!(body["statement"], Value::Null);
    let token = assert_token(&body);

    let (id, idea) = only_idea(&pool).await;
    assert_eq!(idea["status"], "accepted");
    let text = idea["original_text"].as_str().unwrap();
    assert!(!text.contains("jean.dupont"), "unmasked text stored");
    assert!(text.ends_with("Contact : [donnée masquée]"));
    assert_eq!(idea["submitted_admin_level"], "commune");
    assert_eq!(idea["submitted_territory"], "Métropole de Lyon");
    assert_eq!(idea["cgu_version"], crate::domain::rules::CGU_VERSION);
    let (today,): (chrono::NaiveDate,) =
        sqlx::query_as("SELECT (now() AT TIME ZONE 'Europe/Paris')::date")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(idea["cgu_accepted_at"], today.to_string());
    assert_eq!(
        app.history(TargetKind::Idea, id).await,
        ["submitted", "screening", "accepted"]
    );

    // Only the hash of the token is stored, pointing at the idea.
    let (kind, target): (String, Uuid) =
        sqlx::query_as("SELECT target_kind, target_id FROM tracking_tokens WHERE token_hash = $1")
            .bind(&token_hash(&token).unwrap()[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((kind.as_str(), target), ("idea", id));

    // The screening was recorded on the masked text; the retry job is gone, dedup is queued.
    let requests = app.decision.received_requests().await.unwrap();
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(
        sent["state"]
            .as_str()
            .unwrap()
            .ends_with("[donnée masquée]")
    );
    assert_eq!(
        app.jobs().await,
        [(
            "dedup_idea".to_owned(),
            Some(format!("dedup_idea:{id}")),
            false
        )]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn rejected_idea_gets_a_statement_of_reasons(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("on_topic", 0.05), ("realism", 0.1)])
        .await;

    let (status, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "rejected");
    // First failing criterion in grid order.
    assert_eq!(body["reason_code"], "off_topic");
    let message = body["reason_message"].as_str().unwrap();
    assert!(message.starts_with("Votre texte ne propose pas de mesure"));
    let statement = &body["statement"];
    assert_eq!(statement["measure"], "rejected");
    assert_eq!(statement["reason_code"], "off_topic");
    assert_eq!(statement["reason_message"], message);
    assert_eq!(
        statement["rule"],
        json!({
            "article": "4.5",
            "title": "Une mesure pour les finances publiques",
            "url": "/regles#critere-sujet"
        })
    );
    assert_eq!(statement["automated"], true);
    assert_eq!(statement["second_opinion"], false);
    let decided: chrono::NaiveDate = statement["decided_on"].as_str().unwrap().parse().unwrap();
    let purge: chrono::NaiveDate = statement["purge_on"].as_str().unwrap().parse().unwrap();
    let days = (purge - decided).num_days();
    assert!((30..=31).contains(&days), "{days}");

    let (id, idea) = only_idea(&pool).await;
    assert_eq!(idea["status"], "rejected");
    assert_eq!(idea["rejection_reason_code"], "off_topic");
    assert!(idea["rejected_at"].is_string());
    assert_eq!(
        app.history(TargetKind::Idea, id).await,
        ["submitted", "screening", "rejected"]
    );
    assert!(app.jobs().await.is_empty());
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn gray_zone_without_mistral_waits_for_the_job(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3)])
        .await;

    let (status, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "second_opinion");
    assert_token(&body);
    let (id, _) = only_idea(&pool).await;
    // Due at once: no inline attempt.
    assert_eq!(
        app.jobs().await,
        [(
            "second_opinion".to_owned(),
            Some(format!("second_opinion:idea:{id}")),
            false
        )]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn gray_zone_is_settled_inline_by_mistral(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3), ("realism", 0.4)])
        .await;
    app.second_opinion(
        json!({ "decision": "accept", "reason_code": "none", "explanation": "Mesure légale." }),
        Duration::ZERO,
    )
    .await;

    let (status, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "accepted");

    let (id, _) = only_idea(&pool).await;
    assert_eq!(
        app.history(TargetKind::Idea, id).await,
        ["submitted", "screening", "second_opinion", "accepted"]
    );
    let screening: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM screenings s")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(screening["outcome"], "gray_zone");
    assert_eq!(screening["second_opinion_decision"], "accept");
    assert_eq!(screening["second_opinion_reason_code"], Value::Null);
    assert_eq!(screening["second_opinion_model"], "mistral-medium-test");
    assert_eq!(screening["final_decision"], "accept");

    // The prompt names the doubtful criteria by reason code and carries the masked text.
    let requests = app
        .mistral
        .as_ref()
        .unwrap()
        .received_requests()
        .await
        .unwrap();
    assert_eq!(requests.len(), 1);
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let user = sent["messages"][1]["content"].as_str().unwrap();
    assert!(
        user.contains("premier filtre : unlawful, unrealistic"),
        "{user}"
    );
    assert!(user.contains("Section : idée"));
    assert!(!user.contains("jean.dupont"));
    let purposes: Vec<(String, String)> =
        sqlx::query_as("SELECT provider, purpose FROM ai_calls ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        purposes,
        [
            ("decision_service".to_owned(), "screening".to_owned()),
            ("mistral".to_owned(), "second_opinion".to_owned())
        ]
    );
    // The fallback job was removed, dedup is queued.
    assert_eq!(
        app.jobs()
            .await
            .into_iter()
            .map(|(kind, _, _)| kind)
            .collect::<Vec<_>>(),
        ["dedup_idea"]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn second_opinion_rejection_is_explained(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("personal_data", 0.5)])
        .await;
    app.second_opinion(
        json!({
            "decision": "reject",
            "reason_code": "personal_data",
            "explanation": "Le texte désigne un agent."
        }),
        Duration::ZERO,
    )
    .await;

    let (_, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(body["status"], "rejected");
    assert_eq!(body["reason_code"], "personal_data");
    assert_eq!(body["statement"]["second_opinion"], true);
    assert_eq!(body["statement"]["rule"]["article"], "4.4");
    let (_, idea) = only_idea(&pool).await;
    assert_eq!(idea["rejection_reason_code"], "personal_data");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn invalid_second_opinion_is_left_to_the_job(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3)])
        .await;
    // `off_topic` exists, but acceptance requires `none`: invalid output.
    app.second_opinion(
        json!({ "decision": "accept", "reason_code": "off_topic", "explanation": "?" }),
        Duration::ZERO,
    )
    .await;

    let (_, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(body["status"], "second_opinion");
    // The fallback job is delayed while the api tries inline.
    let jobs = app.jobs().await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, "second_opinion");
    assert!(jobs[0].2, "fallback job must be delayed");
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM ai_calls WHERE provider = 'mistral'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "invalid_output");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn slow_second_opinion_answers_pending_then_settles_in_background(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[]).await;
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3)])
        .await;
    app.second_opinion(
        json!({ "decision": "accept", "reason_code": "none", "explanation": "Mesure légale." }),
        Duration::from_secs(11),
    )
    .await;

    let started = std::time::Instant::now();
    let (_, body) = app.post("/api/ideas", &idea_body(&app)).await;
    let waited = started.elapsed();
    assert_eq!(body["status"], "second_opinion");
    assert!(
        waited >= Duration::from_secs(10) && waited < Duration::from_secs(11),
        "{waited:?}"
    );
    // The call goes on and settles the idea.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let (_, idea) = only_idea(&pool).await;
        if idea["status"] == "accepted" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "not settled in background"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn exhausted_budget_skips_the_inline_second_opinion(pool: PgPool) {
    let app = TestApp::new(pool.clone(), true, &[("MISTRAL_DAILY_BUDGET_EUR", "1")]).await;
    sqlx::query(
        "INSERT INTO ai_calls (provider, purpose, model, cost_micro_eur, latency_ms, status)
         VALUES ('mistral', 'analysis_research', 'm', 2000000, 10, 'success')",
    )
    .execute(&pool)
    .await
    .unwrap();
    app.decision_answers(TargetKind::Idea, &[("lawful", 0.3)])
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(app.mistral.as_ref().unwrap())
        .await;

    let (_, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(body["status"], "second_opinion");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn unavailable_decision_service_keeps_the_idea_submitted(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_down().await;

    let (status, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "submitted");
    assert_token(&body);
    let (id, idea) = only_idea(&pool).await;
    assert_eq!(idea["status"], "submitted");
    assert_eq!(app.history(TargetKind::Idea, id).await, ["submitted"]);
    // The screening job retries later.
    assert_eq!(
        app.jobs().await,
        [(
            "screen_idea".to_owned(),
            Some(format!("screen:idea:{id}")),
            true
        )]
    );
    let (error,): (Option<String>,) = sqlx::query_as("SELECT error_code FROM screenings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(error.as_deref(), Some("http_503"));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn invalid_submissions_are_refused_before_any_storage(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[]).await;

    for (patch, status, code) in [
        (
            json!({ "accept_licence": false }),
            StatusCode::BAD_REQUEST,
            "consent_required",
        ),
        (
            json!({ "accept_publication": null }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
        ),
        (
            json!({ "text": "Trop court." }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "text_too_short",
        ),
        (
            json!({ "text": "x".repeat(4_001) }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "text_too_long",
        ),
        (
            json!({ "territory": "t".repeat(121) }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "field_too_long",
        ),
        (
            json!({ "admin_level": "planete" }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
        ),
        (
            json!({ "altcha": "" }),
            StatusCode::BAD_REQUEST,
            "altcha_missing",
        ),
        (
            json!({ "altcha": "bm9wZQ==" }),
            StatusCode::BAD_REQUEST,
            "altcha_missing",
        ),
    ] {
        let mut body = idea_body(&app);
        for (key, value) in patch.as_object().unwrap() {
            body[key] = value.clone();
        }
        let (actual, response) = app.post("/api/ideas", &body).await;
        assert_eq!(actual, status, "{patch}: {response}");
        assert_eq!(response["error"]["code"], code, "{patch}");
        assert!(!response["error"]["message"].as_str().unwrap().is_empty());
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM ideas")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // A replayed ALTCHA solution is refused.
    let body = idea_body(&app);
    assert_eq!(app.post("/api/ideas", &body).await.0, StatusCode::CREATED);
    let (status, response) = app.post("/api/ideas", &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["error"]["code"], "altcha_replayed");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn bodies_are_strict_and_answers_never_cached(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Idea, &[]).await;
    crate::test_support::published_idea(&pool, "mutualiser-achats").await;

    // Unknown fields are refused (AGENTS.md §14.1).
    for (uri, mut body) in [
        ("/api/ideas", idea_body(&app)),
        (
            "/api/ideas/mutualiser-achats/contributions",
            contribution_body(&app),
        ),
        ("/api/ideas/mutualiser-achats/reports", report_body(&app)),
    ] {
        body["ip"] = json!("1.2.3.4");
        let (status, response) = app.post(uri, &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        assert_eq!(response["error"]["code"], "invalid_body");
    }

    // Injection attempts in the slug are plain unknown slugs.
    for slug in [
        "x'%20OR%20'1'='1",
        "mutualiser-achats%3B--",
        "..%2F..%2Fetc",
    ] {
        let (status, response) = app
            .post(&format!("/api/ideas/{slug}/reports"), &report_body(&app))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{slug}");
        assert_eq!(response["error"]["code"], "idea_not_found");
    }

    // The answer carries the token: never cached.
    let response = app.post_response("/api/ideas", &idea_body(&app)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response.headers()[axum::http::header::CACHE_CONTROL],
        "no-store"
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn ideas_are_rate_limited(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[("RATE_LIMIT_IDEAS_PER_HOUR", "2")]).await;
    app.decision_answers(TargetKind::Idea, &[]).await;
    for _ in 0..2 {
        assert_eq!(
            app.post("/api/ideas", &idea_body(&app)).await.0,
            StatusCode::CREATED
        );
    }
    // A refused body does not count…
    let mut invalid = idea_body(&app);
    invalid["accept_licence"] = json!(false);
    assert_eq!(
        app.post("/api/ideas", &invalid).await.0,
        StatusCode::BAD_REQUEST
    );
    // …but the third valid idea of the hour is limited.
    let (status, body) = app.post("/api/ideas", &idea_body(&app)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], "rate_limited");
}

// ---------------------------------------------------------------------------------------------
// Contributions and reports
// ---------------------------------------------------------------------------------------------

const CONTRIBUTION_TEXT: &str = "Le gain est surestimé : les communes achètent déjà via \
    l'UGAP. Appelez-moi au 06 12 34 56 78.";

fn contribution_body(app: &TestApp) -> Value {
    json!({
        "kind": "objection",
        "text": CONTRIBUTION_TEXT,
        "accept_licence": true,
        "accept_publication": true,
        "altcha": app.altcha(),
    })
}

fn report_body(app: &TestApp) -> Value {
    json!({
        "reason": "erreur_chiffrage",
        "details": "Le chiffre de 2 M€ ne correspond pas à la source citée.",
        "reporter_name": "Camille Martin",
        "reporter_email": "camille@exemple.fr",
        "good_faith": true,
        "altcha": app.altcha(),
    })
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn accepted_contribution_is_masked_and_triggers_a_reanalysis(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Contribution, &[]).await;
    let idea = crate::test_support::published_idea(&pool, "mutualiser-achats").await;

    let (status, body) = app
        .post(
            "/api/ideas/mutualiser-achats/contributions",
            &contribution_body(&app),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "accepted");
    let token = assert_token(&body);

    let row: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM contributions c")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row["idea_id"], idea.to_string());
    assert_eq!(row["kind"], "objection");
    assert_eq!(row["status"], "accepted");
    assert!(!row["text"].as_str().unwrap().contains("06 12"));
    assert_eq!(row["cgu_version"], crate::domain::rules::CGU_VERSION);
    assert!(row["cgu_accepted_at"].is_string());
    let id: Uuid = row["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        app.history(TargetKind::Contribution, id).await,
        ["submitted", "screening", "accepted"]
    );
    let (kind, target): (String, Uuid) =
        sqlx::query_as("SELECT target_kind, target_id FROM tracking_tokens WHERE token_hash = $1")
            .bind(&token_hash(&token).unwrap()[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((kind.as_str(), target), ("contribution", id));
    assert_eq!(
        app.jobs().await,
        [(
            "analyze_idea".to_owned(),
            Some(format!("analyze_idea:{idea}")),
            false
        )]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn contributions_need_a_published_idea_and_a_valid_body(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Contribution, &[("relevant", 0.1)])
        .await;
    crate::test_support::published_idea(&pool, "mutualiser-achats").await;
    sqlx::query(
        "INSERT INTO ideas (original_text, slug) VALUES ('Texte en attente.', 'pas-publiee')",
    )
    .execute(&pool)
    .await
    .unwrap();

    for slug in ["inconnue", "pas-publiee", "Majuscules%20et%20espaces"] {
        let (status, body) = app
            .post(
                &format!("/api/ideas/{slug}/contributions"),
                &contribution_body(&app),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{slug}");
        assert_eq!(body["error"]["code"], "idea_not_found");
    }
    for (patch, code) in [
        (json!({ "kind": "doublon" }), "invalid_body"),
        (json!({ "text": "Trop court." }), "text_too_short"),
        (json!({ "text": "x".repeat(2_001) }), "text_too_long"),
        (json!({ "accept_publication": false }), "consent_required"),
    ] {
        let mut body = contribution_body(&app);
        for (key, value) in patch.as_object().unwrap() {
            body[key] = value.clone();
        }
        let (_, response) = app
            .post("/api/ideas/mutualiser-achats/contributions", &body)
            .await;
        assert_eq!(response["error"]["code"], code, "{patch}");
    }

    // Irrelevant contribution: refused with its rule.
    let (status, body) = app
        .post(
            "/api/ideas/mutualiser-achats/contributions",
            &contribution_body(&app),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "rejected");
    assert_eq!(body["reason_code"], "irrelevant");
    assert_eq!(
        body["statement"]["rule"]["url"],
        "/regles#critere-pertinence"
    );
    assert!(app.jobs().await.is_empty());
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn contributions_are_rate_limited(pool: PgPool) {
    let app = TestApp::new(
        pool.clone(),
        false,
        &[("RATE_LIMIT_CONTRIBUTIONS_PER_DAY", "1")],
    )
    .await;
    app.decision_answers(TargetKind::Contribution, &[]).await;
    crate::test_support::published_idea(&pool, "mutualiser-achats").await;
    let uri = "/api/ideas/mutualiser-achats/contributions";
    assert_eq!(
        app.post(uri, &contribution_body(&app)).await.0,
        StatusCode::CREATED
    );
    let (status, body) = app.post(uri, &contribution_body(&app)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("1 contribution par jour")
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn report_is_screened_and_keeps_the_optional_contact(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Report, &[]).await;
    let idea = crate::test_support::published_idea(&pool, "mutualiser-achats").await;

    let mut body = report_body(&app);
    body["details"] = json!("Erreur de calcul, me joindre : camille.martin@exemple.fr");
    let (status, response) = app
        .post("/api/ideas/mutualiser-achats/reports", &body)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    assert_eq!(response["status"], "accepted");
    assert_token(&response);

    let row: Value = sqlx::query_scalar("SELECT to_jsonb(r) FROM reports r")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row["reason"], "erreur_chiffrage");
    assert_eq!(row["status"], "accepted");
    assert!(!row["details"].as_str().unwrap().contains("camille.martin"));
    assert_eq!(row["reporter_name"], "Camille Martin");
    assert_eq!(row["reporter_email"], "camille@exemple.fr");
    assert_eq!(row["good_faith"], true);
    // The decision model sees the idea summary, the reason label and the masked details, never
    // the reporter's contact.
    let requests = app.decision.received_requests().await.unwrap();
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(sent["state"]["motif"], "Erreur de chiffrage");
    assert!(!requests[0].body.windows(7).any(|w| w == b"Camille"));
    assert_eq!(
        app.jobs().await,
        [(
            "review_reports".to_owned(),
            Some(format!("review_reports:{idea}")),
            false
        )]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn report_without_details_skips_the_screening(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    crate::test_support::published_idea(&pool, "mutualiser-achats").await;
    let body = json!({
        "reason": "doublon",
        "precision": "  ",
        "good_faith": true,
        "altcha": app.altcha(),
    });
    let (status, response) = app
        .post("/api/ideas/mutualiser-achats/reports", &body)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    assert_eq!(response["status"], "accepted");
    assert!(app.decision.received_requests().await.unwrap().is_empty());
    let (details, name): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT details, reporter_name FROM reports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((details, name), (None, None));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn invalid_reports_are_refused(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Report, &[("relevant", 0.05)])
        .await;
    crate::test_support::published_idea(&pool, "mutualiser-achats").await;
    let uri = "/api/ideas/mutualiser-achats/reports";
    for (patch, status, code) in [
        (
            json!({ "good_faith": false }),
            StatusCode::BAD_REQUEST,
            "good_faith_required",
        ),
        (
            json!({ "reason": "opinion" }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
        ),
        (
            json!({ "details": "x".repeat(1_001) }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "text_too_long",
        ),
        (
            json!({ "reporter_name": "n".repeat(101) }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "field_too_long",
        ),
        (
            json!({ "reporter_email": "camille" }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_email",
        ),
        (
            json!({ "altcha": "" }),
            StatusCode::BAD_REQUEST,
            "altcha_missing",
        ),
    ] {
        let mut body = report_body(&app);
        for (key, value) in patch.as_object().unwrap() {
            body[key] = value.clone();
        }
        let (actual, response) = app.post(uri, &body).await;
        assert_eq!(actual, status, "{patch}: {response}");
        assert_eq!(response["error"]["code"], code, "{patch}");
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM reports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // An irrelevant report is refused with its statement.
    let (_, response) = app.post(uri, &report_body(&app)).await;
    assert_eq!(response["status"], "rejected");
    assert_eq!(response["reason_code"], "irrelevant");
    assert!(
        response["reason_message"]
            .as_str()
            .unwrap()
            .starts_with("Votre signalement")
    );
}
