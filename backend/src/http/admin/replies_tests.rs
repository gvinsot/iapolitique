//! Integration tests of the rights of reply and of the admin journal.

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::test_support::{
    IdeaSpec, TestApp, admin_call, admin_get, get_json, idea_with_status, listed_idea,
};

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn publishes_edits_and_removes_rights_of_reply(pool: PgPool) {
    let idea = listed_idea(&pool, &IdeaSpec::default()).await;
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let replies = format!("/api/admin/ideas/{idea}/replies");
    let content = json!({
        "author_label": "La direction des achats de l'hôpital",
        "body": "Ce programme existe déjà depuis 2011.\n\nIl couvre 60 % des achats.",
        "requested_on": "2026-10-15",
    });

    for (body, code) in [
        (
            json!({ "author_label": "  ", "body": "Réponse.", "requested_on": "2026-10-15" }),
            "invalid_field",
        ),
        (
            json!({ "author_label": "x".repeat(201), "body": "Réponse.", "requested_on": "2026-10-15" }),
            "invalid_field",
        ),
        (
            json!({ "author_label": "Service", "body": "", "requested_on": "2026-10-15" }),
            "invalid_field",
        ),
        (
            json!({ "author_label": "Service", "body": "Réponse.", "requested_on": "2999-01-01" }),
            "invalid_field",
        ),
        (
            json!({ "author_label": "Service", "body": "Réponse.", "requested_on": "hier" }),
            "invalid_body",
        ),
        (
            json!({ "author_label": "Service", "body": "Réponse." }),
            "invalid_body",
        ),
    ] {
        let (status, response) = admin_call(&app, Method::POST, &replies, Some(&body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(response["error"]["code"], code, "{body}");
    }

    let (status, created) = admin_call(&app, Method::POST, &replies, Some(&content)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["idea_id"], idea.to_string());
    assert_eq!(created["due_on"], "2026-10-18");
    let id = created["id"].as_str().unwrap().to_owned();

    // Shown on the public page, as plain text.
    let (_, _, page) = get_json(&app.router, "/api/ideas/mutualiser-les-achats", &[]).await;
    assert_eq!(
        page["rights_of_reply"][0]["author_label"],
        "La direction des achats de l'hôpital"
    );
    assert_eq!(
        page["rights_of_reply"][0]["body"],
        "Ce programme existe déjà depuis 2011.\n\nIl couvre 60 % des achats."
    );

    let edited = json!({
        "author_label": "La direction des achats",
        "body": "Réponse corrigée.",
        "requested_on": "2026-10-16",
    });
    let (status, body) = admin_call(
        &app,
        Method::PUT,
        &format!("/api/admin/replies/{id}"),
        Some(&edited),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["body"], "Réponse corrigée.");
    let (status, list) = admin_get(&app, &replies).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["author_label"], "La direction des achats");

    let (status, body) = admin_call(
        &app,
        Method::DELETE,
        &format!("/api/admin/replies/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, body) = admin_call(
        &app,
        Method::DELETE,
        &format!("/api/admin/replies/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "reply_not_found");
    let (status, _) = admin_call(
        &app,
        Method::PUT,
        &format!("/api/admin/replies/{}", Uuid::new_v4()),
        Some(&edited),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, _, page) = get_json(&app.router, "/api/ideas/mutualiser-les-achats", &[]).await;
    assert_eq!(page["rights_of_reply"], json!([]));

    // Only under a published or hidden idea.
    let rejected = idea_with_status(&pool, "rejected").await;
    let (status, body) = admin_call(
        &app,
        Method::POST,
        &format!("/api/admin/ideas/{rejected}/replies"),
        Some(&content),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idea_not_published");
    let unknown = format!("/api/admin/ideas/{}/replies", Uuid::new_v4());
    let (status, _) = admin_call(&app, Method::POST, &unknown, Some(&content)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = admin_get(&app, &unknown).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "idea_not_found");

    // The journal lists every mutation, most recent first, with filters.
    let (status, body) = admin_get(&app, "/api/admin/actions").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let actions: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["reply.delete", "reply.update", "reply.create"]);
    assert_eq!(body["items"][0]["target_id"], id.as_str());
    assert_eq!(body["items"][0]["details"]["idea_id"], idea.to_string());
    let (_, body) = admin_get(&app, "/api/admin/actions?action=reply.update").await;
    assert_eq!(body["total"], 1);
    let (_, body) = admin_get(
        &app,
        &format!("/api/admin/actions?target_id={id}&per_page=1"),
    )
    .await;
    assert_eq!(
        (body["total"].clone(), body["total_pages"].clone()),
        (json!(3), json!(3))
    );
    let long = format!("/api/admin/actions?action={}", "a".repeat(101));
    let (status, _) = admin_get(&app, &long).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
