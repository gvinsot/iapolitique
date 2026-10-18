//! Integration tests of the schema against a real PostgreSQL (`DATABASE_URL`).

use std::collections::BTreeSet;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::status::{ContributionStatus, IdeaStatus, ReportStatus};
use crate::domain::types::{
    Actor, AdminLevel, AiCallStatus, AiProvider, AiPurpose, AnalysisTrigger, Confidence,
    ContributionKind, Decision, ExportFormat, IdeaKind, LegalVehicle, ReportReason, ReviewOutcome,
    ScreeningOutcome, TargetKind,
};

/// Values quoted in the definition of a CHECK constraint.
async fn check_values(pool: &PgPool, table: &str, column: &str) -> BTreeSet<String> {
    let name = format!("{table}_{column}_check");
    let (definition,): (String,) = sqlx::query_as(
        "SELECT pg_get_constraintdef(c.oid)
         FROM pg_constraint c JOIN pg_class t ON t.oid = c.conrelid
         WHERE t.relname = $1 AND c.conname = $2",
    )
    .bind(table)
    .bind(&name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|err| panic!("constraint {name}: {err}"));
    definition
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

macro_rules! assert_check_matches {
    ($pool:expr, $table:literal, $column:literal, $enum:ty) => {{
        let expected: BTreeSet<String> =
            <$enum>::ALL.iter().map(|v| v.as_str().to_owned()).collect();
        let actual = check_values($pool, $table, $column).await;
        assert_eq!(
            actual,
            expected,
            "{}.{} does not match {}",
            $table,
            $column,
            stringify!($enum)
        );
    }};
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn check_constraints_match_domain_enums(pool: PgPool) {
    let pool = &pool;
    assert_check_matches!(pool, "ideas", "status", IdeaStatus);
    assert_check_matches!(pool, "ideas", "submitted_admin_level", AdminLevel);
    assert_check_matches!(pool, "ideas", "admin_level", AdminLevel);
    assert_check_matches!(pool, "ideas", "kind", IdeaKind);
    assert_check_matches!(pool, "ideas", "confidence", Confidence);
    assert_check_matches!(pool, "idea_events", "actor", Actor);
    assert_check_matches!(pool, "contribution_events", "actor", Actor);
    assert_check_matches!(pool, "report_events", "actor", Actor);
    assert_check_matches!(pool, "analyses", "trigger", AnalysisTrigger);
    assert_check_matches!(pool, "analyses", "kind", IdeaKind);
    assert_check_matches!(pool, "analyses", "admin_level", AdminLevel);
    assert_check_matches!(pool, "analyses", "legal_vehicle", LegalVehicle);
    assert_check_matches!(pool, "analyses", "confidence", Confidence);
    assert_check_matches!(pool, "contributions", "kind", ContributionKind);
    assert_check_matches!(pool, "contributions", "status", ContributionStatus);
    assert_check_matches!(pool, "reports", "reason", ReportReason);
    assert_check_matches!(pool, "reports", "status", ReportStatus);
    assert_check_matches!(pool, "report_reviews", "outcome", ReviewOutcome);
    assert_check_matches!(pool, "tracking_tokens", "target_kind", TargetKind);
    assert_check_matches!(pool, "screenings", "target_kind", TargetKind);
    assert_check_matches!(pool, "screenings", "outcome", ScreeningOutcome);
    assert_check_matches!(pool, "screenings", "second_opinion_decision", Decision);
    assert_check_matches!(pool, "screenings", "final_decision", Decision);
    assert_check_matches!(pool, "screenings", "human_label", Decision);
    assert_check_matches!(pool, "ai_calls", "provider", AiProvider);
    assert_check_matches!(pool, "ai_calls", "purpose", AiPurpose);
    assert_check_matches!(pool, "ai_calls", "status", AiCallStatus);
    assert_check_matches!(pool, "ai_calls", "target_kind", TargetKind);
    assert_check_matches!(pool, "rate_limit_counters", "action", TargetKind);
    assert_check_matches!(pool, "exports", "format", ExportFormat);
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn migrations_are_idempotent_and_extensions_present(pool: PgPool) {
    crate::db::migrate(&pool).await.unwrap();
    crate::db::ping(&pool).await.unwrap();
    let extensions: Vec<(String,)> = sqlx::query_as(
        "SELECT extname::text FROM pg_extension
         WHERE extname IN ('vector', 'pg_trgm', 'unaccent', 'pgcrypto') ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<_> = extensions.into_iter().map(|(n,)| n).collect();
    assert_eq!(names, ["pg_trgm", "pgcrypto", "unaccent", "vector"]);
}

async fn insert_idea(pool: &PgPool, text: &str) -> Uuid {
    let (id,): (Uuid,) =
        sqlx::query_as("INSERT INTO ideas (original_text) VALUES ($1) RETURNING id")
            .bind(text)
            .fetch_one(pool)
            .await
            .unwrap();
    id
}

/// Inserts an analysis and publishes the idea with it.
async fn publish(pool: &PgPool, idea: Uuid, slug: &str, title: &str, summary: &str) -> Uuid {
    let (analysis,): (Uuid,) = sqlx::query_as(
        "INSERT INTO analyses (
            idea_id, version, trigger, content, title, summary, kind, category, admin_level,
            legal_vehicle, annual_gain_low_eur, annual_gain_median_eur, annual_gain_high_eur,
            implementation_cost_low_eur, implementation_cost_median_eur,
            implementation_cost_high_eur, time_to_effect_months, feasibility, confidence,
            score_eur, net_annual_eur, feasibility_factor, confidence_factor, delay_factor,
            model, prompt_version, schema_version, scoring_version
         ) VALUES (
            $1, 1, 'initial', '{}', $2, $3, 'savings', 'achats_publics', 'etat',
            'decret', 1000, 2000, 3000, 0, 500, 1000, 12, 4, 'medium',
            1000, 1900, 0.8, 0.75, 0.6667, 'mistral-large-latest', '1', '1', '1'
         ) RETURNING id",
    )
    .bind(idea)
    .bind(title)
    .bind(summary)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE ideas SET status = 'published', slug = $2, title = $3, summary = $4,
            current_analysis_id = $5, score_eur = 1000, published_at = now(),
            first_published_at = now()
         WHERE id = $1",
    )
    .bind(idea)
    .bind(slug)
    .bind(title)
    .bind(summary)
    .bind(analysis)
    .execute(pool)
    .await
    .unwrap();
    analysis
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn analyses_store_both_analysis_prompt_versions(pool: PgPool) {
    let idea = insert_idea(&pool, "Mutualiser les achats de fournitures.").await;
    let analysis = publish(&pool, idea, "mutualiser-achats", "Mutualiser", "Résumé").await;
    // Single-call mode: no research step, hence no research prompt version.
    let (research,): (Option<String>,) =
        sqlx::query_as("SELECT research_prompt_version FROM analyses WHERE id = $1")
            .bind(analysis)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(research, None);
    let (prompt, research): (String, Option<String>) = sqlx::query_as(
        "UPDATE analyses SET research_prompt_version = '1.0.0' WHERE id = $1
         RETURNING prompt_version, research_prompt_version",
    )
    .bind(analysis)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((prompt.as_str(), research.as_deref()), ("1", Some("1.0.0")));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn report_reviews_keep_corrections_only_for_updates(pool: PgPool) {
    let idea = insert_idea(&pool, "Mutualiser les achats de fournitures.").await;
    let analysis = publish(&pool, idea, "mutualiser-achats", "Mutualiser", "Résumé").await;
    let insert = "INSERT INTO report_reviews
            (idea_id, analysis_id, outcome, explanation, corrections, model, prompt_version)
         VALUES ($1, $2, $3, 'Explication.', $4, 'mistral-large-latest', '1.0.0')";
    for (outcome, corrections) in [("update", "Corriger la source [S2]."), ("maintain", "")] {
        sqlx::query(insert)
            .bind(idea)
            .bind(analysis)
            .bind(outcome)
            .bind(corrections)
            .execute(&pool)
            .await
            .unwrap();
    }
    let rejected = sqlx::query(insert)
        .bind(idea)
        .bind(analysis)
        .bind("hide")
        .bind("Corriger quand même.")
        .execute(&pool)
        .await;
    assert!(
        rejected.is_err(),
        "corrections are only allowed with outcome = update"
    );
    // Rows written without the column (previous image after a rollback) default to empty.
    sqlx::query(
        "INSERT INTO report_reviews (idea_id, outcome, explanation, model, prompt_version)
         VALUES ($1, 'maintain', 'Explication.', 'mistral-large-latest', '1.0.0')",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn idea_defaults_and_integrity_constraints(pool: PgPool) {
    let id = insert_idea(&pool, "Mutualiser les achats.").await;
    let (status, proposals, created_v7): (String, i32, bool) = sqlx::query_as(
        "SELECT status, proposal_count, uuid_extract_version(id) = 7 FROM ideas WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, IdeaStatus::Submitted.as_str());
    assert_eq!(proposals, 1);
    assert!(created_v7);

    // A merged idea needs a parent, a purged idea has no text, publication needs an analysis.
    for statement in [
        "UPDATE ideas SET status = 'merged' WHERE id = $1",
        "UPDATE ideas SET status = 'purged' WHERE id = $1",
        "UPDATE ideas SET original_text = NULL WHERE id = $1",
        "UPDATE ideas SET status = 'published', slug = 'x', title = 't' WHERE id = $1",
        "UPDATE ideas SET slug = 'Not A Slug' WHERE id = $1",
        "UPDATE ideas SET parent_idea_id = id WHERE id = $1",
    ] {
        let result = sqlx::query(statement).bind(id).execute(&pool).await;
        assert!(result.is_err(), "should fail: {statement}");
    }
    sqlx::query(
        "UPDATE ideas SET status = 'purged', original_text = NULL, original_text_purged_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn updated_at_is_maintained(pool: PgPool) {
    let id = insert_idea(&pool, "Texte").await;
    sqlx::query("UPDATE ideas SET created_at = now() - interval '1 day', updated_at = now() - interval '1 day' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (fresh,): (bool,) =
        sqlx::query_as("SELECT updated_at > now() - interval '1 minute' FROM ideas WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(fresh, "the trigger overrides updated_at");
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn full_text_search_ignores_accents(pool: PgPool) {
    let a = insert_idea(&pool, "a").await;
    let b = insert_idea(&pool, "b").await;
    publish(
        &pool,
        a,
        "reduire-le-parc-automobile-de-l-etat",
        "Réduire le parc automobile de l'État",
        "Diminuer le nombre de véhicules de service des ministères.",
    )
    .await;
    publish(
        &pool,
        b,
        "mutualiser-les-achats-des-communes",
        "Mutualiser les achats des communes",
        "Groupements de commandes entre collectivités.",
    )
    .await;

    for (query, expected) in [
        ("etat", a),
        ("véhicule", a),
        ("vehicules", a),
        ("collectivites", b),
        ("achat", b),
    ] {
        let found: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM ideas
             WHERE status = 'published'
               AND search_vector @@ websearch_to_tsquery('french_unaccent', $1)",
        )
        .bind(query)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(found, vec![(expected,)], "query {query}");
    }
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn embeddings_support_cosine_nearest_neighbour(pool: PgPool) {
    let vector = |hot: usize| {
        let values: Vec<String> = (0..1024)
            .map(|i| {
                if i == hot {
                    "1".to_owned()
                } else {
                    "0.01".to_owned()
                }
            })
            .collect();
        format!("[{}]", values.join(","))
    };
    let a = insert_idea(&pool, "a").await;
    let b = insert_idea(&pool, "b").await;
    for (idea, hot) in [(a, 1), (b, 2)] {
        sqlx::query(
            "INSERT INTO idea_embeddings (idea_id, model, embedding, content_sha256)
             VALUES ($1, 'mistral-embed', $2::vector, sha256('x'::bytea))",
        )
        .bind(idea)
        .bind(vector(hot))
        .execute(&pool)
        .await
        .unwrap();
    }
    let (nearest, similarity): (Uuid, f64) = sqlx::query_as(
        "SELECT idea_id, 1 - (embedding <=> $1::vector) FROM idea_embeddings
         ORDER BY embedding <=> $1::vector LIMIT 1",
    )
    .bind(vector(2))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(nearest, b);
    assert!(similarity > 0.999);

    let wrong_dimension = sqlx::query(
        "INSERT INTO idea_embeddings (idea_id, model, embedding, content_sha256)
         VALUES ($1, 'm', '[1,2,3]'::vector, sha256('x'::bytea))",
    )
    .bind(insert_idea(&pool, "c").await)
    .execute(&pool)
    .await;
    assert!(wrong_dimension.is_err());
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn duplicate_contribution_and_report_constraints(pool: PgPool) {
    let parent = insert_idea(&pool, "parent").await;
    let child = insert_idea(&pool, "child").await;
    // A duplicate needs its source idea, other kinds need a text.
    assert!(
        sqlx::query("INSERT INTO contributions (idea_id, kind) VALUES ($1, 'doublon')")
            .bind(parent)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO contributions (idea_id, kind) VALUES ($1, 'objection')")
            .bind(parent)
            .execute(&pool)
            .await
            .is_err()
    );
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, status, source_idea_id)
         VALUES ($1, 'doublon', 'accepted', $2)",
    )
    .bind(parent)
    .bind(child)
    .execute(&pool)
    .await
    .unwrap();
    // A reviewed report needs its review.
    assert!(
        sqlx::query(
            "INSERT INTO reports (idea_id, reason, status) VALUES ($1, 'autre', 'reviewed')"
        )
        .bind(parent)
        .execute(&pool)
        .await
        .is_err()
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn tracking_tokens_store_only_hashes(pool: PgPool) {
    let idea = insert_idea(&pool, "x").await;
    let short = sqlx::query(
        "INSERT INTO tracking_tokens (token_hash, target_kind, target_id) VALUES ('\\x00', 'idea', $1)",
    )
    .bind(idea)
    .execute(&pool)
    .await;
    assert!(short.is_err());
    sqlx::query(
        "INSERT INTO tracking_tokens (token_hash, target_kind, target_id)
         VALUES (sha256('token'::bytea), 'idea', $1)",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
}
