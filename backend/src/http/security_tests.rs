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

/// Runs `future` with a subscriber capturing every log line (current thread): the output.
pub(crate) async fn capture_logs<F: std::future::Future>(future: F) -> (F::Output, String) {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Buffer {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let buffer = Buffer::default();
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let output = future.await;
    drop(guard);
    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    (output, logs)
}

const PUBLIC_HOST: &str = "qa.iapolitique.fr";
const ADMIN_HOST: &str = "qa.admin.iapolitique.fr";

async fn app_with_hosts(pool: PgPool) -> TestApp {
    TestApp::new(
        pool,
        false,
        &[("TRAEFIK_HOST", PUBLIC_HOST), ("ADMIN_HOST", ADMIN_HOST)],
    )
    .await
}

fn with_headers(
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", CLIENT_IP);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
        .unwrap()
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn cors_preflights_are_refused_and_never_answered(pool: PgPool) {
    let app = app_with_hosts(pool).await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let preflight = [
        ("origin", "https://evil.example"),
        ("access-control-request-method", "POST"),
        ("access-control-request-headers", "content-type"),
    ];
    for (router, uri) in [
        (&app.router, "/api/ideas"),
        (&app.router, "/api/track"),
        (&app.router, "/api/nothing"),
        (&admin, "/api/admin/session"),
    ] {
        let (status, headers, body) =
            send(router, with_headers(Method::OPTIONS, uri, &preflight, None)).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{uri}");
        assert_eq!(body["error"]["code"], "method_not_allowed", "{uri}");
        assert_security_headers(&headers, uri);
    }
    // A simple cross-origin GET is answered, without any Access-Control-* header.
    let (status, headers, _) = send(
        &app.router,
        with_headers(
            Method::GET,
            "/api/stats",
            &[("origin", "https://evil.example")],
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_security_headers(&headers, "cross-origin GET");
}

/// A write request and its expected answer (status, error code).
type WriteCase<'a> = (
    &'a axum::Router,
    Method,
    &'a str,
    Vec<(&'a str, &'a str)>,
    Option<&'a str>,
    StatusCode,
    &'a str,
);

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn cross_site_writes_are_refused(pool: PgPool) {
    let app = app_with_hosts(pool).await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let json = ("content-type", "application/json");
    let bearer = format!("Bearer {SECRET}");
    let token = ("authorization", bearer.as_str());
    let public_origin = format!("https://{PUBLIC_HOST}");
    let admin_origin = format!("https://{ADMIN_HOST}");
    let job = "/api/admin/jobs/999999/retry".to_owned();

    // (router, method, uri, headers, body, expected status, expected code)
    let cases: Vec<WriteCase<'_>> = vec![
        // Wrong Content-Type: a cross-site form.
        (
            &app.router,
            Method::POST,
            "/api/ideas",
            vec![("content-type", "application/x-www-form-urlencoded")],
            Some("text=x"),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
        (
            &app.router,
            Method::POST,
            "/api/track",
            vec![("content-type", "text/plain")],
            Some(r#"{"token":"x"}"#),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
        (
            &app.router,
            Method::POST,
            "/api/track",
            vec![],
            Some(r#"{"token":"x"}"#),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
        // Foreign origin.
        (
            &app.router,
            Method::POST,
            "/api/ideas",
            vec![json, ("origin", "https://evil.example")],
            Some("{}"),
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
        ),
        (
            &app.router,
            Method::POST,
            "/api/track/delete",
            vec![json, ("origin", "null")],
            Some(r#"{"token":"x"}"#),
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
        ),
        (
            &app.router,
            Method::POST,
            "/api/track",
            vec![json, ("sec-fetch-site", "cross-site")],
            Some(r#"{"token":"x"}"#),
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
        ),
        (
            &admin,
            Method::POST,
            &job,
            vec![token, ("origin", public_origin.as_str())],
            None,
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
        ),
        (
            &admin,
            Method::PUT,
            "/api/admin/settings/dedup-threshold",
            vec![token, json, ("origin", "https://evil.example")],
            Some(r#"{"value":0.9}"#),
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
        ),
        // Same origin (or no browser at all): the handler answers.
        (
            &app.router,
            Method::POST,
            "/api/ideas",
            vec![
                json,
                ("origin", public_origin.as_str()),
                ("sec-fetch-site", "same-origin"),
            ],
            Some("{}"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
        ),
        (
            &app.router,
            Method::POST,
            "/api/ideas",
            vec![json, ("origin", admin_origin.as_str())],
            Some("{}"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
        ),
        (
            &app.router,
            Method::POST,
            "/api/ideas",
            vec![json],
            Some("{}"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
        ),
        (
            &admin,
            Method::POST,
            &job,
            vec![token, ("origin", admin_origin.as_str())],
            None,
            StatusCode::NOT_FOUND,
            "job_not_found",
        ),
        (
            &admin,
            Method::POST,
            &job,
            vec![token],
            None,
            StatusCode::NOT_FOUND,
            "job_not_found",
        ),
    ];
    for (router, method, uri, headers, body, status, code) in cases {
        let context = format!("{method} {uri} {headers:?}");
        let (actual, response_headers, response) =
            send(router, with_headers(method, uri, &headers, body)).await;
        assert_eq!(actual, status, "{context}: {response}");
        assert_eq!(response["error"]["code"], code, "{context}");
        assert_security_headers(&response_headers, &context);
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn csrf_refusals_are_logged_without_ip_nor_content(pool: PgPool) {
    let app = app_with_hosts(pool).await;
    let request = with_headers(
        Method::POST,
        "/api/ideas",
        &[
            ("content-type", "application/json"),
            ("origin", "https://evil.example"),
        ],
        Some(r#"{"text":"texte-secret-du-visiteur"}"#),
    );
    let ((status, _, _), logs) = capture_logs(send(&app.router, request)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(logs.contains("security event"), "{logs}");
    assert!(logs.contains("csrf_refused"), "{logs}");
    assert!(logs.contains("origin_not_allowed"), "{logs}");
    for secret in [CLIENT_IP, "evil.example", "texte-secret-du-visiteur"] {
        assert!(!logs.contains(secret), "{secret} logged: {logs}");
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn admin_authentication_locks_after_repeated_failures(pool: PgPool) {
    use crate::http::admin::LOCKOUT_MAX_FAILURES;

    let app = TestApp::new(pool, false, &[]).await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let session = |authorization: Option<&str>| {
        let headers: Vec<(&str, &str)> = authorization
            .map(|value| vec![("authorization", value)])
            .unwrap_or_default();
        with_headers(Method::GET, "/api/admin/session", &headers, None)
    };
    let bearer = format!("Bearer {SECRET}");
    let wrong = "Bearer wrong-token-0123456789abcdef0123456789";

    // Missing tokens do not count.
    for _ in 0..LOCKOUT_MAX_FAILURES + 2 {
        let (status, _, _) = send(&admin, session(None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, _) = send(&admin, session(Some(&bearer))).await;
    assert_eq!(status, StatusCode::OK);

    let ((), logs) = capture_logs(async {
        for _ in 0..LOCKOUT_MAX_FAILURES {
            let (status, _, body) = send(&admin, session(Some(wrong))).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        }
    })
    .await;
    assert!(logs.contains("admin_auth_failure"), "{logs}");
    assert!(logs.contains("admin_auth_lockout"), "{logs}");
    assert!(!logs.contains("wrong-token"), "token logged: {logs}");
    assert!(!logs.contains(CLIENT_IP), "IP logged: {logs}");

    // Locked: even the right token is refused, on every admin route.
    for uri in ["/api/admin/session", "/api/admin/dashboard"] {
        let request = with_headers(Method::GET, uri, &[("authorization", &bearer)], None);
        let (status, headers, body) = send(&admin, request).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{uri}");
        assert_eq!(body["error"]["code"], "admin_locked");
        let retry: u64 = headers[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((1..=300).contains(&retry), "{retry}");
        assert_security_headers(&headers, uri);
    }
    // Probes stay open.
    let (status, _, _) = send(&admin, request(Method::GET, "/api/health", None)).await;
    assert_eq!(status, StatusCode::OK);
}

/// Every admin route of the full spec, with its methods, path parameters filled in.
fn admin_operations() -> Vec<(Method, String)> {
    let spec = serde_json::to_value(super::openapi::full_spec()).unwrap();
    let mut operations = Vec::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        if !path.starts_with("/api/admin") {
            continue;
        }
        // Job ids are integers, every other id a UUID.
        let id = if path.contains("/jobs/") {
            "1"
        } else {
            "00000000-0000-0000-0000-000000000001"
        };
        let uri = path
            .replace("{id}", id)
            .replace("{slug}", "idee")
            .replace(['{', '}'], "");
        for method in item.as_object().unwrap().keys() {
            let method = Method::from_bytes(method.to_ascii_uppercase().as_bytes()).unwrap();
            operations.push((method, uri.clone()));
        }
    }
    operations
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_public_port_knows_no_admin_route(pool: PgPool) {
    let app = TestApp::new(pool, false, &[]).await;
    let admin = admin_router(app.state.clone(), Secret::new(SECRET));
    let operations = admin_operations();
    assert!(operations.len() >= 25, "{operations:?}");
    let bearer = format!("Bearer {SECRET}");
    for (method, uri) in operations {
        let body = (method != Method::GET).then_some("{}");
        let headers = [
            ("authorization", bearer.as_str()),
            ("content-type", "application/json"),
        ];
        let context = format!("{method} {uri}");
        let (status, response_headers, response) = send(
            &app.router,
            with_headers(method.clone(), &uri, &headers, body),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{context}: {response}");
        assert_eq!(response["error"]["code"], "not_found", "{context}");
        assert_security_headers(&response_headers, &context);
        // The same operation exists on the admin port (anything but a 404 of the router).
        let (status, _, response) = send(&admin, with_headers(method, &uri, &headers, body)).await;
        assert!(
            status != StatusCode::NOT_FOUND || response["error"]["code"] != "not_found",
            "{context} unknown on the admin port"
        );
    }
}
