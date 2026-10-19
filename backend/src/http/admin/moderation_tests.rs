//! Integration tests of the admin moderation routes: ideas, reports, escalations and human
//! review requests.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::jobs::priority;
use crate::test_support::{
    IdeaSpec, TestApp, admin_actions, admin_call, admin_get, idea_with_status, listed_idea,
    published_idea,
};

async fn post(app: &TestApp, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    admin_call(app, Method::POST, uri, body.as_ref()).await
}

async fn status_of(pool: &PgPool, table: &str, id: Uuid) -> String {
    let sql = match table {
        "ideas" => "SELECT status FROM ideas WHERE id = $1",
        "contributions" => "SELECT status FROM contributions WHERE id = $1",
        "reports" => "SELECT status FROM reports WHERE id = $1",
        other => panic!("{other}"),
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn queued_jobs(pool: &PgPool) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT kind, payload FROM jobs WHERE status = 'queued' ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Whether the queued regeneration of the exports is due now, and its priority.
async fn exports_job_due(pool: &PgPool) -> (bool, i16) {
    sqlx::query_as(
        "SELECT run_after <= now(), priority FROM jobs
         WHERE kind = 'generate_exports' AND status = 'queued'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A refused idea with its screening.
async fn rejected_idea(pool: &PgPool) -> Uuid {
    let id = idea_with_status(pool, "rejected").await;
    sqlx::query(
        "UPDATE ideas SET rejection_reason_code = 'off_topic', rejected_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO screenings (target_kind, target_id, prompt_version, outcome, reason_code,
                                 final_decision)
         VALUES ('idea', $1, '1.1.0', 'rejected', 'off_topic', 'reject')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn review_request(pool: &PgPool, kind: &str, target: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO review_requests (target_kind, target_id, message)
         VALUES ($1, $2, 'Mon idée concerne bien les finances publiques.') RETURNING id",
    )
    .bind(kind)
    .bind(target)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn lists_and_searches_ideas(pool: PgPool) {
    let published = listed_idea(&pool, &IdeaSpec::default()).await;
    let rejected = rejected_idea(&pool).await;
    let app = TestApp::new(pool.clone(), false, &[]).await;

    let (status, body) = admin_get(&app, "/api/admin/ideas").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 2);
    let (_, body) = admin_get(&app, "/api/admin/ideas?status=rejected").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], rejected.to_string());
    assert_eq!(body["items"][0]["rejection_reason_code"], "off_topic");
    assert!(
        body["items"][0]["excerpt"]
            .as_str()
            .unwrap()
            .starts_with("Mutualiser")
    );
    // Search: accents and case ignored, by identifier, LIKE wildcards literal.
    let (_, body) = admin_get(&app, "/api/admin/ideas?q=HOPITAUX").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], published.to_string());
    let (_, body) = admin_get(&app, &format!("/api/admin/ideas?q={rejected}")).await;
    assert_eq!(body["total"], 1);
    let (_, body) = admin_get(&app, "/api/admin/ideas?q=%25").await;
    assert_eq!(body["total"], 0);
    // INJ-5: full-width wildcards, which unaccent() turns into `%` and `_`, are literal too.
    for wildcard in ["%EF%BC%85", "%EF%BC%BF%EF%BC%BF%EF%BC%BF"] {
        let (_, body) = admin_get(&app, &format!("/api/admin/ideas?q={wildcard}")).await;
        assert_eq!(body["total"], 0, "{wildcard}");
    }
    let (status, _) = admin_get(&app, "/api/admin/ideas?status=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn hides_republishes_and_reanalyzes_ideas(pool: PgPool) {
    let idea = listed_idea(&pool, &IdeaSpec::default()).await;
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let hide = format!("/api/admin/ideas/{idea}/hide");
    let republish = format!("/api/admin/ideas/{idea}/republish");

    for (body, code) in [
        (json!({ "reason": "   " }), "invalid_field"),
        (json!({ "reason": "x".repeat(1001) }), "invalid_field"),
        (json!({}), "invalid_body"),
        (json!({ "reason": "ok", "extra": 1 }), "invalid_body"),
    ] {
        let (status, response) = post(&app, &hide, Some(body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert_eq!(response["error"]["code"], code);
    }
    let (status, _) = post(&app, &hide, None).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let reason = json!({ "reason": "Contenu signalé par courrier." });
    let (status, body) = post(&app, &hide, Some(reason.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "hidden");
    assert_eq!(body["hidden_reason"], "admin");
    assert_eq!(queued_jobs(&pool).await[0].0, "generate_exports");
    // Out of the exports right away, not two minutes later (§7.4).
    assert_eq!(
        exports_job_due(&pool).await,
        (true, priority::URGENT_EXPORTS)
    );
    let (status, body) = post(&app, &hide, Some(reason.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idea_not_published");
    let (status, body) = post(
        &app,
        &format!("/api/admin/ideas/{}/hide", Uuid::new_v4()),
        Some(reason.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "idea_not_found");
    let (public, _, _) =
        crate::test_support::get_json(&app.router, "/api/ideas/mutualiser-les-achats", &[]).await;
    assert_eq!(public, StatusCode::NOT_FOUND);

    sqlx::query("DELETE FROM jobs WHERE kind = 'generate_exports'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = post(&app, &republish, Some(reason.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "published");
    // A publication keeps the two-minute grouping.
    assert_eq!(exports_job_due(&pool).await, (false, priority::MAINTENANCE));
    assert_eq!(body["hidden_reason"], Value::Null);
    assert_eq!(body["hidden_at"], Value::Null);
    let (status, body) = post(&app, &republish, Some(reason)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idea_not_hidden");

    let reanalyze = format!("/api/admin/ideas/{idea}/reanalyze");
    let (status, body) = post(&app, &reanalyze, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body, json!({ "queued": true, "status": "published" }));
    let (_, body) = post(&app, &reanalyze, None).await;
    assert_eq!(body["queued"], false);
    let analyses: Vec<(String, Value)> = queued_jobs(&pool)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "analyze_idea")
        .collect();
    assert_eq!(analyses.len(), 1);
    assert_eq!(analyses[0].1["trigger"], "admin");

    let failed = idea_with_status(&pool, "analysis_failed").await;
    let (status, body) = post(&app, &format!("/api/admin/ideas/{failed}/reanalyze"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["status"], "queued_analysis");
    assert_eq!(status_of(&pool, "ideas", failed).await, "queued_analysis");
    let submitted = idea_with_status(&pool, "submitted").await;
    let (status, body) = post(
        &app,
        &format!("/api/admin/ideas/{submitted}/reanalyze"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "reanalysis_not_available");

    let actions: Vec<String> = admin_actions(&pool)
        .await
        .into_iter()
        .map(|a| a.0)
        .collect();
    assert_eq!(
        actions,
        [
            "idea.hide",
            "idea.republish",
            "idea.reanalyze",
            "idea.reanalyze",
            "idea.reanalyze"
        ]
    );
    let events: Vec<(String, String)> =
        sqlx::query_as("SELECT to_status, actor FROM idea_events WHERE idea_id = $1 ORDER BY id")
            .bind(idea)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        events,
        [
            ("hidden".to_owned(), "admin".to_owned()),
            ("published".to_owned(), "admin".to_owned())
        ]
    );
}

/// A dead analysis left its idea in `analysis_failed`, which the job ignores: retrying the job
/// moves the idea back to `queued_analysis`, like « Relancer l'analyse ». An idea in another
/// status keeps it (the job knows what to do with it).
#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn retrying_a_dead_analysis_queues_its_idea_again(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let dead_analysis = |idea: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO jobs (kind, status, attempts, finished_at, last_error, payload,
                                   dedupe_key)
                 VALUES ('analyze_idea', 'dead', 3, now(), 'lock expired', $1, $2) RETURNING id",
            )
            .bind(json!({ "idea_id": idea, "trigger": "initial" }))
            .bind(format!("analyze_idea:{idea}"))
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };

    let failed = idea_with_status(&pool, "analysis_failed").await;
    let job = dead_analysis(failed).await;
    let (status, body) = post(&app, &format!("/api/admin/jobs/{job}/retry"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "queued");
    assert_eq!(status_of(&pool, "ideas", failed).await, "queued_analysis");
    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT to_status, actor, reason_code FROM idea_events WHERE idea_id = $1 ORDER BY id",
    )
    .bind(failed)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        events.last().unwrap(),
        &(
            "queued_analysis".to_owned(),
            "admin".to_owned(),
            Some("admin_reanalysis".to_owned())
        )
    );

    let published = published_idea(&pool, "retry-analysis").await;
    let job_published = dead_analysis(published).await;
    let (status, _) = post(
        &app,
        &format!("/api/admin/jobs/{job_published}/retry"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(status_of(&pool, "ideas", published).await, "published");

    let details: Vec<Value> = sqlx::query_scalar(
        "SELECT details FROM admin_actions WHERE action = 'job.retry' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        details,
        [
            json!({ "kind": "analyze_idea", "idea_id": failed, "idea_requeued": true }),
            json!({ "kind": "analyze_idea", "idea_id": published, "idea_requeued": false }),
        ]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn lists_reports_and_handles_escalations(pool: PgPool) {
    let idea = listed_idea(&pool, &IdeaSpec::default()).await;
    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO report_reviews (idea_id, outcome, explanation, model, prompt_version,
                                     escalate)
         VALUES ($1, 'hide', 'Menace explicite.', 'm', '1.0.0', true) RETURNING id",
    )
    .bind(idea)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO reports (idea_id, reason, details, status, review_id, reporter_name,
                              reporter_email, good_faith)
         VALUES ($1, 'contenu_problematique', 'Le texte menace une personne.', 'reviewed', $2,
                 'Camille', 'camille@example.org', true),
                ($1, 'erreur_chiffrage', NULL, 'accepted', NULL, NULL, NULL, true)",
    )
    .bind(idea)
    .bind(review)
    .execute(&pool)
    .await
    .unwrap();
    let escalation: Uuid = sqlx::query_scalar(
        "INSERT INTO escalations (idea_id, review_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(idea)
    .bind(review)
    .fetch_one(&pool)
    .await
    .unwrap();
    let app = TestApp::new(pool.clone(), false, &[]).await;

    let (status, body) = admin_get(&app, "/api/admin/reports").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 2);
    let (_, body) = admin_get(&app, "/api/admin/reports?status=reviewed").await;
    let report = &body["items"][0];
    assert_eq!(report["reason"], "contenu_problematique");
    assert_eq!(report["reporter_email"], "camille@example.org");
    assert_eq!(report["review"]["escalate"], true);
    assert_eq!(report["idea"]["slug"], "mutualiser-les-achats");
    let (_, body) = admin_get(
        &app,
        &format!("/api/admin/reports?idea_id={idea}&reason=erreur_chiffrage"),
    )
    .await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["review"], Value::Null);
    let (status, _) = admin_get(&app, "/api/admin/reports?idea_id=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, body) = admin_get(&app, "/api/admin/escalations?status=pending").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["review_explanation"], "Menace explicite.");
    assert!(body["items"][0]["due_at"].is_string());

    let handle = format!("/api/admin/escalations/{escalation}/handle");
    let (status, _) = post(&app, &handle, Some(json!({ "outcome": "archived" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = post(
        &app,
        &handle,
        Some(json!({ "outcome": "transmitted", "note": "Signalé à PHAROS." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "done");
    assert_eq!(body["outcome"], "transmitted");
    assert_eq!(body["note"], "Signalé à PHAROS.");
    let (status, body) = post(&app, &handle, Some(json!({ "outcome": "dismissed" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "escalation_already_handled");
    let (status, body) = post(
        &app,
        &format!("/api/admin/escalations/{}/handle", Uuid::new_v4()),
        Some(json!({ "outcome": "dismissed" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "escalation_not_found");
    let (_, body) = admin_get(&app, "/api/admin/escalations?status=pending").await;
    assert_eq!(body["total"], 0);
    assert_eq!(
        admin_actions(&pool).await,
        [("escalation.handle".to_owned(), Some(escalation.to_string()))]
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn overturns_a_refused_idea(pool: PgPool) {
    let idea = rejected_idea(&pool).await;
    let request = review_request(&pool, "idea", idea).await;
    let app = TestApp::new(pool.clone(), false, &[]).await;

    let (status, body) = admin_get(&app, "/api/admin/review-requests?status=pending").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let item = &body["items"][0];
    assert_eq!(item["target_kind"], "idea");
    assert_eq!(
        item["decision"],
        json!({ "measure": "rejected", "reason_code": "off_topic" })
    );
    assert!(item["text"].as_str().unwrap().starts_with("Mutualiser"));

    let resolve = format!("/api/admin/review-requests/{request}/resolve");
    let (status, body) = post(
        &app,
        &resolve,
        Some(json!({ "resolution": "overturned", "note": "Le texte est dans le sujet." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "done");
    assert_eq!(body["resolution"], "overturned");
    assert_eq!(body["decision"], Value::Null);
    assert_eq!(status_of(&pool, "ideas", idea).await, "accepted");
    let (code, at): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT rejection_reason_code, rejected_at FROM ideas WHERE id = $1")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((code, at), (None, None));
    assert_eq!(queued_jobs(&pool).await[0].0, "dedup_idea");
    let label: Option<String> =
        sqlx::query_scalar("SELECT human_label FROM screenings WHERE target_id = $1")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(label.as_deref(), Some("accept"));

    let (status, body) = post(&app, &resolve, Some(json!({ "resolution": "confirmed" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "review_request_already_resolved");
    let (status, body) = post(
        &app,
        &format!("/api/admin/review-requests/{}/resolve", Uuid::new_v4()),
        Some(json!({ "resolution": "confirmed" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "review_request_not_found");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn resolves_other_review_requests(pool: PgPool) {
    // A masked idea: overturning republishes it.
    let hidden = listed_idea(
        &pool,
        &IdeaSpec {
            status: "hidden",
            ..IdeaSpec::default()
        },
    )
    .await;
    let on_hidden = review_request(&pool, "idea", hidden).await;
    // A refused contribution: overturning queues the re-analysis of its idea.
    let parent = listed_idea(
        &pool,
        &IdeaSpec {
            slug: "parente",
            ..IdeaSpec::default()
        },
    )
    .await;
    let contribution: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, status, text, rejection_reason_code,
                                    rejected_at)
         VALUES ($1, 'objection', 'rejected', 'Une objection jugée hors sujet à tort.',
                 'irrelevant', now())
         RETURNING id",
    )
    .bind(parent)
    .fetch_one(&pool)
    .await
    .unwrap();
    let on_contribution = review_request(&pool, "contribution", contribution).await;
    // A refused idea whose refusal is confirmed.
    let confirmed = rejected_idea(&pool).await;
    let on_confirmed = review_request(&pool, "idea", confirmed).await;
    // A published idea: nothing to overturn.
    let published = listed_idea(
        &pool,
        &IdeaSpec {
            slug: "publiee",
            ..IdeaSpec::default()
        },
    )
    .await;
    let on_published = review_request(&pool, "idea", published).await;
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let resolve = |id: Uuid| format!("/api/admin/review-requests/{id}/resolve");
    let overturn = json!({ "resolution": "overturned" });

    let (status, body) = post(&app, &resolve(on_hidden), Some(overturn.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(status_of(&pool, "ideas", hidden).await, "published");

    let (status, body) = post(&app, &resolve(on_contribution), Some(overturn.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["idea"]["slug"], "parente");
    assert_eq!(
        status_of(&pool, "contributions", contribution).await,
        "accepted"
    );
    let kinds: Vec<String> = queued_jobs(&pool).await.into_iter().map(|j| j.0).collect();
    assert!(kinds.contains(&"generate_exports".to_owned()));
    assert!(kinds.contains(&"analyze_idea".to_owned()));

    let (status, body) = post(
        &app,
        &resolve(on_confirmed),
        Some(json!({ "resolution": "confirmed" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resolution"], "confirmed");
    assert_eq!(status_of(&pool, "ideas", confirmed).await, "rejected");

    let (status, body) = post(&app, &resolve(on_published), Some(overturn)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "decision_not_overturnable");
    let (_, body) = admin_get(&app, "/api/admin/review-requests?status=pending").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], on_published.to_string());

    let actions = admin_actions(&pool).await;
    assert_eq!(actions.len(), 3);
    assert!(actions.iter().all(|(a, _)| a == "review_request.resolve"));
}

/// The administrator refuses a published idea (§8): off the site and the exports at once, its
/// queued analysis cancelled, a refusal decided by a human for its author; then accepts it
/// again: the pipeline resumes from the duplicate detection, the slug kept.
#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn refuses_then_accepts_a_published_idea(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "refusee-par-l-admin").await;
    crate::jobs::queue::enqueue(
        &pool,
        &crate::jobs::pipeline::analyze_job(idea, crate::domain::types::AnalysisTrigger::Admin),
    )
    .await
    .unwrap();

    // A reason outside the grid, or unknown fields: refused before anything changes.
    for body in [
        json!({ "reason_code": "pas_dans_la_grille" }),
        json!({ "reason_code": "off_topic", "note": "x" }),
    ] {
        let (status, _) = post(&app, &format!("/api/admin/ideas/{idea}/reject"), Some(body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(status_of(&pool, "ideas", idea).await, "published");

    let (status, body) = post(
        &app,
        &format!("/api/admin/ideas/{idea}/reject"),
        Some(json!({ "reason_code": "off_topic" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "rejected");
    let (reason, by_admin): (Option<String>, bool) =
        sqlx::query_as("SELECT rejection_reason_code, rejected_by_admin FROM ideas WHERE id = $1")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((reason.as_deref(), by_admin), (Some("off_topic"), true));
    // The queued analysis is gone; the exports are regenerated at once.
    assert!(
        !queued_jobs(&pool)
            .await
            .iter()
            .any(|(kind, _)| kind == "analyze_idea")
    );
    assert_eq!(
        exports_job_due(&pool).await,
        (true, priority::URGENT_EXPORTS)
    );
    let (status, _, _) =
        crate::test_support::get_json(&app.router, "/api/ideas/refusee-par-l-admin", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Its author sees a refusal decided by a human, with its date of erasure.
    let statement = crate::moderation::statement::load(
        &pool,
        &crate::ai::prompts::test_prompts(),
        crate::moderation::Target::new(crate::domain::types::TargetKind::Idea, idea),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!statement.automated);
    assert_eq!(statement.reason_code, "off_topic");
    assert!(statement.purge_on.is_some());

    // Once refused, it cannot be refused again; a submitted idea cannot be refused either.
    let (status, body) = post(
        &app,
        &format!("/api/admin/ideas/{idea}/reject"),
        Some(json!({ "reason_code": "off_topic" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idea_not_rejectable");
    let submitted = idea_with_status(&pool, "submitted").await;
    let (status, body) = post(&app, &format!("/api/admin/ideas/{submitted}/accept"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idea_not_rejected");

    let (status, body) = post(&app, &format!("/api/admin/ideas/{idea}/accept"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["slug"], "refusee-par-l-admin");
    let (reason, by_admin): (Option<String>, bool) =
        sqlx::query_as("SELECT rejection_reason_code, rejected_by_admin FROM ideas WHERE id = $1")
            .bind(idea)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((reason, by_admin), (None, false));
    assert!(
        queued_jobs(&pool)
            .await
            .iter()
            .any(|(kind, payload)| kind == "dedup_idea" && payload["idea_id"] == idea.to_string())
    );

    let actions: Vec<(String, Option<String>)> = admin_actions(&pool).await;
    assert_eq!(
        actions,
        [
            ("idea.reject".to_owned(), Some(idea.to_string())),
            ("idea.accept".to_owned(), Some(idea.to_string()))
        ]
    );
    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT to_status, actor, reason_code FROM idea_events WHERE idea_id = $1 ORDER BY id",
    )
    .bind(idea)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        events,
        [
            (
                "rejected".to_owned(),
                "admin".to_owned(),
                Some("admin_rejection".to_owned())
            ),
            (
                "accepted".to_owned(),
                "admin".to_owned(),
                Some("admin_acceptance".to_owned())
            ),
        ]
    );
}

/// Every accepted status may be refused by the administrator; the pending human review of a
/// hidden idea is closed by the decision.
#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn refuses_an_idea_from_every_accepted_status(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    for from in [
        "accepted",
        "dedup",
        "queued_analysis",
        "analyzing",
        "analysis_failed",
    ] {
        let idea = idea_with_status(&pool, from).await;
        let (status, body) = post(
            &app,
            &format!("/api/admin/ideas/{idea}/reject"),
            Some(json!({ "reason_code": "off_topic" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{from}: {body}");
        assert_eq!(status_of(&pool, "ideas", idea).await, "rejected", "{from}");
    }
    let hidden = published_idea(&pool, "masquee").await;
    sqlx::query("UPDATE ideas SET status = 'hidden', hidden_reason = 'admin', hidden_at = now() WHERE id = $1")
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    let request = review_request(&pool, "idea", hidden).await;
    let (status, _) = post(
        &app,
        &format!("/api/admin/ideas/{hidden}/reject"),
        Some(json!({ "reason_code": "disrespectful" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (state, resolution): (String, Option<String>) =
        sqlx::query_as("SELECT status, resolution FROM review_requests WHERE id = $1")
            .bind(request)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (state.as_str(), resolution.as_deref()),
        ("done", Some("confirmed"))
    );
}

/// The administrator corrects the text of a refused idea before accepting it (§8): validated
/// and masked like a submission, refused for any other status; once published, the idea says
/// « texte modifié par l'éditeur du site ».
#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn corrects_the_text_of_a_refused_idea_before_accepting_it(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    let idea = published_idea(&pool, "texte-corrige").await;
    let uri = format!("/api/admin/ideas/{idea}/text");
    let corrected = "Mutualiser les achats de fournitures des communes voisines, contact : \
                     jean.dupont@example.fr, pour obtenir de meilleurs prix.";

    // Published: not editable.
    let (status, body) =
        admin_call(&app, Method::PUT, &uri, Some(&json!({ "text": corrected }))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "idea_not_rejected");

    let (status, _) = post(
        &app,
        &format!("/api/admin/ideas/{idea}/reject"),
        Some(json!({ "reason_code": "personal_data" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = admin_call(
        &app,
        Method::PUT,
        &uri,
        Some(&json!({ "text": "Trop court." })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "text_too_short");

    let (status, body) =
        admin_call(&app, Method::PUT, &uri, Some(&json!({ "text": corrected }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let text = body["text"].as_str().unwrap();
    assert!(text.contains("[donnée masquée]"), "{text}");
    assert!(!text.contains("example.fr"), "{text}");
    assert!(body["text_edited_at"].is_string());
    let (_, list) = admin_get(&app, "/api/admin/ideas?status=rejected").await;
    assert_eq!(list["items"][0]["text"], text);
    // The score is shown on 1000 in the admin too, never as an amount of euros.
    sqlx::query("UPDATE ideas SET score_eur = 524000000 WHERE id = $1")
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    let (_, list) = admin_get(&app, "/api/admin/ideas?status=rejected").await;
    assert_eq!(list["items"][0]["points"], 715);

    let (status, _) = post(&app, &format!("/api/admin/ideas/{idea}/accept"), None).await;
    assert_eq!(status, StatusCode::OK);
    // Published again at the end of its analysis: the page says the publisher edited it.
    sqlx::query("UPDATE ideas SET status = 'published' WHERE id = $1")
        .bind(idea)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, page) =
        crate::test_support::get_json(&app.router, "/api/ideas/texte-corrige", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["original"]["edited_by_publisher"], true);
    assert_eq!(page["original"]["text"], text);

    let actions: Vec<String> = admin_actions(&pool)
        .await
        .into_iter()
        .map(|a| a.0)
        .collect();
    assert_eq!(actions, ["idea.reject", "idea.edit_text", "idea.accept"]);
}
