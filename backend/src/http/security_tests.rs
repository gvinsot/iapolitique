//! Security tests of the HTTP servers (AGENTS.md §14.8): headers on every response family of
//! the public and admin ports.

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use super::admin_router;
use super::security::assert_security_headers;
use crate::config::Secret;
use crate::db::exports::{ExportFile, insert};
use crate::domain::types::ExportFormat;
use crate::test_support::{CLIENT_IP, SECRET, TestApp};

/// Sends `request` to `router`: status, headers and JSON body (`Null` when not JSON).
async fn send(router: &axum::Router, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let response: Response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

fn request(method: Method, uri: &str, body: Option<String>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", CLIENT_IP);
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn generate_exports(pool: &PgPool) {
    let files: Vec<ExportFile> = ExportFormat::ALL
        .iter()
        .map(|&format| ExportFile {
            format,
            content: format!("content {}", format.as_str()).into_bytes(),
        })
        .collect();
    let mut tx = pool.begin().await.unwrap();
    insert(&mut tx, &files, 0, "1", "1.0").await.unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn public_responses_carry_the_security_headers(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let oversized = json!({ "text": "x".repeat(70_000) }).to_string();
    // (request, expected status, expected Cache-Control)
    let cases = [
        (
            request(Method::GET, "/api/health", None),
            StatusCode::OK,
            "no-store",
        ),
        (
            request(Method::HEAD, "/api/health", None),
            StatusCode::OK,
            "no-store",
        ),
        (
            request(Method::GET, "/api/stats", None),
            StatusCode::OK,
            "public, max-age=60",
        ),
        (
            request(Method::GET, "/api/taxonomy", None),
            StatusCode::OK,
            "public, max-age=300",
        ),
        (
            request(Method::GET, "/api/altcha/challenge", None),
            StatusCode::OK,
            "no-store",
        ),
        (
            request(Method::GET, "/api/nothing", None),
            StatusCode::NOT_FOUND,
            "no-store",
        ),
        (
            request(Method::GET, "/api/ideas/inconnue", None),
            StatusCode::NOT_FOUND,
            "no-store",
        ),
        (
            request(Method::GET, "/api/ideas?sort=nope", None),
            StatusCode::BAD_REQUEST,
            "no-store",
        ),
        (
            request(Method::DELETE, "/api/health", None),
            StatusCode::METHOD_NOT_ALLOWED,
            "no-store",
        ),
        (
            request(Method::POST, "/api/ideas", Some(oversized)),
            StatusCode::PAYLOAD_TOO_LARGE,
            "no-store",
        ),
        (
            request(Method::POST, "/api/ideas", Some("{}".to_owned())),
            StatusCode::UNPROCESSABLE_ENTITY,
            "no-store",
        ),
        (
            request(
                Method::POST,
                "/api/track",
                Some(json!({ "token": "nope" }).to_string()),
            ),
            StatusCode::NOT_FOUND,
            "no-store",
        ),
        (
            request(Method::GET, "/api/exports/ideas.csv", None),
            StatusCode::SERVICE_UNAVAILABLE,
            "no-store",
        ),
    ];
    for (request, status, cache_control) in cases {
        let context = format!("{} {}", request.method(), request.uri());
        let (actual, headers, body) = send(&app.router, request).await;
        assert_eq!(actual, status, "{context}: {body}");
        assert_security_headers(&headers, &context);
        assert_eq!(headers[header::CACHE_CONTROL], cache_control, "{context}");
    }

    generate_exports(&pool).await;
    for path in ["ideas.json", "ideas.csv", "LICENSE", "metadata.json"] {
        let uri = format!("/api/exports/{path}");
        let (status, headers, _) = send(&app.router, request(Method::GET, &uri, None)).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_security_headers(&headers, &uri);
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
        let etag = headers[header::ETAG].to_str().unwrap().to_owned();
        let conditional = Request::get(&uri)
            .header(header::IF_NONE_MATCH, etag)
            .body(Body::empty())
            .unwrap();
        let (status, headers, _) = send(&app.router, conditional).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED, "{uri}");
        assert_security_headers(&headers, &uri);
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn admin_responses_carry_the_security_headers(pool: PgPool) {
    let app = TestApp::new(pool, false, &[]).await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let bearer = format!("Bearer {SECRET}");
    let with_token = |method: Method, uri: &str| {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, &bearer)
            .body(Body::empty())
            .unwrap()
    };
    let cases = [
        (request(Method::GET, "/api/health", None), StatusCode::OK),
        (
            request(Method::GET, "/api/openapi.json", None),
            StatusCode::OK,
        ),
        (
            request(Method::GET, "/api/admin/session", None),
            StatusCode::UNAUTHORIZED,
        ),
        (
            with_token(Method::GET, "/api/admin/session"),
            StatusCode::OK,
        ),
        (
            with_token(Method::GET, "/api/admin/dashboard"),
            StatusCode::OK,
        ),
        (
            with_token(Method::GET, "/api/admin/unknown"),
            StatusCode::NOT_FOUND,
        ),
        (
            with_token(Method::DELETE, "/api/admin/session"),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            with_token(Method::GET, "/api/admin/jobs?per_page=500"),
            StatusCode::BAD_REQUEST,
        ),
        (
            with_token(Method::GET, "/api/admin/screenings/export.jsonl"),
            StatusCode::OK,
        ),
        (
            request(Method::GET, "/api/nothing", None),
            StatusCode::NOT_FOUND,
        ),
    ];
    for (request, status) in cases {
        let context = format!("admin {} {}", request.method(), request.uri());
        let (actual, headers, body) = send(&admin, request).await;
        assert_eq!(actual, status, "{context}: {body}");
        assert_security_headers(&headers, &context);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store", "{context}");
    }
}
