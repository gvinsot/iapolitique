//! Integration tests of the calibration routes: screenings, labels, JSONL export, settings.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::ai::decision::questions_hash;
use crate::config::Secret;
use crate::db::settings;
use crate::domain::types::TargetKind;
use crate::test_support::{
    IdeaSpec, SECRET, TestApp, admin_actions, admin_call, admin_get, decision_answers,
    idea_with_status, listed_idea,
};

/// Inserts a screening of `target` whose criteria all pass, except `failing`.
async fn screening(
    app: &TestApp,
    kind: TargetKind,
    target: Uuid,
    outcome: &str,
    failing: Option<&str>,
    grid_version: Option<&str>,
) -> Uuid {
    let prompts = &app.state.prompts;
    let section = prompts.screening.section(kind);
    let mut probabilities = Map::new();
    let mut verdicts = Map::new();
    let mut thresholds = Map::new();
    for criterion in section {
        let fails = failing == Some(criterion.key.as_str());
        probabilities.insert(
            criterion.key.clone(),
            json!(if fails { 0.01 } else { 0.99 }),
        );
        verdicts.insert(
            criterion.key.clone(),
            json!(if fails { "fail" } else { "pass" }),
        );
        thresholds.insert(criterion.key.clone(), json!(criterion.thresholds));
    }
    let answers = decision_answers(prompts, kind, &[])["answers"].clone();
    let final_decision = match outcome {
        "accepted" => Some("accept"),
        "rejected" => Some("reject"),
        _ => None,
    };
    sqlx::query_scalar(
        "INSERT INTO screenings (target_kind, target_id, prompt_version, thresholds, raw_answers,
                                 pass_probabilities, criterion_verdicts, outcome, reason_code,
                                 gray_zone, latency_ms, final_decision)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $8 = 'gray_zone', 1200, $10)
         RETURNING id",
    )
    .bind(kind.as_str())
    .bind(target)
    .bind(grid_version.unwrap_or(&prompts.screening.version))
    .bind(Value::Object(thresholds))
    .bind(answers)
    .bind(Value::Object(probabilities))
    .bind(Value::Object(verdicts))
    .bind(outcome)
    .bind(failing.map(|key| {
        section
            .iter()
            .find(|c| c.key == key)
            .unwrap()
            .reason_code
            .clone()
    }))
    .bind(final_decision)
    .fetch_one(&app.pool)
    .await
    .unwrap()
}

async fn put(app: &TestApp, uri: &str, body: Value) -> (StatusCode, Value) {
    admin_call(app, Method::PUT, uri, Some(&body)).await
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn lists_and_labels_screenings(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let off_topic = app.state.prompts.screening.idea[4].key.clone();
    let accepted_idea = idea_with_status(&pool, "accepted").await;
    let rejected_idea = idea_with_status(&pool, "rejected").await;
    let gray_idea = idea_with_status(&pool, "second_opinion").await;
    let accepted = screening(
        &app,
        TargetKind::Idea,
        accepted_idea,
        "accepted",
        None,
        None,
    )
    .await;
    let rejected = screening(
        &app,
        TargetKind::Idea,
        rejected_idea,
        "rejected",
        Some(&off_topic),
        None,
    )
    .await;
    let gray = screening(&app, TargetKind::Idea, gray_idea, "gray_zone", None, None).await;
    sqlx::query(
        "INSERT INTO screenings (target_kind, target_id, prompt_version, error_code)
         VALUES ('idea', gen_random_uuid(), '1.1.0', 'timeout')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (status, body) = admin_get(&app, "/api/admin/screenings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 4);
    let (_, body) = admin_get(&app, "/api/admin/screenings?outcome=rejected").await;
    assert_eq!(body["total"], 1);
    let item = &body["items"][0];
    assert_eq!(item["id"], rejected.to_string());
    assert!(item["excerpt"].as_str().unwrap().starts_with("Mutualiser"));
    let criteria = item["criteria"].as_array().unwrap();
    let keys: Vec<&str> = criteria
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    let grid: Vec<&str> = app
        .state
        .prompts
        .screening
        .idea
        .iter()
        .map(|c| c.key.as_str())
        .collect();
    assert_eq!(keys, grid);
    let failed = criteria
        .iter()
        .find(|c| c["key"] == off_topic.as_str())
        .unwrap();
    assert_eq!(failed["verdict"], "fail");
    assert_eq!(failed["probability"], 0.01);
    assert!(failed["reject_below"].is_number());
    let reason = item["reason_code"].as_str().unwrap().to_owned();
    let (_, body) = admin_get(&app, &format!("/api/admin/screenings?reason_code={reason}")).await;
    assert_eq!(body["total"], 1);
    for uri in [
        "/api/admin/screenings?reason_code=nope",
        "/api/admin/screenings?kind=vote",
        "/api/admin/screenings?labeled=maybe",
    ] {
        let (status, _) = admin_get(&app, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }

    let label = |id: Uuid| format!("/api/admin/screenings/{id}/label");
    let (status, body) = put(&app, &label(rejected), json!({ "label": "correct" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["label"]["label"], "correct");
    assert_eq!(body["label"]["expected"], "reject");
    let (status, body) = put(
        &app,
        &label(accepted),
        json!({ "label": "incorrect", "note": "Hors sujet en réalité." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["label"]["expected"], "reject");
    assert_eq!(body["label"]["note"], "Hors sujet en réalité.");
    for (id, request) in [
        (
            accepted,
            json!({ "label": "incorrect", "expected": "accept" }),
        ),
        (
            rejected,
            json!({ "label": "correct", "expected": "accept" }),
        ),
        (gray, json!({ "label": "correct" })),
        (gray, json!({ "label": "incorrect" })),
    ] {
        let (status, body) = put(&app, &label(id), request.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{request}");
        assert_eq!(body["error"]["code"], "label_inconsistent");
    }
    let (status, body) = put(
        &app,
        &label(gray),
        json!({ "label": "incorrect", "expected": "accept" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["label"]["label"], "incorrect");
    let (status, _) = put(&app, &label(gray), json!({ "label": "bof" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = put(&app, &label(Uuid::new_v4()), json!({ "label": "correct" })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "screening_not_found");

    let (_, body) = admin_get(&app, "/api/admin/screenings?labeled=true").await;
    assert_eq!(body["total"], 3);
    let (_, body) = admin_get(&app, "/api/admin/screenings?labeled=false").await;
    assert_eq!(body["total"], 0);
    let (status, body) = admin_call(&app, Method::DELETE, &label(gray), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["label"], Value::Null);
    let (_, body) = admin_get(&app, "/api/admin/screenings?labeled=false").await;
    assert_eq!(body["total"], 1);

    let actions: Vec<String> = admin_actions(&pool)
        .await
        .into_iter()
        .map(|a| a.0)
        .collect();
    assert_eq!(
        actions,
        [
            "screening.label",
            "screening.label",
            "screening.label",
            "screening.unlabel"
        ]
    );
}

async fn export(app: &TestApp, query: &str) -> (StatusCode, axum::http::HeaderMap, Vec<Value>) {
    let router = crate::http::admin_router(app.state.clone(), Secret::new(SECRET));
    let request = Request::get(format!("/api/admin/screenings/export.jsonl{query}"))
        .header("authorization", format!("Bearer {SECRET}"))
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let lines = if status == StatusCode::OK {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    } else {
        Vec::new()
    };
    (status, headers, lines)
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn exports_the_calibration_bench_files(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let prompts = std::sync::Arc::clone(&app.state.prompts);
    let off_topic = prompts.screening.idea[4].key.clone();
    let idea = idea_with_status(&pool, "rejected").await;
    let rejected = screening(
        &app,
        TargetKind::Idea,
        idea,
        "rejected",
        Some(&off_topic),
        None,
    )
    .await;
    sqlx::query("UPDATE screenings SET human_label = 'reject', labeled_at = now() WHERE id = $1")
        .bind(rejected)
        .execute(&pool)
        .await
        .unwrap();
    let parent = listed_idea(&pool, &IdeaSpec::default()).await;
    let contribution: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, status, text)
         VALUES ($1, 'amelioration', 'accepted', 'Préciser le calendrier de la mutualisation.')
         RETURNING id",
    )
    .bind(parent)
    .fetch_one(&pool)
    .await
    .unwrap();
    let on_contribution = screening(
        &app,
        TargetKind::Contribution,
        contribution,
        "accepted",
        None,
        None,
    )
    .await;
    sqlx::query("UPDATE screenings SET human_label = 'accept', labeled_at = now() WHERE id = $1")
        .bind(on_contribution)
        .execute(&pool)
        .await
        .unwrap();
    // An older grid version, not labelled, and a failed call (no raw answers).
    let old = screening(
        &app,
        TargetKind::Idea,
        Uuid::new_v4(),
        "accepted",
        None,
        Some("0.9.0"),
    )
    .await;
    sqlx::query(
        "INSERT INTO screenings (target_kind, target_id, prompt_version, error_code)
         VALUES ('idea', gen_random_uuid(), '1.1.0', 'timeout')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (status, headers, lines) = export(&app, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "application/x-ndjson; charset=utf-8"
    );
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"iapolitique-screening-results-")
    );
    assert_eq!(lines.len(), 3);
    let by_id = |id: Uuid| {
        lines
            .iter()
            .find(|l| l["id"] == id.to_string())
            .unwrap()
            .clone()
    };
    let line = by_id(rejected);
    assert_eq!(line["section"], "idea");
    assert_eq!(
        line["questions_hash"],
        questions_hash(&prompts.screening.idea)
    );
    assert_eq!(line["latency_ms"], 1200);
    assert!(line["answers"][off_topic.as_str()].is_object());
    assert_eq!(by_id(old)["questions_hash"], "grid-0.9.0");
    assert_eq!(
        by_id(on_contribution)["questions_hash"],
        questions_hash(&prompts.screening.contribution)
    );

    let (_, _, lines) = export(&app, "?labeled_only=true&kind=idea").await;
    assert_eq!(lines.len(), 1);

    let (status, headers, samples) = export(&app, "?part=samples").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("screening-samples-")
    );
    assert_eq!(samples.len(), 2);
    let sample = samples
        .iter()
        .find(|s| s["id"] == rejected.to_string())
        .unwrap();
    assert_eq!(sample["expected"], "reject");
    assert_eq!(sample["fails"], json!([off_topic]));
    assert_eq!(sample["split"], "tune");
    assert!(sample["text"].as_str().unwrap().starts_with("Mutualiser"));
    let sample = samples
        .iter()
        .find(|s| s["id"] == on_contribution.to_string())
        .unwrap();
    assert_eq!(sample["section"], "contribution");
    assert_eq!(sample["kind"], "amelioration");
    assert_eq!(sample["fails"], json!([]));
    assert!(!sample["parent"].as_str().unwrap().is_empty());

    for query in ["?limit=0", "?limit=20001", "?part=all", "?other=1"] {
        let (status, _, _) = export(&app, query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn updates_thresholds_and_dedup_settings(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let first = app.state.prompts.screening.idea[0].clone();
    let report_key = app.state.prompts.screening.report[0].key.clone();

    let (status, body) = admin_get(&app, "/api/admin/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["grid_version"],
        app.state.prompts.screening.version.as_str()
    );
    let criterion = &body["screening"][0]["criteria"][0];
    assert_eq!(criterion["key"], first.key.as_str());
    assert_eq!(criterion["adjusted"], false);
    assert_eq!(criterion["reject_below"], first.thresholds.reject_below);
    assert_eq!(body["dedup_similarity_threshold"]["value"], 0.95);
    assert_eq!(body["dedup_similarity_threshold"]["adjusted"], false);

    let uri = "/api/admin/settings/screening-thresholds";
    let (status, body) = put(
        &app,
        uri,
        json!({
            "idea": { first.key.clone(): { "reject_below": 0.02 } },
            "report": { report_key.clone(): { "reject_below": 0.1, "accept_above": 0.8 } },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let criterion = &body["screening"][0]["criteria"][0];
    assert_eq!(criterion["reject_below"], 0.02);
    assert_eq!(criterion["accept_above"], first.thresholds.accept_above);
    assert_eq!(
        criterion["default_reject_below"],
        first.thresholds.reject_below
    );
    assert_eq!(criterion["adjusted"], true);
    assert_eq!(body["screening"][2]["criteria"][0]["accept_above"], 0.8);
    let stored = settings::get(&pool, settings::SCREENING_THRESHOLDS)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored["idea"][first.key.as_str()],
        json!({ "reject_below": 0.02 })
    );

    for (request, expected) in [
        (
            json!({ "idea": { "unknown_key": { "reject_below": 0.1 } } }),
            "invalid_thresholds",
        ),
        (
            json!({ "idea": { first.key.clone(): { "reject_below": 1.5 } } }),
            "invalid_thresholds",
        ),
        (
            json!({ "idea": { first.key.clone(): { "reject_below": -0.1 } } }),
            "invalid_thresholds",
        ),
        (
            json!({ "idea": { first.key.clone(): { "reject_below": 0.9, "accept_above": 0.1 } } }),
            "invalid_thresholds",
        ),
        (json!({ "vote": {} }), "invalid_body"),
        (
            json!({ "idea": { first.key.clone(): { "maximum": 0.1 } } }),
            "invalid_body",
        ),
    ] {
        let (status, body) = put(&app, uri, request.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{request}");
        assert_eq!(body["error"]["code"], expected, "{request}");
    }
    // Refused updates changed nothing.
    let (_, body) = admin_get(&app, "/api/admin/settings").await;
    assert_eq!(body["screening"][0]["criteria"][0]["reject_below"], 0.02);

    let (status, body) = put(&app, uri, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["screening"][0]["criteria"][0]["adjusted"], false);
    assert_eq!(
        settings::get(&pool, settings::SCREENING_THRESHOLDS)
            .await
            .unwrap(),
        None
    );

    let uri = "/api/admin/settings/dedup-threshold";
    let (status, body) = put(&app, uri, json!({ "value": 0.9 })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dedup_similarity_threshold"]["value"], 0.9);
    assert_eq!(body["dedup_similarity_threshold"]["adjusted"], true);
    for value in [json!(0.3), json!(1.2), json!("0.9")] {
        let (status, _) = put(&app, uri, json!({ "value": value })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{value}");
    }
    let (status, body) = put(&app, uri, json!({ "value": null })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["dedup_similarity_threshold"]["value"], 0.95);
    assert_eq!(body["dedup_similarity_threshold"]["adjusted"], false);

    let actions: Vec<String> = admin_actions(&pool)
        .await
        .into_iter()
        .map(|a| a.0)
        .collect();
    assert_eq!(
        actions,
        [
            "settings.screening_thresholds",
            "settings.screening_thresholds",
            "settings.dedup_threshold",
            "settings.dedup_threshold"
        ]
    );
}
