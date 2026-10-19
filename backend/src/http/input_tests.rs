//! Deserialization hardening of every input (AGENTS.md §14.1, §14.8): each JSON body refuses
//! unknown fields (`422 invalid_body`) and each string field refuses an oversized value with a
//! 4xx (never a 500 nor a success); each list refuses unknown and oversized query parameters.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use super::admin_router;
use crate::config::Secret;
use crate::test_support::{CLIENT_IP, SECRET, TestApp, published_idea};

const ID: &str = "00000000-0000-0000-0000-000000000001";

async fn call(
    router: &axum::Router,
    method: Method,
    uri: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", CLIENT_IP)
        .header("authorization", format!("Bearer {SECRET}"));
    let body = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A valid body of each JSON endpoint, with the string fields to oversize and their limit.
struct Endpoint {
    admin: bool,
    method: Method,
    uri: String,
    body: Value,
    /// (JSON pointer of a string field, characters that exceed its limit, error code)
    strings: Vec<(&'static str, usize, &'static str)>,
}

fn endpoints(altcha: &dyn Fn() -> String) -> Vec<Endpoint> {
    let idea_text = "Mutualiser les achats de fournitures entre les communes voisines pour \
                     obtenir de meilleurs prix.";
    let public = |method: Method, uri: &str, body: Value, strings| Endpoint {
        admin: false,
        method,
        uri: uri.to_owned(),
        body,
        strings,
    };
    let admin = |method: Method, uri: String, body: Value, strings| Endpoint {
        admin: true,
        method,
        uri,
        body,
        strings,
    };
    vec![
        public(
            Method::POST,
            "/api/ideas",
            json!({ "text": idea_text, "territory": "Lyon", "accept_licence": true,
                    "accept_publication": true, "altcha": altcha() }),
            vec![
                ("/text", 4_001, "text_too_long"),
                ("/territory", 121, "field_too_long"),
                ("/altcha", 2_049, "altcha_missing"),
            ],
        ),
        public(
            Method::POST,
            "/api/ideas/idee-publiee/contributions",
            json!({ "kind": "amelioration", "text": idea_text, "accept_licence": true,
                    "accept_publication": true, "altcha": altcha() }),
            vec![
                ("/text", 2_001, "text_too_long"),
                ("/altcha", 2_049, "altcha_missing"),
            ],
        ),
        public(
            Method::POST,
            "/api/ideas/idee-publiee/reports",
            json!({ "reason": "erreur_chiffrage", "details": "Chiffre faux.",
                    "reporter_name": "Nom", "reporter_email": "a@example.org",
                    "good_faith": true, "altcha": altcha() }),
            vec![
                ("/details", 1_001, "text_too_long"),
                ("/reporter_name", 101, "field_too_long"),
                ("/reporter_email", 255, "field_too_long"),
                ("/altcha", 2_049, "altcha_missing"),
            ],
        ),
        public(
            Method::POST,
            "/api/track",
            json!({ "token": "x".repeat(43) }),
            vec![("/token", 44, "tracking_not_found")],
        ),
        public(
            Method::POST,
            "/api/track/delete",
            json!({ "token": "x".repeat(43) }),
            vec![("/token", 44, "tracking_not_found")],
        ),
        public(
            Method::POST,
            "/api/track/review-request",
            json!({ "token": "x".repeat(43), "message": "Merci de revoir." }),
            vec![
                ("/token", 44, "tracking_not_found"),
                ("/message", 1_001, "text_too_long"),
            ],
        ),
        admin(
            Method::POST,
            format!("/api/admin/ideas/{ID}/hide"),
            json!({ "reason": "Motif." }),
            vec![("/reason", 1_001, "invalid_field")],
        ),
        admin(
            Method::POST,
            format!("/api/admin/ideas/{ID}/republish"),
            json!({ "reason": "Motif." }),
            vec![("/reason", 1_001, "invalid_field")],
        ),
        admin(
            Method::POST,
            format!("/api/admin/escalations/{ID}/handle"),
            json!({ "outcome": "transmitted", "note": "Note." }),
            vec![("/note", 2_001, "invalid_field")],
        ),
        admin(
            Method::POST,
            format!("/api/admin/review-requests/{ID}/resolve"),
            json!({ "resolution": "confirmed", "note": "Note." }),
            vec![("/note", 2_001, "invalid_field")],
        ),
        admin(
            Method::PUT,
            format!("/api/admin/screenings/{ID}/label"),
            json!({ "label": "correct", "expected": "accept", "note": "Note." }),
            vec![("/note", 1_001, "invalid_field")],
        ),
        admin(
            Method::PUT,
            "/api/admin/settings/screening-thresholds".to_owned(),
            json!({ "idea": { "injection": { "reject_below": 0.1 } } }),
            vec![],
        ),
        admin(
            Method::PUT,
            "/api/admin/settings/dedup-threshold".to_owned(),
            json!({ "value": 0.9 }),
            vec![],
        ),
        admin(
            Method::POST,
            format!("/api/admin/ideas/{ID}/replies"),
            json!({ "author_label": "La mairie", "body": "Réponse.",
                    "requested_on": "2026-01-02" }),
            vec![
                ("/author_label", 201, "invalid_field"),
                ("/body", 8_001, "invalid_field"),
            ],
        ),
        admin(
            Method::PUT,
            format!("/api/admin/replies/{ID}"),
            json!({ "author_label": "La mairie", "body": "Réponse.",
                    "requested_on": "2026-01-02" }),
            vec![
                ("/author_label", 201, "invalid_field"),
                ("/body", 8_001, "invalid_field"),
            ],
        ),
    ]
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn json_bodies_refuse_unknown_and_oversized_fields(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    published_idea(&pool, "idee-publiee").await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let altcha = || app.altcha();
    for endpoint in endpoints(&altcha) {
        let router = if endpoint.admin { &admin } else { &app.router };
        let context = format!("{} {}", endpoint.method, endpoint.uri);

        // Unknown field, at the top level.
        let mut body = endpoint.body.clone();
        body["unexpected"] = json!("x");
        let (status, response) =
            call(router, endpoint.method.clone(), &endpoint.uri, Some(&body)).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{context}: {response}"
        );
        assert_eq!(response["error"]["code"], "invalid_body", "{context}");

        // Oversized string fields.
        for (pointer, chars, code) in &endpoint.strings {
            let mut body = endpoint.body.clone();
            // A fresh ALTCHA solution: the previous one may have been consumed.
            if body.get("altcha").is_some() {
                body["altcha"] = json!(app.altcha());
            }
            *body.pointer_mut(pointer).unwrap() = json!("é".repeat(*chars));
            let (status, response) =
                call(router, endpoint.method.clone(), &endpoint.uri, Some(&body)).await;
            assert!(
                status.is_client_error(),
                "{context} {pointer}: {status} {response}"
            );
            assert_eq!(response["error"]["code"], *code, "{context} {pointer}");
        }
    }

    // Nested objects refuse unknown fields too.
    let (status, response) = call(
        &admin,
        Method::PUT,
        "/api/admin/settings/screening-thresholds",
        Some(&json!({ "idea": { "injection": { "reject_below": 0.1, "other": 1 } } })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["error"]["code"], "invalid_body");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn list_queries_refuse_unknown_and_oversized_parameters(pool: PgPool) {
    let app = TestApp::new(pool, false, &[]).await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let long = "e".repeat(201);
    // (admin port, path, oversized parameter)
    let lists = [
        (false, "/api/ideas", Some("q")),
        (false, "/api/ideas", Some("territory")),
        (false, "/api/ideas", Some("category")),
        (true, "/api/admin/jobs", None),
        (true, "/api/admin/ideas", Some("q")),
        (true, "/api/admin/reports", None),
        (true, "/api/admin/escalations", None),
        (true, "/api/admin/review-requests", None),
        (true, "/api/admin/screenings", Some("reason_code")),
        (true, "/api/admin/screenings/export.jsonl", None),
        (true, "/api/admin/actions", Some("action")),
        (true, "/api/admin/actions", Some("target_id")),
    ];
    for (on_admin, path, oversized) in lists {
        let router = if on_admin { &admin } else { &app.router };
        let (status, response) =
            call(router, Method::GET, &format!("{path}?unexpected=1"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {response}");
        assert_eq!(response["error"]["code"], "invalid_query", "{path}");
        if !path.ends_with(".jsonl") {
            let (status, response) =
                call(router, Method::GET, &format!("{path}?per_page=51"), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {response}");
        }
        if let Some(parameter) = oversized {
            let uri = format!("{path}?{parameter}={long}");
            let (status, response) = call(router, Method::GET, &uri, None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {response}");
            assert_eq!(response["error"]["code"], "invalid_query", "{uri}");
        }
    }
    let (status, _) = call(
        &admin,
        Method::GET,
        "/api/admin/screenings/export.jsonl?limit=20001",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
