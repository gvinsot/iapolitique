//! Integration tests of the public portal routes (`GET /api/ideas`, `GET /api/ideas/{slug}`).

use axum::http::{StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::test_support::{IdeaSpec, TestApp, get_json, listed_idea};

async fn app(pool: PgPool) -> TestApp {
    TestApp::new(pool, false, &[("TRAEFIK_HOST", "iapolitique.example")]).await
}

fn slugs(body: &Value) -> Vec<&str> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["slug"].as_str().unwrap())
        .collect()
}

/// Three published ideas, one hidden, one merged into the first.
async fn catalogue(pool: &PgPool) -> Uuid {
    let first = listed_idea(
        pool,
        &IdeaSpec {
            slug: "mutualiser-les-achats",
            score_eur: 3_000_000,
            annual_gain_median_eur: 5_000_000,
            feasibility: 3,
            time_to_effect_months: 24,
            published_days_ago: 30,
            ..IdeaSpec::default()
        },
    )
    .await;
    listed_idea(
        pool,
        &IdeaSpec {
            slug: "eclairage-public-nocturne",
            title: "Éteindre l'éclairage public la nuit",
            summary: "Couper l'éclairage public entre 1 h et 5 h dans les communes.",
            admin_level: "commune",
            category: "achats_publics",
            territory: Some(("Saint-Étienne", "42218")),
            score_eur: 2_000_000,
            annual_gain_median_eur: 9_000_000,
            feasibility: 5,
            time_to_effect_months: 6,
            published_days_ago: 2,
            ..IdeaSpec::default()
        },
    )
    .await;
    listed_idea(
        pool,
        &IdeaSpec {
            slug: "taxe-sur-les-residences-secondaires",
            title: "Relever la taxe sur les résidences secondaires",
            summary: "Augmenter la majoration de taxe d'habitation dans les zones tendues.",
            admin_level: "commune",
            category: "achats_publics",
            kind: "revenue",
            territory: Some(("Lyon", "69123")),
            score_eur: 1_000_000,
            annual_gain_median_eur: 1_000_000,
            feasibility: 4,
            time_to_effect_months: 12,
            published_days_ago: 10,
            ..IdeaSpec::default()
        },
    )
    .await;
    listed_idea(
        pool,
        &IdeaSpec {
            slug: "idee-masquee",
            title: "Idée masquée sur les achats",
            score_eur: 9_000_000,
            status: "hidden",
            ..IdeaSpec::default()
        },
    )
    .await;
    first
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn lists_published_ideas_by_score_with_cache_headers(pool: PgPool) {
    catalogue(&pool).await;
    let app = app(pool).await;
    let (status, headers, body) = get_json(&app.router, "/api/ideas", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=60");
    assert_eq!(
        slugs(&body),
        [
            "mutualiser-les-achats",
            "eclairage-public-nocturne",
            "taxe-sur-les-residences-secondaires"
        ]
    );
    assert_eq!(body["total"], 3);
    assert_eq!(body["total_pages"], 1);
    assert_eq!(body["search"], Value::Null);
    let item = &body["items"][1];
    assert_eq!(
        item["category"],
        json!({ "key": "achats_publics", "label": "Achats publics" })
    );
    assert_eq!(
        item["territory"],
        json!({ "name": "Saint-Étienne", "insee_code": "42218" })
    );
    assert_eq!(
        item["annual_gain_eur"],
        json!({ "low": 4_500_000, "median": 9_000_000, "high": 18_000_000 })
    );
    assert_eq!(item["feasibility"], 5);
    assert_eq!(item["proposal_count"], 1);
    assert!(item.get("id").is_none());
    assert_eq!(body["items"][0]["territory"], Value::Null);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn sorts_filters_and_paginates(pool: PgPool) {
    catalogue(&pool).await;
    let app = app(pool).await;
    let list = |uri: &'static str| {
        let router = app.router.clone();
        async move {
            let (status, _, body) = get_json(&router, uri, &[]).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            body
        }
    };
    let order = |body: Value| {
        slugs(&body)
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order(list("/api/ideas?sort=gain").await),
        [
            "eclairage-public-nocturne",
            "mutualiser-les-achats",
            "taxe-sur-les-residences-secondaires"
        ]
    );
    assert_eq!(
        order(list("/api/ideas?sort=feasibility").await),
        [
            "eclairage-public-nocturne",
            "taxe-sur-les-residences-secondaires",
            "mutualiser-les-achats"
        ]
    );
    assert_eq!(
        order(list("/api/ideas?sort=delay").await),
        [
            "eclairage-public-nocturne",
            "taxe-sur-les-residences-secondaires",
            "mutualiser-les-achats"
        ]
    );
    assert_eq!(
        order(list("/api/ideas?sort=recent").await),
        [
            "eclairage-public-nocturne",
            "taxe-sur-les-residences-secondaires",
            "mutualiser-les-achats"
        ]
    );

    assert_eq!(
        order(list("/api/ideas?admin_level=commune&kind=revenue").await),
        ["taxe-sur-les-residences-secondaires"]
    );
    assert_eq!(
        order(list("/api/ideas?category=maladie").await),
        ["mutualiser-les-achats"]
    );
    // Territory: name without accents and case, or INSEE code.
    assert_eq!(
        order(list("/api/ideas?territory=saint-etienne").await),
        ["eclairage-public-nocturne"]
    );
    assert_eq!(
        order(list("/api/ideas?territory=69123").await),
        ["taxe-sur-les-residences-secondaires"]
    );
    // LIKE wildcards are literal.
    assert_eq!(
        order(list("/api/ideas?territory=%25").await),
        Vec::<String>::new()
    );
    assert_eq!(
        order(list("/api/ideas?territory=_").await),
        Vec::<String>::new()
    );

    let body = list("/api/ideas?per_page=2&page=2").await;
    assert_eq!(order(body.clone()), ["taxe-sur-les-residences-secondaires"]);
    assert_eq!(
        (body["total"].clone(), body["total_pages"].clone()),
        (json!(3), json!(2))
    );
    let body = list("/api/ideas?per_page=2&page=9").await;
    assert_eq!(body["items"], json!([]));
    assert_eq!(body["total"], 3);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn searches_in_french_then_by_similarity(pool: PgPool) {
    catalogue(&pool).await;
    let app = app(pool).await;
    let search = |q: &'static str| {
        let router = app.router.clone();
        async move {
            let (status, _, body) = get_json(&router, q, &[]).await;
            assert_eq!(status, StatusCode::OK, "{q}: {body}");
            body
        }
    };
    // Stemming and accents: « eteindre eclairages » matches « Éteindre l'éclairage ».
    let body = search("/api/ideas?q=eteindre%20eclairages").await;
    assert_eq!(slugs(&body), ["eclairage-public-nocturne"]);
    assert_eq!(body["search"], "exact");
    // Web search syntax: exclusion.
    let body = search("/api/ideas?q=public%20-nuit").await;
    assert_eq!(slugs(&body), ["mutualiser-les-achats"]);
    assert_eq!(body["search"], "exact");
    // Typo: no full text match, approaching results.
    let body = search("/api/ideas?q=residance%20secondere").await;
    assert_eq!(slugs(&body), ["taxe-sur-les-residences-secondaires"]);
    assert_eq!(body["search"], "approximate");
    // The hidden idea is never found.
    let body = search("/api/ideas?q=masquee").await;
    assert_eq!(body["total"], 0);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn refuses_invalid_queries_without_500(pool: PgPool) {
    catalogue(&pool).await;
    let app = app(pool).await;
    for uri in [
        "/api/ideas?per_page=51",
        "/api/ideas?per_page=0",
        "/api/ideas?page=0",
        "/api/ideas?page=-1",
        "/api/ideas?page=x",
        "/api/ideas?sort=score%3B%20DROP%20TABLE%20ideas",
        "/api/ideas?admin_level=planete",
        "/api/ideas?kind=autre",
        "/api/ideas?category=inconnue",
        "/api/ideas?unknown=1",
    ] {
        let (status, _, body) = get_json(&app.router, uri, &[]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "invalid_query", "{uri}");
    }
    let long = format!("/api/ideas?q={}", "a".repeat(201));
    let (status, _, _) = get_json(&app.router, &long, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Injection attempts in the search and the territory are plain text.
    for uri in [
        "/api/ideas?q=%27%20OR%201%3D1%20--",
        "/api/ideas?q=%27%29%3B%20DELETE%20FROM%20ideas%3B%20--",
        "/api/ideas?territory=%27%20OR%20%271%27%3D%271",
        "/api/ideas?q=%21%21%20%26%26%20%7C%7C%20%3A%2A",
    ] {
        let (status, _, body) = get_json(&app.router, uri, &[]).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        // At most an approaching match on a word of the payload, never more than the
        // published ideas.
        assert!(body["total"].as_u64().unwrap() <= 3, "{uri}");
        assert!(!slugs(&body).contains(&"idee-masquee"), "{uri}");
    }
    let (_, _, body) = get_json(&app.router, "/api/ideas", &[]).await;
    assert_eq!(body["total"], 3);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn shows_a_published_idea(pool: PgPool) {
    let idea = catalogue(&pool).await;
    // An integrated contribution (with the analysis answer), a pending one, a duplicate.
    let analysis: Uuid = sqlx::query_scalar("SELECT current_analysis_id FROM ideas WHERE id = $1")
        .bind(idea)
        .fetch_one(&pool)
        .await
        .unwrap();
    let integrated: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, status, text, neutral_summary,
                                    integrated_in_analysis_id, integrated_at)
         VALUES ($1, 'objection', 'integrated', 'Texte brut de la contribution, jamais publié.',
                 'Les fournisseurs locaux pourraient perdre des marchés.', $2, now())
         RETURNING id",
    )
    .bind(idea)
    .bind(analysis)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE analyses SET content = jsonb_set(content, '{contributions_considered}',
             jsonb_build_array(jsonb_build_object('contribution_id', $2::text,
                 'summary', 'Résumé', 'taken_into_account', false,
                 'response', 'Le risque est mentionné dans les inconvénients.')))
         WHERE id = $1",
    )
    .bind(analysis)
    .bind(integrated)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, status, text)
         VALUES ($1, 'amelioration', 'accepted', 'Texte en attente de la prochaine analyse.')",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on)
         VALUES ($1, 'La direction des achats', 'Ce programme existe déjà.', current_date)",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();

    let app = app(pool).await;
    let (status, headers, body) =
        get_json(&app.router, "/api/ideas/mutualiser-les-achats", &[]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=60");
    assert_eq!(body["slug"], "mutualiser-les-achats");
    assert_eq!(
        body["canonical_url"],
        "https://iapolitique.example/idees/mutualiser-les-achats"
    );
    assert_eq!(body["original"]["personal_data_masked"], true);
    assert!(
        body["original"]["text"]
            .as_str()
            .unwrap()
            .contains("[donnée masquée]")
    );
    assert_eq!(body["ai"]["ai_generated"], true);
    assert_eq!(body["ai"]["human_reviewed"], false);
    assert_eq!(body["ai"]["provider"], "Mistral AI");
    assert_eq!(body["ai"]["model"], "mistral-large-latest");
    assert_eq!(body["ai"]["analysis_version"], 1);
    assert_eq!(body["ai"]["schema_version"], "sha256:0123456789ab");
    assert_eq!(body["score"]["label"], "score calculé par algorithme");
    assert_eq!(body["score"]["score"], 3_000_000);
    assert_eq!(body["score"]["scoring_version"], "1");

    let analysis = &body["analysis"];
    assert_eq!(analysis["legal_vehicle"], "organisation_interne");
    assert_eq!(analysis["feasibility"]["score"], 3);
    assert!(
        !analysis["feasibility"]["obstacles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(analysis["annual_gain_eur"]["median"], 5_000_000);
    assert!(
        !analysis["annual_gain_eur"]["calculation"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    let impacts = analysis["impacts"].as_array().unwrap();
    assert_eq!(impacts.len(), 9);
    assert_eq!(impacts[0]["public"], "menages_modestes");
    assert_eq!(impacts[0]["label"], "Ménages modestes");
    assert_eq!(analysis["sources"][0]["id"], "S1");
    assert!(analysis.get("contributions_considered").is_none());

    assert_eq!(
        body["contributions"],
        json!([{
            "kind": "objection",
            "summary": "Les fournisseurs locaux pourraient perdre des marchés.",
            "taken_into_account": false,
            "response": "Le risque est mentionné dans les inconvénients.",
            "integrated_on": body["contributions"][0]["integrated_on"].clone(),
        }])
    );
    let serialized = body.to_string();
    assert!(!serialized.contains("Texte brut"));
    assert!(!serialized.contains(&integrated.to_string()));
    assert!(!serialized.contains(&idea.to_string()));
    assert_eq!(
        body["rights_of_reply"][0]["author_label"],
        "La direction des achats"
    );
    assert_eq!(
        body["rights_of_reply"][0]["body"],
        "Ce programme existe déjà."
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn hidden_unknown_and_merged_ideas(pool: PgPool) {
    let parent = catalogue(&pool).await;
    // A merged duplicate that kept a slug points to its parent.
    let merged: Uuid = sqlx::query_scalar(
        "INSERT INTO ideas (original_text, status, parent_idea_id, slug)
         VALUES ('Texte du doublon.', 'merged', $1, 'ancien-doublon') RETURNING id",
    )
    .bind(parent)
    .fetch_one(&pool)
    .await
    .unwrap();
    let app = app(pool.clone()).await;
    for uri in [
        "/api/ideas/idee-masquee",
        "/api/ideas/inconnue",
        "/api/ideas/Invalid_Slug",
        "/api/ideas/%27%20OR%201%3D1",
    ] {
        let (status, _, body) = get_json(&app.router, uri, &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["error"]["code"], "idea_not_found", "{uri}");
    }
    let (status, headers, body) = get_json(&app.router, "/api/ideas/ancien-doublon", &[]).await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        headers[header::LOCATION],
        "/api/ideas/mutualiser-les-achats"
    );
    assert_eq!(body, json!({ "slug": "mutualiser-les-achats" }));

    // Once the parent is hidden, the duplicate is not found either.
    sqlx::query("UPDATE ideas SET status = 'hidden' WHERE id = $1")
        .bind(parent)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, _) = get_json(&app.router, "/api/ideas/ancien-doublon", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = merged;
}
