//! Open data exports (AGENTS.md §7.4): what they read (published ideas only, day-precision
//! dates, no internal identifier) and where they are stored (`exports`, served from the
//! database by the api).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::types::ExportFormat;

/// A published idea with its current analysis, as exported.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ExportedIdea {
    /// Internal: groups the contributions, never exported.
    pub id: Uuid,
    pub slug: String,
    /// Masked original text.
    pub original_text: String,
    /// The publisher corrected the text before accepting the idea (§8).
    pub text_edited: bool,
    pub proposal_count: i32,
    pub contribution_count: i32,
    pub published_on: NaiveDate,
    pub updated_on: NaiveDate,
    pub analysis_version: i32,
    pub analysis_on: NaiveDate,
    pub content: Value,
    pub model: String,
    pub prompt_version: String,
    pub research_prompt_version: Option<String>,
    pub schema_version: String,
    pub scoring_version: String,
    pub title: String,
    pub summary: String,
    pub admin_level: String,
    pub territory_name: Option<String>,
    pub territory_code: Option<String>,
    pub category: String,
    pub kind: String,
    pub legal_vehicle: String,
    pub annual_gain_low_eur: i64,
    pub annual_gain_median_eur: i64,
    pub annual_gain_high_eur: i64,
    pub implementation_cost_low_eur: i64,
    pub implementation_cost_median_eur: i64,
    pub implementation_cost_high_eur: i64,
    pub time_to_effect_months: i32,
    pub feasibility: i16,
    pub confidence: String,
    pub score_eur: i64,
    pub net_annual_eur: i64,
    pub feasibility_factor: f64,
    pub confidence_factor: f64,
    pub delay_factor: f64,
}

/// Published ideas, by score (the default ranking of the portal).
pub async fn published_ideas(pool: &PgPool) -> sqlx::Result<Vec<ExportedIdea>> {
    sqlx::query_as(
        "SELECT i.id, i.slug, i.original_text, i.text_edited_at IS NOT NULL AS text_edited,
                i.proposal_count, i.contribution_count,
                (coalesce(i.first_published_at, i.published_at)
                    AT TIME ZONE 'Europe/Paris')::date AS published_on,
                (coalesce(i.last_analyzed_at, a.created_at)
                    AT TIME ZONE 'Europe/Paris')::date AS updated_on,
                a.version AS analysis_version,
                (a.created_at AT TIME ZONE 'Europe/Paris')::date AS analysis_on,
                a.content, a.model, a.prompt_version, a.research_prompt_version,
                a.schema_version, a.scoring_version, a.title, a.summary, a.admin_level,
                a.territory_name, a.territory_code, a.category, a.kind, a.legal_vehicle,
                a.annual_gain_low_eur, a.annual_gain_median_eur, a.annual_gain_high_eur,
                a.implementation_cost_low_eur, a.implementation_cost_median_eur,
                a.implementation_cost_high_eur, a.time_to_effect_months, a.feasibility,
                a.confidence, a.score_eur, a.net_annual_eur, a.feasibility_factor,
                a.confidence_factor, a.delay_factor
         FROM ideas i JOIN analyses a ON a.id = i.current_analysis_id
         WHERE i.status = 'published' AND i.slug IS NOT NULL AND i.original_text IS NOT NULL
         ORDER BY i.score_eur DESC, i.slug",
    )
    .fetch_all(pool)
    .await
}

/// Neutral summaries of the contributions integrated into published ideas (kind, summary), by
/// idea, in order of integration. Merged duplicates are counted, not summarized.
pub async fn integrated_contributions(
    pool: &PgPool,
) -> sqlx::Result<BTreeMap<Uuid, Vec<(String, String)>>> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT c.idea_id, c.kind, c.neutral_summary
         FROM contributions c JOIN ideas i ON i.id = c.idea_id AND i.status = 'published'
         WHERE c.status = 'integrated' AND c.kind <> 'doublon' AND c.neutral_summary IS NOT NULL
         ORDER BY c.idea_id, c.integrated_at, c.id",
    )
    .fetch_all(pool)
    .await?;
    let mut by_idea: BTreeMap<Uuid, Vec<(String, String)>> = BTreeMap::new();
    for (idea_id, kind, summary) in rows {
        by_idea.entry(idea_id).or_default().push((kind, summary));
    }
    Ok(by_idea)
}

/// Today in Europe/Paris, from the database clock.
pub async fn today(pool: &PgPool) -> sqlx::Result<NaiveDate> {
    sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Paris')::date")
        .fetch_one(pool)
        .await
}

/// One file of a generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    pub format: ExportFormat,
    pub content: Vec<u8>,
}

/// Stores the files of one generation in the caller's transaction; returns the generation id.
pub async fn insert(
    conn: &mut PgConnection,
    files: &[ExportFile],
    idea_count: usize,
    scoring_version: &str,
    export_schema_version: &str,
) -> sqlx::Result<Uuid> {
    let generation_id = Uuid::new_v4();
    let idea_count = i32::try_from(idea_count).unwrap_or(i32::MAX);
    for file in files {
        let size = i64::try_from(file.content.len()).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO exports (format, content, content_sha256, size_bytes, idea_count,
                                  scoring_version, generation_id, export_schema_version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(file.format.as_str())
        .bind(&file.content)
        .bind(Sha256::digest(&file.content).to_vec())
        .bind(size)
        .bind(idea_count)
        .bind(scoring_version)
        .bind(generation_id)
        .bind(export_schema_version)
        .execute(&mut *conn)
        .await?;
    }
    Ok(generation_id)
}

/// Locks the exported ideas (`FOR SHARE`, in id order) until the end of the caller's
/// transaction and tells whether they are all still published. A masking or a withdrawal
/// committed since they were read makes the generation stale (the caller reads them again); one
/// that comes later waits for the generation to be stored, then regenerates the exports itself
/// (`pipeline::regenerate_exports_now`). So no stored generation holds an idea that was no longer
/// published when it was stored (§7.4).
pub async fn still_published(conn: &mut PgConnection, ids: &[Uuid]) -> sqlx::Result<bool> {
    let published: Vec<bool> = sqlx::query_scalar(
        "SELECT status = 'published' FROM ideas WHERE id = ANY($1) ORDER BY id FOR SHARE",
    )
    .bind(ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(published.len() == ids.len() && published.into_iter().all(|p| p))
}

/// The latest generated file of a format, as served by the api, without its content: a
/// conditional request is answered from it, and the content is read only when the api does not
/// hold it already (EXP-1).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredExport {
    pub id: i64,
    pub content_sha256: Vec<u8>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// Day of the generation (Europe/Paris), used in the download name.
    pub generated_on: NaiveDate,
}

/// The file of the latest generation of `format`, `None` before the first generation.
pub async fn latest(pool: &PgPool, format: ExportFormat) -> sqlx::Result<Option<StoredExport>> {
    sqlx::query_as(
        "SELECT id, content_sha256, generated_at,
                (generated_at AT TIME ZONE 'Europe/Paris')::date AS generated_on
         FROM exports WHERE format = $1
         ORDER BY generated_at DESC, id DESC
         LIMIT 1",
    )
    .bind(format.as_str())
    .fetch_optional(pool)
    .await
}

/// Content of the export file `id` (`None` if it was removed meanwhile).
pub async fn content(pool: &PgPool, id: i64) -> sqlx::Result<Option<Vec<u8>>> {
    sqlx::query_scalar("SELECT content FROM exports WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
