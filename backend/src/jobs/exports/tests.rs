use regex::Regex;
use serde_json::{Value, json};
use sqlx::PgPool;

use super::*;
use crate::jobs::{pipeline, queue};
use crate::test_support::{TestApp, analysis_output, idea_with_status, published_idea};

#[test]
fn csv_cells_are_quoted_and_neutralized() {
    for (value, expected) in [
        ("simple", "simple"),
        ("=1+1", "'=1+1"),
        ("+33", "'+33"),
        ("-5", "'-5"),
        ("@SUM(A1)", "'@SUM(A1)"),
        ("\tcmd", "'\tcmd"),
        ("\rcmd", "\"'\rcmd\""),
        ("a,b", "\"a,b\""),
        ("dit \"oui\"", "\"dit \"\"oui\"\"\""),
        ("ligne\nsuivante", "\"ligne\nsuivante\""),
        (
            "=HYPERLINK(\"x\",\"y\")",
            "\"'=HYPERLINK(\"\"x\"\",\"\"y\"\")\"",
        ),
        ("", ""),
    ] {
        assert_eq!(csv_cell(value), expected, "{value:?}");
    }
    assert_eq!(
        canonical_url("iapolitique.fr", "une-idee"),
        "https://iapolitique.fr/idees/une-idee"
    );
}

#[test]
fn metadata_carries_the_notices() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
    let meta = metadata(date, 3);
    assert_eq!(meta["licence"]["spdx"], "etalab-2.0");
    assert_eq!(meta["generated_at"], "2026-10-19");
    assert_eq!(meta["export_schema_version"], EXPORT_SCHEMA_VERSION);
    assert_eq!(meta["idea_count"], 3);
    assert_eq!(
        meta["attribution"],
        "Source : iapolitique.fr – Propositions citoyennes et analyses générées par IA \
         (Mistral AI), mise à jour du 2026-10-19 – Licence Ouverte 2.0"
    );
    assert_eq!(meta["ai_notice"], AI_NOTICE);
    for key in [
        "third_party_content",
        "personal_data",
        "irrevocability",
        "resync",
        "exclusions",
        "unofficial_notice",
        "score_notice",
    ] {
        assert!(meta[key].as_str().is_some_and(|s| !s.is_empty()), "{key}");
    }
    assert!(LICENCE_TEXT.starts_with("# LICENCE OUVERTE 2.0/OPEN LICENCE 2.0"));
    assert!(LICENCE_TEXT.contains("Cette licence est une version 2.0 de la Licence Ouverte."));
}

/// format, content, SHA-256, size, layout version.
type ExportRow = (String, Vec<u8>, Vec<u8>, i64, Option<String>);

/// Files of the latest generation, by format.
async fn latest(pool: &PgPool) -> BTreeMap<String, Vec<u8>> {
    let rows: Vec<ExportRow> = sqlx::query_as(
        "SELECT format, content, content_sha256, size_bytes, export_schema_version
         FROM exports
         WHERE generation_id = (SELECT generation_id FROM exports
                                ORDER BY generated_at DESC, id DESC LIMIT 1)",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(format, content, hash, size, version)| {
            use sha2::Digest;
            assert_eq!(hash, sha2::Sha256::digest(&content).to_vec());
            assert_eq!(size, content.len() as i64);
            assert_eq!(version.as_deref(), Some(EXPORT_SCHEMA_VERSION));
            (format, content)
        })
        .collect()
}

async fn run(app: &TestApp) {
    let ctx = app.job_context();
    queue::enqueue(
        &app.pool,
        &crate::jobs::NewJob::new(JobKind::GenerateExports),
    )
    .await
    .unwrap();
    let job = queue::claim(&app.pool, "test-worker", &[JobKind::GenerateExports])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ExportsHandler.handle(&ctx, &job).await,
        Ok(JobOutcome::Done)
    );
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn exports_only_published_ideas(pool: PgPool) {
    let app = TestApp::new(
        pool.clone(),
        false,
        &[("TRAEFIK_HOST", "qa.iapolitique.fr")],
    )
    .await;
    let idea = published_idea(&pool, "parente").await;
    let mut content = analysis_output();
    content["contributions_considered"] = json!([{
        "contribution_id": "00000000-0000-0000-0000-000000000001",
        "summary": "Résumé.", "taken_into_account": true, "response": "Oui."
    }]);
    sqlx::query("UPDATE analyses SET content = $2 WHERE idea_id = $1;")
        .bind(idea)
        .bind(&content)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE ideas SET original_text = $2, proposal_count = 2 WHERE id = $1")
        .bind(idea)
        .bind("=HYPERLINK(\"https://exemple.test\") mutualiser les achats, simplement.")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO contributions (idea_id, kind, status, text, neutral_summary,
                                    integrated_in_analysis_id, integrated_at)
         SELECT $1, 'objection', 'integrated', 'Texte brut jamais exporté.',
                'Risque de dépendance aux fournisseurs.', current_analysis_id, now()
         FROM ideas WHERE id = $1",
    )
    .bind(idea)
    .execute(&pool)
    .await
    .unwrap();
    // Not exported: hidden or not yet published ideas.
    let hidden = published_idea(&pool, "masquee").await;
    sqlx::query("UPDATE ideas SET status = 'hidden', hidden_reason = 'admin' WHERE id = $1")
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    idea_with_status(&pool, "queued_analysis").await;

    run(&app).await;
    let files = latest(&pool).await;
    assert_eq!(
        files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["csv", "json", "license", "metadata"]
    );
    assert_eq!(files["license"], LICENCE_TEXT.as_bytes());

    let document: Value = serde_json::from_slice(&files["json"]).unwrap();
    let metadata: Value = serde_json::from_slice(&files["metadata"]).unwrap();
    assert_eq!(document["metadata"], metadata);
    assert_eq!(metadata["idea_count"], 1);
    assert_eq!(document["licence_text"], LICENCE_TEXT);
    let ideas = document["ideas"].as_array().unwrap();
    assert_eq!(ideas.len(), 1);
    let exported = &ideas[0];
    assert_eq!(exported["slug"], "parente");
    assert_eq!(exported["url"], "https://qa.iapolitique.fr/idees/parente");
    assert_eq!(exported["proposal_count"], 2);
    assert_eq!(exported["original"]["source_type"], "user");
    assert_eq!(exported["original"]["attribution"], ATTRIBUTION);
    assert_eq!(exported["original"]["personal_data_masked"], true);
    let analysis = &exported["analysis"];
    assert_eq!(analysis["ai_generated"], true);
    assert_eq!(analysis["human_reviewed"], false);
    assert_eq!(analysis["ai_provider"], "Mistral AI");
    assert_eq!(analysis["ai_model"], "mistral-large-latest");
    assert_eq!(analysis["analysis_version"], 1);
    assert_eq!(
        analysis["title"],
        "Mutualiser les achats courants des hôpitaux publics"
    );
    assert!(analysis.get("contributions_considered").is_none());
    let source = &analysis["sources"][0];
    assert_eq!(source["publisher"], "Cour des comptes");
    assert!(source.get("used_for").is_none());
    assert_eq!(
        exported["contributions"],
        json!([{ "kind": "objection", "summary": "Risque de dépendance aux fournisseurs.",
                 "ai_generated": true }])
    );
    assert_eq!(exported["score"]["score_eur"], 1000);
    let date = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
    for value in [
        &exported["date_publication"],
        &exported["date_derniere_mise_a_jour"],
        &analysis["generated_at"],
        &source["consulted_on"],
        &metadata["generated_at"],
    ] {
        assert!(date.is_match(value.as_str().unwrap()), "{value}");
    }
    // No internal identifier, raw contribution text or hidden idea.
    let text = String::from_utf8(files["json"].clone()).unwrap();
    assert!(!text.contains(&idea.to_string()));
    assert!(!text.contains("00000000-0000-0000-0000-000000000001"));
    assert!(!text.contains("Texte brut"));
    assert!(!text.contains("masquee"));

    let csv = String::from_utf8(files["csv"].clone()).unwrap();
    assert!(csv.starts_with("\u{FEFF}slug,url,date_publication,"));
    let lines: Vec<&str> = csv.trim_end_matches("\r\n").split("\r\n").collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[1].starts_with("parente,https://qa.iapolitique.fr/idees/parente,"));
    assert!(lines[1].contains(
        "\"'=HYPERLINK(\"\"https://exemple.test\"\") mutualiser les achats, simplement.\""
    ));
    assert!(lines[1].contains(",user,Contribution anonyme – iapolitique.fr,"));
    assert!(lines[1].contains("S1 : La fonction achats des hôpitaux publics — Cour des comptes"));
    assert!(!csv.contains("masquee"));
}

#[sqlx::test(migrator = "crate::db::MIGRATOR")]
async fn an_empty_site_exports_empty_files(pool: PgPool) {
    let app = TestApp::new(pool.clone(), false, &[]).await;
    run(&app).await;
    let files = latest(&pool).await;
    let document: Value = serde_json::from_slice(&files["json"]).unwrap();
    assert_eq!(document["ideas"], json!([]));
    let csv = String::from_utf8(files["csv"].clone()).unwrap();
    assert_eq!(csv, format!("\u{FEFF}{}\r\n", CSV_COLUMNS.join(",")));
    let (count, ideas): (i64, i32) =
        sqlx::query_as("SELECT count(*), max(idea_count) FROM exports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((count, ideas), (4, 0));
}

#[test]
fn the_handler_runs_every_night() {
    assert_eq!(ExportsHandler.schedule(), Some(SCHEDULE));
    assert_eq!(SCHEDULE.kind, JobKind::GenerateExports);
    // Changes regenerate the exports through the same kind, debounced.
    assert_eq!(pipeline::exports_job().kind, JobKind::GenerateExports);
}
