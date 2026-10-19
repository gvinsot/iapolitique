//! Withdrawal of a published idea by its author (owner decision D2, AGENTS.md §5.2, §7.1, §7.3):
//! every visibility surface — list, search, detail, statistics, exports, contributions, reports,
//! rights of reply, tracking of the author and of the other visitors, admin views — and its
//! interaction with masking, escalation and republication. The purge is tested in `jobs::purge`.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::tracking::{self, TrackingToken};
use crate::domain::types::{AnalysisTrigger, TargetKind};
use crate::jobs::analysis::AnalyzeHandler;
use crate::jobs::exports::ExportsHandler;
use crate::jobs::reports::ReviewReportsHandler;
use crate::jobs::worker::JobHandler;
use crate::jobs::{JobKind, JobOutcome, NewJob, pipeline, queue};
use crate::test_support::{
    IdeaSpec, TestApp, admin_call, admin_get, get_json, idea_with_status, listed_idea,
};

const SLUG: &str = "mutualiser-les-achats";

async fn token_for(pool: &PgPool, kind: TargetKind, id: Uuid) -> String {
    let token = TrackingToken::generate();
    tracking::insert(pool, &token, kind, id).await.unwrap();
    token.into_inner()
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn track(app: &TestApp, token: &str) -> (StatusCode, Value) {
    app.post("/api/track", &json!({ "token": token })).await
}

async fn withdraw(app: &TestApp, token: &str) {
    let (status, body) = app
        .post("/api/track/delete", &json!({ "token": token }))
        .await;
    assert_eq!(
        (status, &body),
        (StatusCode::OK, &json!({ "status": "deleted" }))
    );
}

async fn generate_exports(app: &TestApp) {
    queue::enqueue(&app.pool, &NewJob::new(JobKind::GenerateExports))
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET run_after = now() WHERE kind = 'generate_exports'")
        .execute(&app.pool)
        .await
        .unwrap();
    let job = queue::claim(&app.pool, "test-worker", &[JobKind::GenerateExports])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ExportsHandler.handle(&app.job_context(), &job).await,
        Ok(JobOutcome::Done)
    );
    queue::complete(&app.pool, job.id, "test-worker")
        .await
        .unwrap();
}

/// What the public sees of the idea: listed, found, detail, contributions shown, exported.
async fn public_view(app: &TestApp) -> (Value, Value, StatusCode, Value, Value) {
    let (_, _, list) = get_json(&app.router, "/api/ideas", &[]).await;
    let (_, _, search) = get_json(&app.router, "/api/ideas?q=hopitaux", &[]).await;
    let (detail, _, _) = get_json(&app.router, &format!("/api/ideas/{SLUG}"), &[]).await;
    let (_, _, stats) = get_json(&app.router, "/api/stats", &[]).await;
    generate_exports(app).await;
    let (_, _, exported) = get_json(&app.router, "/api/exports/ideas.json", &[]).await;
    (list, search, detail, stats, exported)
}

/// A published idea with everything that can be attached to it, and the tokens of its author
/// and of the other visitors.
struct Fixture {
    idea: Uuid,
    author: String,
    contributor: String,
    rejected_contributor: String,
    reporter: String,
    duplicate_author: String,
    reply: Uuid,
}

async fn fixture(pool: &PgPool, status: &str) -> Fixture {
    let idea = listed_idea(
        pool,
        &IdeaSpec {
            slug: SLUG,
            status,
            ..IdeaSpec::default()
        },
    )
    .await;
    let analysis: Uuid = sqlx::query_scalar("SELECT current_analysis_id FROM ideas WHERE id = $1")
        .bind(idea)
        .fetch_one(pool)
        .await
        .unwrap();
    let integrated: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text, status, integrated_in_analysis_id,
                                    neutral_summary, integrated_at)
         VALUES ($1, 'objection', 'Texte', 'integrated', $2, 'Résumé neutre', now())
         RETURNING id",
    )
    .bind(idea)
    .bind(analysis)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE ideas SET contribution_count = 1 WHERE id = $1")
        .bind(idea)
        .execute(pool)
        .await
        .unwrap();
    let rejected: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text, status, rejection_reason_code,
                                    rejected_at)
         VALUES ($1, 'amelioration', 'Autre texte', 'rejected', 'off_topic', now())
         RETURNING id",
    )
    .bind(idea)
    .fetch_one(pool)
    .await
    .unwrap();
    let report: Uuid = sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, details, reporter_email, status, good_faith)
         VALUES ($1, 'autre', 'Précision', 'r@exemple.fr', 'accepted', true) RETURNING id",
    )
    .bind(idea)
    .fetch_one(pool)
    .await
    .unwrap();
    let duplicate: Uuid = sqlx::query_scalar(
        "INSERT INTO ideas (original_text, status, parent_idea_id)
         VALUES ('Même idée, autrement dite.', 'merged', $1) RETURNING id",
    )
    .bind(idea)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, source_idea_id, status)
         VALUES ($1, 'doublon', $2, 'accepted')",
    )
    .bind(idea)
    .bind(duplicate)
    .execute(pool)
    .await
    .unwrap();
    let reply: Uuid = sqlx::query_scalar(
        "INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on)
         VALUES ($1, 'Mairie de X', 'Réponse.', current_date) RETURNING id",
    )
    .bind(idea)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO jobs (kind, payload, dedupe_key) VALUES
             ('analyze_idea', jsonb_build_object('idea_id', $1::text, 'trigger', 'contribution'), 'a'),
             ('review_reports', jsonb_build_object('idea_id', $1::text), 'b'),
             ('screen_report', jsonb_build_object('target_kind', 'report', 'target_id', $2::text), 'c'),
             ('screen_contribution', jsonb_build_object('target_kind', 'contribution', 'target_id', $3::text), 'd')",
    )
    .bind(idea)
    .bind(report)
    .bind(rejected)
    .execute(pool)
    .await
    .unwrap();
    Fixture {
        idea,
        author: token_for(pool, TargetKind::Idea, idea).await,
        contributor: token_for(pool, TargetKind::Contribution, integrated).await,
        rejected_contributor: token_for(pool, TargetKind::Contribution, rejected).await,
        reporter: token_for(pool, TargetKind::Report, report).await,
        duplicate_author: token_for(pool, TargetKind::Idea, duplicate).await,
        reply,
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_withdrawn_idea_leaves_every_public_surface(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    app.decision_answers(TargetKind::Contribution, &[]).await;
    let f = fixture(&pool, "published").await;

    // Before: listed, found, shown with its reply and contribution, counted, exported.
    let (list, search, detail, stats, exported) = public_view(&app).await;
    assert_eq!(list["total"], 1);
    assert_eq!(search["total"], 1);
    assert_eq!(detail, StatusCode::OK);
    assert_eq!(stats["ideas_published"], 1);
    assert_eq!(stats["ideas_received"], 2);
    assert_eq!(stats["duplicates_merged"], 1);
    assert_eq!(stats["contributions_integrated"], 1);
    assert_eq!(exported["ideas"].as_array().unwrap().len(), 1);
    let (_, body) = track(&app, &f.duplicate_author).await;
    assert_eq!(body["idea"]["slug"], SLUG);
    assert_eq!(body["idea_withdrawn"], false);

    withdraw(&app, &f.author).await;

    // Kept for the admin: nothing is erased.
    for (sql, left) in [
        ("SELECT count(*) FROM ideas", 2),
        ("SELECT count(*) FROM analyses", 1),
        ("SELECT count(*) FROM contributions", 3),
        ("SELECT count(*) FROM reports", 1),
        ("SELECT count(*) FROM rights_of_reply", 1),
        ("SELECT count(*) FROM tracking_tokens", 5),
    ] {
        assert_eq!(count(&pool, sql).await, left, "{sql}");
    }
    let (status, from, at): (String, Option<String>, bool) = sqlx::query_as(
        "SELECT status, withdrawn_from, withdrawn_at IS NOT NULL FROM ideas WHERE id = $1",
    )
    .bind(f.idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (status.as_str(), from.as_deref(), at),
        ("withdrawn", Some("published"), true)
    );
    let event: (Option<String>, String, String, Option<String>) = sqlx::query_as(
        "SELECT from_status, to_status, actor, reason_code FROM idea_events
         WHERE idea_id = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(f.idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event,
        (
            Some("published".to_owned()),
            "withdrawn".to_owned(),
            "author".to_owned(),
            Some("author_withdrawal".to_owned())
        )
    );
    // No more work about it; the exports are regenerated.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind <> 'generate_exports' AND status = 'queued'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind = 'generate_exports' AND status = 'queued'"
        )
        .await,
        1
    );
    // Due now with the top priority: a withdrawn idea never stays downloadable (§7.4).
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind = 'generate_exports' AND status = 'queued'
               AND run_after <= now() AND priority = 60"
        )
        .await,
        1
    );

    // Gone from the public site, the API and the exports, with what was attached to it.
    let (list, search, detail, stats, exported) = public_view(&app).await;
    assert_eq!(list["total"], 0);
    assert_eq!(search["total"], 0);
    assert_eq!(detail, StatusCode::NOT_FOUND);
    assert_eq!(stats["ideas_published"], 0);
    assert_eq!(stats["ideas_received"], 0);
    assert_eq!(stats["duplicates_merged"], 0);
    assert_eq!(stats["contributions_integrated"], 0);
    assert_eq!(stats["published_by_kind"], json!([]));
    assert_eq!(exported["ideas"], json!([]));
    let (_, _, body) = get_json(&app.router, &format!("/api/ideas/{SLUG}"), &[]).await;
    assert_eq!(body["error"]["code"], "idea_not_found");
    for uri in [
        format!("/api/ideas/{SLUG}/contributions"),
        format!("/api/ideas/{SLUG}/reports"),
    ] {
        let body = if uri.ends_with("contributions") {
            json!({
                "kind": "objection",
                "text": "Une objection assez longue pour être examinée.",
                "accept_licence": true,
                "accept_publication": true,
                "altcha": app.altcha(),
            })
        } else {
            json!({ "reason": "autre", "good_faith": true, "altcha": app.altcha() })
        };
        let (status, body) = app.post(&uri, &body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} {body}");
        assert_eq!(body["error"]["code"], "idea_not_found");
    }

    // Tracking: the author sees the deletion done, the others « idée retirée par son auteur ».
    let (status, body) = track(&app, &f.author).await;
    assert_eq!(
        (status, &body["status"]),
        (StatusCode::OK, &json!("deleted"))
    );
    for (token, status) in [
        (&f.contributor, "integrated"),
        (&f.rejected_contributor, "rejected"),
        (&f.reporter, "accepted"),
        (&f.duplicate_author, "merged"),
    ] {
        let (code, body) = track(&app, token).await;
        assert_eq!(code, StatusCode::OK, "{body}");
        assert_eq!(body["status"], status);
        assert_eq!(body["idea"], Value::Null);
        assert_eq!(body["idea_withdrawn"], true);
        assert!(!body.to_string().contains(&f.idea.to_string()));
    }
    // Nothing left to review about a withdrawn idea.
    let (status, body) = app
        .post(
            "/api/track/review-request",
            &json!({ "token": f.rejected_contributor }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "review_not_available");
    // Idempotent; the others may still delete their own texts.
    withdraw(&app, &f.author).await;
    withdraw(&app, &f.reporter).await;
    assert_eq!(count(&pool, "SELECT count(*) FROM reports").await, 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ideas WHERE status = 'withdrawn'"
        )
        .await,
        1
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn the_admin_sees_a_withdrawn_idea_but_never_republishes_it(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let f = fixture(&pool, "published").await;
    withdraw(&app, &f.author).await;

    let (status, page) = admin_get(&app, "/api/admin/ideas?status=withdrawn").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    let item = &page["items"][0];
    assert_eq!(item["status"], "withdrawn");
    assert_eq!(item["slug"], SLUG);
    assert!(item["withdrawn_at"].is_string());
    assert!(item["excerpt"].is_string());
    let (_, all) = admin_get(&app, "/api/admin/ideas").await;
    let other = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["status"] == "merged")
        .unwrap();
    assert_eq!(other["withdrawn_at"], Value::Null);

    let (_, reports) = admin_get(&app, "/api/admin/reports").await;
    assert_eq!(reports["items"][0]["idea"]["status"], "withdrawn");
    let (status, replies) = admin_get(&app, &format!("/api/admin/ideas/{}/replies", f.idea)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replies.as_array().map_or(0, Vec::len), 1, "{replies}");

    let reason = json!({ "reason": "Demande de republication." });
    for (uri, body, code) in [
        ("republish", Some(&reason), "idea_withdrawn"),
        ("hide", Some(&reason), "idea_not_published"),
        ("reanalyze", None, "reanalysis_not_available"),
    ] {
        let (status, answer) = admin_call(
            &app,
            Method::POST,
            &format!("/api/admin/ideas/{}/{uri}", f.idea),
            body,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{uri} {answer}");
        assert_eq!(answer["error"]["code"], code, "{uri}");
    }
    let (status, answer) = admin_call(
        &app,
        Method::POST,
        &format!("/api/admin/ideas/{}/replies", f.idea),
        Some(&json!({
            "author_label": "Mairie de Y",
            "body": "Réponse.",
            "requested_on": "2026-10-01",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    // The admin may still remove a reply attached to it.
    let (status, _) = admin_call(
        &app,
        Method::DELETE,
        &format!("/api/admin/replies/{}", f.reply),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ideas WHERE status = 'withdrawn'"
        )
        .await,
        1
    );

    // The jobs that would have touched it do nothing.
    let ctx = app.job_context();
    for (job, handler) in [
        (
            pipeline::analyze_job(f.idea, AnalysisTrigger::Admin),
            &AnalyzeHandler as &dyn JobHandler,
        ),
        (
            pipeline::review_reports_job(f.idea, chrono::Utc::now()),
            &ReviewReportsHandler as &dyn JobHandler,
        ),
    ] {
        queue::enqueue(&pool, &job).await.unwrap();
        sqlx::query("UPDATE jobs SET run_after = now() - interval '1 hour'")
            .execute(&pool)
            .await
            .unwrap();
        let claimed = queue::claim(&pool, "test-worker", &[handler.kind()])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(handler.handle(&ctx, &claimed).await, Ok(JobOutcome::Done));
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn a_hidden_escalated_idea_is_withdrawn_and_stays_in_the_escalation_queue(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let f = fixture(&pool, "hidden").await;
    sqlx::query("UPDATE ideas SET hidden_reason = 'escalation' WHERE id = $1")
        .bind(f.idea)
        .execute(&pool)
        .await
        .unwrap();
    let escalation: Uuid =
        sqlx::query_scalar("INSERT INTO escalations (idea_id) VALUES ($1) RETURNING id")
            .bind(f.idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO review_requests (target_kind, target_id) VALUES ('idea', $1)")
        .bind(f.idea)
        .execute(&pool)
        .await
        .unwrap();

    withdraw(&app, &f.author).await;
    let (status, from): (String, Option<String>) =
        sqlx::query_as("SELECT status, withdrawn_from FROM ideas WHERE id = $1")
            .bind(f.idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), from.as_deref()),
        ("withdrawn", Some("hidden"))
    );
    // The author's review request has no object any more; the escalation stays.
    assert_eq!(
        count(&pool, "SELECT count(*) FROM review_requests").await,
        0
    );
    let (_, queue) = admin_get(&app, "/api/admin/escalations?status=pending").await;
    assert_eq!(queue["total"], 1);
    assert_eq!(queue["items"][0]["id"], json!(escalation));
    assert_eq!(queue["items"][0]["idea"]["status"], "withdrawn");
    assert_eq!(count(&pool, "SELECT count(*) FROM admin_actions").await, 0);
    // The admin can still close it.
    let (status, _) = admin_call(
        &app,
        Method::POST,
        &format!("/api/admin/escalations/{escalation}/handle"),
        Some(&json!({ "outcome": "transmitted" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn unpublished_ideas_are_still_erased_at_once(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    for status in ["accepted", "queued_analysis", "analysis_failed", "rejected"] {
        let idea = idea_with_status(&pool, status).await;
        let author = token_for(&pool, TargetKind::Idea, idea).await;
        withdraw(&app, &author).await;
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM ideas WHERE id = $1")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "{status}");
        assert_eq!(track(&app, &author).await.1["status"], "deleted");
    }
}
