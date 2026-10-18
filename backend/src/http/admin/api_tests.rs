//! Integration tests of the admin API: authentication, dashboard and job queue.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};

use sqlx::PgPool;
use tower::ServiceExt;

use crate::config::Secret;
use crate::jobs::{JobKind, NewJob, queue};
use crate::test_support::{SECRET, TestApp, admin_actions, admin_call, admin_get};

/// Admin routes, with a method, for the authentication tests.
pub(crate) const ADMIN_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/admin/dashboard"),
    ("GET", "/api/admin/jobs"),
    ("POST", "/api/admin/jobs/1/retry"),
    ("GET", "/api/admin/ideas"),
    (
        "POST",
        "/api/admin/ideas/00000000-0000-0000-0000-000000000000/hide",
    ),
    (
        "POST",
        "/api/admin/ideas/00000000-0000-0000-0000-000000000000/republish",
    ),
    (
        "POST",
        "/api/admin/ideas/00000000-0000-0000-0000-000000000000/reanalyze",
    ),
    ("GET", "/api/admin/reports"),
    ("GET", "/api/admin/escalations"),
    (
        "POST",
        "/api/admin/escalations/00000000-0000-0000-0000-000000000000/handle",
    ),
    ("GET", "/api/admin/review-requests"),
    (
        "POST",
        "/api/admin/review-requests/00000000-0000-0000-0000-000000000000/resolve",
    ),
    ("GET", "/api/admin/screenings"),
    ("GET", "/api/admin/screenings/export.jsonl"),
    (
        "PUT",
        "/api/admin/screenings/00000000-0000-0000-0000-000000000000/label",
    ),
    (
        "DELETE",
        "/api/admin/screenings/00000000-0000-0000-0000-000000000000/label",
    ),
    ("GET", "/api/admin/settings"),
    ("PUT", "/api/admin/settings/screening-thresholds"),
    ("PUT", "/api/admin/settings/dedup-threshold"),
];

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn every_admin_route_requires_the_token(pool: PgPool) {
    let app = TestApp::new(pool, false, &[]).await;
    let admin = crate::http::admin_router(app.state.clone(), Secret::new(SECRET));
    for (method, uri) in ADMIN_ROUTES {
        let method: Method = method.parse().unwrap();
        for authorization in [None, Some("Bearer wrong-token")] {
            let mut request = Request::builder().method(method.clone()).uri(*uri);
            if let Some(value) = authorization {
                request = request.header("authorization", value);
            }
            let response = admin
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {uri}"
            );
        }
        // The public port does not know them, even with the token.
        let request = Request::builder()
            .method(method.clone())
            .uri(*uri)
            .header("authorization", format!("Bearer {SECRET}"))
            .body(Body::empty())
            .unwrap();
        let response = app.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn dashboard_summarizes_queues_rates_and_costs(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[("MISTRAL_DAILY_BUDGET_EUR", "2")]).await;
    queue::enqueue(&pool, &NewJob::new(JobKind::Purge))
        .await
        .unwrap();
    queue::enqueue(&pool, &NewJob::new(JobKind::GenerateExports))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO jobs (kind, status, finished_at, last_error)
         VALUES ('analyze_idea', 'dead', now(), 'mistral 400')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (outcome, reason, gray) in [
        ("accepted", None, false),
        ("accepted", None, false),
        ("rejected", Some("off_topic"), false),
        ("gray_zone", None, true),
    ] {
        sqlx::query(
            "INSERT INTO screenings (target_kind, target_id, prompt_version, outcome,
                                     reason_code, gray_zone)
             VALUES ('idea', gen_random_uuid(), '1.1.0', $1, $2, $3)",
        )
        .bind(outcome)
        .bind(reason)
        .bind(gray)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO ai_calls (provider, purpose, model, latency_ms, status, cost_micro_eur)
         VALUES ('mistral', 'analysis_research', 'm', 10, 'success', 1500000),
                ('mistral', 'analysis_structuring', 'm', 10, 'success', 250000),
                ('mistral', 'analysis_structuring', 'm', 10, 'success', 250000)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO ideas (original_text, status) VALUES ('Texte.', 'analysis_failed')")
        .execute(&pool)
        .await
        .unwrap();

    let (status, body) = admin_get(&app, "/api/admin/dashboard").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["jobs"]["queued"], 2);
    assert_eq!(body["jobs"]["dead"], 1);
    assert_eq!(body["queues"]["jobs_dead"], 1);
    assert_eq!(body["queues"]["ideas_analysis_failed"], 1);
    let ideas = &body["screening"][0];
    assert_eq!(ideas["kind"], "idea");
    assert_eq!(ideas["accepted"], 2);
    assert_eq!(ideas["rejected"], 1);
    assert_eq!(ideas["acceptance_rate"], 0.5);
    assert_eq!(ideas["gray_zone_rate"], 0.25);
    let costs = &body["ai_costs"];
    assert_eq!(costs["today_eur"], 2.0);
    assert_eq!(costs["month_eur"], 2.0);
    assert_eq!(costs["daily_budget_eur"], 2.0);
    assert_eq!(costs["remaining_today_eur"], 0.0);
    assert_eq!(costs["budget_reached"], true);
    let structuring = costs["by_purpose"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["purpose"] == "analysis_structuring")
        .unwrap();
    assert_eq!(structuring["calls_today"], 2);
    assert_eq!(structuring["today_eur"], 0.5);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn lists_and_retries_dead_jobs(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    for _ in 0..3 {
        queue::enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap();
    }
    let dead: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, status, attempts, finished_at, last_error, dedupe_key)
         VALUES ('dedup_idea', 'dead', 5, now(), 'embedding failed', 'dedup_idea:x')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, body) = admin_get(&app, "/api/admin/jobs?per_page=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 4);
    assert_eq!(body["total_pages"], 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["items"][0]["id"], dead);
    let (_, body) = admin_get(&app, "/api/admin/jobs?status=dead&kind=dedup_idea").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["last_error"], "embedding failed");
    for uri in [
        "/api/admin/jobs?status=zombie",
        "/api/admin/jobs?per_page=100",
        "/api/admin/jobs?kind=x",
        "/api/admin/jobs?other=1",
    ] {
        let (status, _) = admin_get(&app, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }

    let retry = format!("/api/admin/jobs/{dead}/retry");
    let (status, body) = admin_call(&app, Method::POST, &retry, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "queued");
    assert_eq!(body["attempts"], 0);
    let (status, body) = admin_call(&app, Method::POST, &retry, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "job_not_dead");
    let (status, body) = admin_call(&app, Method::POST, "/api/admin/jobs/999999/retry", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "job_not_found");
    let (status, _) = admin_call(&app, Method::POST, "/api/admin/jobs/abc/retry", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A dead job whose key is held by a queued job cannot be retried.
    let other: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, status, finished_at, dedupe_key)
         VALUES ('dedup_idea', 'dead', now(), 'dedup_idea:x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let (status, body) = admin_call(
        &app,
        Method::POST,
        &format!("/api/admin/jobs/{other}/retry"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "job_already_queued");

    assert_eq!(
        admin_actions(&pool).await,
        [("job.retry".to_owned(), Some(dead.to_string()))]
    );
}
