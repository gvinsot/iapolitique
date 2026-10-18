//! Open data exports (AGENTS.md §7.4), job `generate_exports`: every night, and 2 minutes after
//! a publication, a masking or a deletion (debounced, [`pipeline::exports_job`]).
//!
//! One generation stores four files in `exports` (served from the database by the api):
//!
//! - `ideas.json`: `metadata`, the licence text and the published ideas;
//! - `ideas.csv`: one row per published idea (UTF-8 with BOM, `,`, RFC 4180 quoting, formula
//!   cells prefixed by `'`, §14.1);
//! - `LICENSE`: the Licence Ouverte 2.0 (official Etalab text, `backend/assets`);
//! - `metadata.json`: licence, producer, generation day, schema version, notices (AI,
//!   third-party content, masked personal data, irrevocability, resynchronisation).
//!
//! Per idea: the masked original text (`source_type = user`, anonymous attribution), the
//! AI-generated analysis marked as such (`ai_generated`, `ai_provider`, `ai_model`,
//! `generated_at`, `analysis_version`, `human_reviewed = false`), the score and its components,
//! the neutral summaries of the integrated contributions, the canonical URL and day-precision
//! dates. Sources keep their title, publisher, URL and the day they were consulted, not the
//! paraphrase. Never exported: ideas that are not published, internal identifiers, tokens,
//! screenings, reports, contacts, fine timestamps.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::schedule::DailySchedule;
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobKind, JobOutcome};
use crate::db::exports::{self, ExportFile, ExportedIdea};
use crate::domain::scoring::SCORING_VERSION;
use crate::domain::types::ExportFormat;

/// Version of the layout of the export files; bump it when a field changes.
pub const EXPORT_SCHEMA_VERSION: &str = "1.0";

/// Nightly regeneration, Europe/Paris time (after the 03:30 purge).
pub const SCHEDULE: DailySchedule =
    DailySchedule::new("nightly_exports", JobKind::GenerateExports, 4, 15);

/// Official text of the Licence Ouverte 2.0, fetched once from
/// <https://github.com/etalab/licence-ouverte/blob/master/LO.md> (commit `05cd5ed`).
pub const LICENCE_TEXT: &str = include_str!("../../assets/licence-ouverte-2.0.md");

/// Host of the canonical URLs when `TRAEFIK_HOST` is not set.
pub const DEFAULT_PUBLIC_HOST: &str = "iapolitique.fr";

pub const ATTRIBUTION: &str = "Contribution anonyme – iapolitique.fr";
pub const AI_PROVIDER: &str = "Mistral AI";
pub const AI_NOTICE: &str = "Analyse générée automatiquement par intelligence artificielle \
(Mistral AI), sans relecture humaine. Elle peut contenir des erreurs ; vérifiez les sources \
citées. Ce n'est pas une position officielle.";

/// Fields of the analysis content (§6.2) copied into the export, in this order; the others
/// (identifiers of contributions…) are left out.
const ANALYSIS_FIELDS: &[&str] = &[
    "title",
    "summary",
    "improved_proposal",
    "existing_situation",
    "admin_level",
    "territory",
    "category",
    "kind",
    "annual_gain_eur",
    "implementation_cost_eur",
    "time_to_effect_months",
    "legal_vehicle",
    "feasibility",
    "confidence",
    "confidence_explanation",
    "assumptions",
    "pros",
    "cons",
    "risks",
    "impacts",
    "changes_since_previous",
];

/// Canonical URL of an idea on the portal.
pub fn canonical_url(host: &str, slug: &str) -> String {
    format!("https://{host}/idees/{slug}")
}

fn day(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Sources of an analysis: title, publisher, URL and the day of the analysis (no paraphrase).
fn sources(content: &Value, consulted_on: NaiveDate) -> Vec<Value> {
    content
        .get("sources")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|source| {
            let text = |key: &str| source.get(key).cloned().unwrap_or(Value::Null);
            json!({
                "id": text("id"),
                "title": text("title"),
                "publisher": text("publisher"),
                "url": text("url"),
                "consulted_on": day(consulted_on),
            })
        })
        .collect()
}

/// The exported object of an idea.
fn idea_json(host: &str, idea: &ExportedIdea, contributions: &[(String, String)]) -> Value {
    let mut analysis = Map::new();
    analysis.insert("ai_generated".into(), json!(true));
    analysis.insert("ai_provider".into(), json!(AI_PROVIDER));
    analysis.insert("ai_model".into(), json!(idea.model));
    analysis.insert("generated_at".into(), json!(day(idea.analysis_on)));
    analysis.insert("analysis_version".into(), json!(idea.analysis_version));
    analysis.insert("human_reviewed".into(), json!(false));
    analysis.insert("prompt_version".into(), json!(idea.prompt_version));
    analysis.insert(
        "research_prompt_version".into(),
        json!(idea.research_prompt_version),
    );
    analysis.insert("schema_version".into(), json!(idea.schema_version));
    for field in ANALYSIS_FIELDS {
        if let Some(value) = idea.content.get(*field) {
            analysis.insert((*field).to_owned(), value.clone());
        }
    }
    analysis.insert(
        "sources".into(),
        Value::Array(sources(&idea.content, idea.analysis_on)),
    );
    json!({
        "slug": idea.slug,
        "url": canonical_url(host, &idea.slug),
        "date_publication": day(idea.published_on),
        "date_derniere_mise_a_jour": day(idea.updated_on),
        "proposal_count": idea.proposal_count,
        "contribution_count": idea.contribution_count,
        "original": {
            "text": idea.original_text,
            "source_type": "user",
            "attribution": ATTRIBUTION,
            "personal_data_masked": true,
        },
        "analysis": analysis,
        "score": {
            "label": "score calculé par algorithme",
            "scoring_version": idea.scoring_version,
            "score_eur": idea.score_eur,
            "net_annual_eur": idea.net_annual_eur,
            "feasibility_factor": idea.feasibility_factor,
            "confidence_factor": idea.confidence_factor,
            "delay_factor": idea.delay_factor,
        },
        "contributions": contributions.iter().map(|(kind, summary)| json!({
            "kind": kind,
            "summary": summary,
            "ai_generated": true,
        })).collect::<Vec<_>>(),
    })
}

/// `metadata.json` (also embedded in `ideas.json`).
pub fn metadata(generated_on: NaiveDate, idea_count: usize) -> Value {
    let date = day(generated_on);
    json!({
        "title": "Propositions citoyennes et analyses générées par IA – iapolitique.fr",
        "producer": "iapolitique.fr",
        "generated_at": date,
        "export_schema_version": EXPORT_SCHEMA_VERSION,
        "scoring_version": SCORING_VERSION,
        "idea_count": idea_count,
        "licence": {
            "name": "Licence Ouverte / Open Licence version 2.0",
            "spdx": "etalab-2.0",
            "url": "https://www.etalab.gouv.fr/licence-ouverte-open-licence/",
            "compatibility": "Compatible avec la licence Creative Commons Attribution (CC BY 4.0).",
            "scope": "Dans la mesure où des droits existent sur ces données.",
            "licensor": "L'éditeur d'iapolitique.fr, à qui les contributeurs concèdent leurs droits par les conditions d'utilisation (/regles).",
            "file": "LICENSE",
        },
        "attribution": format!(
            "Source : iapolitique.fr – Propositions citoyennes et analyses générées par IA \
             (Mistral AI), mise à jour du {date} – Licence Ouverte 2.0"
        ),
        "ai_notice": AI_NOTICE,
        "score_notice": "Le score est calculé par algorithme (formule publique, page Méthodologie) : ce n'est ni un sondage ni une mesure d'opinion.",
        "unofficial_notice": "Site citoyen indépendant et non officiel, sans lien avec l'État ni aucune administration.",
        "third_party_content": "Les contenus de tiers cités (sources) sont exclus de la licence et restent soumis aux droits de leurs auteurs ; seuls leur titre, leur éditeur et leur adresse sont donnés.",
        "personal_data": "Données personnelles masquées : les textes originaux sont publiés après masquage automatique des données personnelles ([donnée masquée]).",
        "irrevocability": "Une copie déjà téléchargée ne peut pas être retirée : une idée supprimée par son auteur ou masquée disparaît seulement des exports suivants.",
        "resync": "Resynchronisez régulièrement vos copies avec le dernier export, régénéré chaque nuit et après toute publication, tout masquage ou toute suppression.",
        "exclusions": "Ne sont pas exportés : les idées refusées, masquées ou en attente, les jetons de suivi, les journaux de filtrage, les signalements et toute coordonnée. Les dates sont données au jour près.",
        "files": {
            "ideas.json": "Métadonnées, texte de la licence et idées publiées avec leur analyse (JSON, UTF-8).",
            "ideas.csv": "Une ligne par idée publiée (CSV, UTF-8 avec BOM).",
            "LICENSE": "Texte de la Licence Ouverte 2.0.",
            "metadata.json": "Ces métadonnées.",
        },
        "csv": {
            "encoding": "UTF-8 avec BOM",
            "separator": ",",
            "quoting": "RFC 4180 (guillemets doublés, fins de ligne CRLF)",
            "lists": "Les listes sont jointes par « | ».",
            "formula_protection": "Une cellule commençant par =, +, -, @, une tabulation ou un retour chariot est préfixée par une apostrophe (').",
        },
    })
}

/// `ideas.json`.
pub fn ideas_json(meta: &Value, ideas: &[Value]) -> Vec<u8> {
    let document = json!({
        "metadata": meta,
        "licence_text": LICENCE_TEXT,
        "ideas": ideas,
    });
    serde_json::to_vec_pretty(&document).expect("a JSON value serializes")
}

/// A CSV cell: formula characters neutralized (§14.1), then RFC 4180 quoting when needed.
pub fn csv_cell(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

const CSV_COLUMNS: &[&str] = &[
    "slug",
    "url",
    "date_publication",
    "date_derniere_mise_a_jour",
    "original_text",
    "source_type",
    "attribution",
    "title",
    "summary",
    "improved_proposal",
    "admin_level",
    "territory_name",
    "territory_insee_code",
    "category",
    "kind",
    "annual_gain_low_eur",
    "annual_gain_median_eur",
    "annual_gain_high_eur",
    "implementation_cost_low_eur",
    "implementation_cost_median_eur",
    "implementation_cost_high_eur",
    "time_to_effect_months",
    "legal_vehicle",
    "feasibility",
    "confidence",
    "score_eur",
    "net_annual_eur",
    "feasibility_factor",
    "confidence_factor",
    "delay_factor",
    "scoring_version",
    "proposal_count",
    "contribution_count",
    "ai_generated",
    "ai_provider",
    "ai_model",
    "generated_at",
    "analysis_version",
    "human_reviewed",
    "pros",
    "cons",
    "risks",
    "sources",
];

/// Strings of a list of the analysis content, joined by ` | `.
fn joined(content: &Value, field: &str) -> String {
    content
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default()
}

fn content_text(content: &Value, field: &str) -> String {
    content
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `ideas.csv`.
pub fn ideas_csv(host: &str, ideas: &[ExportedIdea]) -> Vec<u8> {
    let mut out = String::from("\u{FEFF}");
    out.push_str(&CSV_COLUMNS.join(","));
    out.push_str("\r\n");
    for idea in ideas {
        let sources = sources(&idea.content, idea.analysis_on)
            .iter()
            .map(|s| {
                let text = |key: &str| s[key].as_str().unwrap_or_default().to_owned();
                format!(
                    "{} : {} — {} — {}",
                    text("id"),
                    text("title"),
                    text("publisher"),
                    text("url")
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let cells: Vec<String> = vec![
            idea.slug.clone(),
            canonical_url(host, &idea.slug),
            day(idea.published_on),
            day(idea.updated_on),
            idea.original_text.clone(),
            "user".to_owned(),
            ATTRIBUTION.to_owned(),
            idea.title.clone(),
            idea.summary.clone(),
            content_text(&idea.content, "improved_proposal"),
            idea.admin_level.clone(),
            idea.territory_name.clone().unwrap_or_default(),
            idea.territory_code.clone().unwrap_or_default(),
            idea.category.clone(),
            idea.kind.clone(),
            idea.annual_gain_low_eur.to_string(),
            idea.annual_gain_median_eur.to_string(),
            idea.annual_gain_high_eur.to_string(),
            idea.implementation_cost_low_eur.to_string(),
            idea.implementation_cost_median_eur.to_string(),
            idea.implementation_cost_high_eur.to_string(),
            idea.time_to_effect_months.to_string(),
            idea.legal_vehicle.clone(),
            idea.feasibility.to_string(),
            idea.confidence.clone(),
            idea.score_eur.to_string(),
            idea.net_annual_eur.to_string(),
            idea.feasibility_factor.to_string(),
            idea.confidence_factor.to_string(),
            idea.delay_factor.to_string(),
            idea.scoring_version.clone(),
            idea.proposal_count.to_string(),
            idea.contribution_count.to_string(),
            "true".to_owned(),
            AI_PROVIDER.to_owned(),
            idea.model.clone(),
            day(idea.analysis_on),
            idea.analysis_version.to_string(),
            "false".to_owned(),
            joined(&idea.content, "pros"),
            joined(&idea.content, "cons"),
            joined(&idea.content, "risks"),
            sources,
        ];
        debug_assert_eq!(cells.len(), CSV_COLUMNS.len());
        out.push_str(
            &cells
                .iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// Builds the four files of a generation.
pub fn build(
    host: &str,
    generated_on: NaiveDate,
    ideas: &[ExportedIdea],
    contributions: &BTreeMap<Uuid, Vec<(String, String)>>,
) -> Vec<ExportFile> {
    let meta = metadata(generated_on, ideas.len());
    let objects: Vec<Value> = ideas
        .iter()
        .map(|idea| {
            let summaries = contributions
                .get(&idea.id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            idea_json(host, idea, summaries)
        })
        .collect();
    vec![
        ExportFile {
            format: ExportFormat::Json,
            content: ideas_json(&meta, &objects),
        },
        ExportFile {
            format: ExportFormat::Csv,
            content: ideas_csv(host, ideas),
        },
        ExportFile {
            format: ExportFormat::License,
            content: LICENCE_TEXT.as_bytes().to_vec(),
        },
        ExportFile {
            format: ExportFormat::Metadata,
            content: serde_json::to_vec_pretty(&meta).expect("a JSON value serializes"),
        },
    ]
}

/// Handler of [`JobKind::GenerateExports`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ExportsHandler;

impl JobHandler for ExportsHandler {
    fn kind(&self) -> JobKind {
        JobKind::GenerateExports
    }

    fn schedule(&self) -> Option<DailySchedule> {
        Some(SCHEDULE)
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, _job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let pool = &ctx.pool;
            let host = ctx
                .config
                .public_host
                .as_deref()
                .unwrap_or(DEFAULT_PUBLIC_HOST);
            let ideas = exports::published_ideas(pool).await?;
            let contributions = exports::integrated_contributions(pool).await?;
            let today = exports::today(pool).await?;
            let files = build(host, today, &ideas, &contributions);
            let mut tx = pool.begin().await?;
            let generation = exports::insert(
                &mut tx,
                &files,
                ideas.len(),
                SCORING_VERSION,
                EXPORT_SCHEMA_VERSION,
            )
            .await?;
            tx.commit().await?;
            tracing::info!(
                generation_id = %generation,
                ideas = ideas.len(),
                json_bytes = files[0].content.len(),
                csv_bytes = files[1].content.len(),
                "open data exports generated"
            );
            Ok(JobOutcome::Done)
        })
    }
}

#[cfg(test)]
mod tests;
