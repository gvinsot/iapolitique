//! What the public portal reads (AGENTS.md §6.3, §13.2): the ranking of the published ideas
//! with its filters and search, and the page of one idea.
//!
//! Only `published` ideas are listed or shown. The SQL is made of compile-time constants
//! (§14.1): the sort is a closed enum mapped to a constant `ORDER BY`, the optional filters are
//! bound parameters (`$n IS NULL OR …`), and the search text only ever reaches
//! `websearch_to_tsquery` or `word_similarity` as a parameter.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::text_enum;

text_enum! {
    /// Order of the ranking (§6.3): composite score by default.
    pub enum IdeaSort {
        /// Composite score, highest first.
        Score => "score",
        /// Median annual gain, highest first.
        Gain => "gain",
        /// Feasibility, highest first.
        Feasibility => "feasibility",
        /// Delay before the first effects, shortest first.
        Delay => "delay",
        /// Most recently published first.
        Recent => "recent",
    }
}

/// Minimal `word_similarity` of the trigram fallback of the search (pg_trgm, 0 to 1).
pub const TRIGRAM_THRESHOLD: f64 = 0.3;

/// Filters of the ranking, already validated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdeaFilter {
    pub admin_level: Option<String>,
    pub category: Option<String>,
    pub kind: Option<String>,
    /// Territory name (substring, accents and case ignored) or INSEE code (exact).
    pub territory: Option<String>,
    /// Full text search on the neutral title and summary.
    pub q: Option<String>,
}

/// How the search text is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// French full text search, accents ignored (`french_unaccent`).
    FullText,
    /// Trigram similarity (typos, partial words), used when the full text search finds nothing.
    Trigram,
}

/// Shared `FROM … WHERE` of the ranking. Parameters: `$1` admin level, `$2` category, `$3`
/// kind, `$4` territory, `$5` search text, `$6` trigram mode, `$7` trigram threshold. The
/// territory name is matched with `strpos`, which has no metacharacter: a `LIKE` pattern would
/// need escaping after `unaccent()`, which turns `％` or `＿` into `%` or `_` (INJ-5).
macro_rules! listing_from {
    () => {
        "FROM ideas i JOIN analyses a ON a.id = i.current_analysis_id
         WHERE i.status = 'published'
           AND ($1::text IS NULL OR i.admin_level = $1)
           AND ($2::text IS NULL OR i.category = $2)
           AND ($3::text IS NULL OR i.kind = $3)
           AND ($4::text IS NULL
                OR i.territory_code = upper($4)
                OR strpos(unaccent(lower(coalesce(i.territory_name, ''))),
                          unaccent(lower($4))) > 0)
           AND ($5::text IS NULL
                OR (NOT $6 AND i.search_vector @@ websearch_to_tsquery('french_unaccent', $5))
                OR ($6 AND word_similarity(unaccent(lower($5)),
                                           unaccent(lower(i.title || ' ' || i.summary))) >= $7))"
    };
}

macro_rules! listing_select {
    ($order:literal) => {
        concat!(
            "SELECT i.slug, a.title, a.summary, a.admin_level, a.territory_name,
                    a.territory_code, a.category, a.kind, a.annual_gain_low_eur,
                    a.annual_gain_median_eur, a.annual_gain_high_eur, a.feasibility,
                    a.time_to_effect_months, a.confidence, i.score_eur, i.proposal_count,
                    i.contribution_count,
                    date_trunc('day', a.created_at AT TIME ZONE 'Europe/Paris')
                        AT TIME ZONE 'Europe/Paris' AS updated_at ",
            listing_from!(),
            " ORDER BY ",
            $order,
            " LIMIT $8 OFFSET $9"
        )
    };
}

const COUNT_SQL: &str = concat!("SELECT count(*) ", listing_from!());

impl IdeaSort {
    /// The constant query of this order.
    const fn sql(self) -> &'static str {
        match self {
            Self::Score => listing_select!("i.score_eur DESC, i.id"),
            Self::Gain => listing_select!("i.annual_gain_median_eur DESC, i.score_eur DESC, i.id"),
            Self::Feasibility => listing_select!("i.feasibility DESC, i.score_eur DESC, i.id"),
            Self::Delay => listing_select!("i.time_to_effect_months, i.score_eur DESC, i.id"),
            Self::Recent => listing_select!("i.published_at DESC, i.id"),
        }
    }
}

/// An idea of the ranking.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ListedIdea {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub admin_level: String,
    pub territory_name: Option<String>,
    pub territory_code: Option<String>,
    pub category: String,
    pub kind: String,
    pub annual_gain_low_eur: i64,
    pub annual_gain_median_eur: i64,
    pub annual_gain_high_eur: i64,
    pub feasibility: i16,
    pub time_to_effect_months: i32,
    pub confidence: String,
    pub score_eur: i64,
    pub proposal_count: i32,
    pub contribution_count: i32,
    /// Date of the current analysis.
    pub updated_at: DateTime<Utc>,
}

/// A page of the ranking and the number of matching ideas.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub ideas: Vec<ListedIdea>,
    pub total: i64,
    /// How the search text was matched (`None` without search).
    pub search: Option<SearchMode>,
}

fn bind_filter<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    filter: &'q IdeaFilter,
    mode: SearchMode,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    query
        .bind(filter.admin_level.as_deref())
        .bind(filter.category.as_deref())
        .bind(filter.kind.as_deref())
        .bind(filter.territory.as_deref())
        .bind(filter.q.as_deref())
        .bind(mode == SearchMode::Trigram)
        .bind(TRIGRAM_THRESHOLD)
}

async fn count(pool: &PgPool, filter: &IdeaFilter, mode: SearchMode) -> sqlx::Result<i64> {
    let (total,): (i64,) = bind_filter(sqlx::query_as(COUNT_SQL), filter, mode)
        .fetch_one(pool)
        .await?;
    Ok(total)
}

/// A page of the ranking. The search uses the French full text search first and falls back to
/// trigram similarity when it finds nothing (misspelt or partial words).
pub async fn list(
    pool: &PgPool,
    filter: &IdeaFilter,
    sort: IdeaSort,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Listing> {
    let mut mode = SearchMode::FullText;
    let mut total = count(pool, filter, mode).await?;
    if total == 0 && filter.q.is_some() {
        mode = SearchMode::Trigram;
        total = count(pool, filter, mode).await?;
    }
    let ideas = if total == 0 || offset >= total {
        Vec::new()
    } else {
        bind_filter(sqlx::query_as(sort.sql()), filter, mode)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await?
    };
    Ok(Listing {
        ideas,
        total,
        search: filter.q.as_ref().map(|_| mode),
    })
}

/// The idea behind a slug.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct IdeaPage {
    pub id: Uuid,
    pub status: String,
    pub slug: String,
    /// Masked original text.
    pub original_text: Option<String>,
    pub proposal_count: i32,
    pub contribution_count: i32,
    pub published_on: Option<NaiveDate>,
    pub current_analysis_id: Option<Uuid>,
    /// Slug of the published idea a merged duplicate points to.
    pub parent_slug: Option<String>,
}

/// The idea of `slug` (whatever its status), `None` when no idea has this slug.
pub async fn idea_page(pool: &PgPool, slug: &str) -> sqlx::Result<Option<IdeaPage>> {
    sqlx::query_as(
        "SELECT i.id, i.status, i.slug, i.original_text, i.proposal_count, i.contribution_count,
                (coalesce(i.first_published_at, i.published_at)
                    AT TIME ZONE 'Europe/Paris')::date AS published_on,
                i.current_analysis_id,
                CASE WHEN p.status = 'published' THEN p.slug END AS parent_slug
         FROM ideas i LEFT JOIN ideas p ON p.id = i.parent_idea_id
         WHERE i.slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// An analysis of an idea (`analyses` row).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct AnalysisRow {
    pub version: i32,
    pub content: Value,
    pub title: String,
    pub summary: String,
    pub kind: String,
    pub category: String,
    pub admin_level: String,
    pub territory_name: Option<String>,
    pub territory_code: Option<String>,
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
    pub model: String,
    pub prompt_version: String,
    pub research_prompt_version: Option<String>,
    pub schema_version: String,
    pub scoring_version: String,
    pub web_search_count: i32,
    pub created_at: DateTime<Utc>,
}

/// The analysis of an idea page. Its date is truncated to midnight Europe/Paris, like the
/// `updated_at` of the ranking: the time of an analysis follows its submission by a fixed
/// pipeline delay, so a finer timestamp would date the submission (PRIV-5, §7.4).
pub async fn analysis(pool: &PgPool, analysis_id: Uuid) -> sqlx::Result<Option<AnalysisRow>> {
    sqlx::query_as(
        "SELECT version, content, title, summary, kind, category, admin_level, territory_name,
                territory_code, legal_vehicle, annual_gain_low_eur, annual_gain_median_eur,
                annual_gain_high_eur, implementation_cost_low_eur,
                implementation_cost_median_eur, implementation_cost_high_eur,
                time_to_effect_months, feasibility, confidence, score_eur, net_annual_eur,
                feasibility_factor, confidence_factor, delay_factor, model, prompt_version,
                research_prompt_version, schema_version, scoring_version, web_search_count,
                date_trunc('day', created_at AT TIME ZONE 'Europe/Paris')
                    AT TIME ZONE 'Europe/Paris' AS created_at
         FROM analyses WHERE id = $1",
    )
    .bind(analysis_id)
    .fetch_optional(pool)
    .await
}

/// Contributions shown on an idea page.
pub const MAX_SHOWN_CONTRIBUTIONS: i64 = 500;

/// A contribution integrated by an analysis: its neutral summary and the answer of the
/// analysis, never its text (§5.5).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct IntegratedContribution {
    pub kind: String,
    pub summary: String,
    pub taken_into_account: bool,
    pub response: String,
    pub integrated_on: Option<NaiveDate>,
}

/// Integrated improvements and objections of an idea (merged duplicates are only counted), in
/// order of integration. `taken_into_account` and `response` come from the analysis that
/// integrated each one.
pub async fn integrated_contributions(
    pool: &PgPool,
    idea_id: Uuid,
) -> sqlx::Result<Vec<IntegratedContribution>> {
    sqlx::query_as(
        "SELECT c.kind, c.neutral_summary AS summary,
                coalesce((x.considered->>'taken_into_account')::boolean, true)
                    AS taken_into_account,
                coalesce(x.considered->>'response', '') AS response,
                (c.integrated_at AT TIME ZONE 'Europe/Paris')::date AS integrated_on
         FROM contributions c
         LEFT JOIN analyses a ON a.id = c.integrated_in_analysis_id
         LEFT JOIN LATERAL (
             SELECT e AS considered
             FROM jsonb_array_elements(
                 CASE WHEN jsonb_typeof(a.content->'contributions_considered') = 'array'
                      THEN a.content->'contributions_considered' ELSE '[]'::jsonb END) e
             WHERE e->>'contribution_id' = c.id::text
             LIMIT 1
         ) x ON true
         WHERE c.idea_id = $1 AND c.status = 'integrated' AND c.kind <> 'doublon'
           AND c.neutral_summary IS NOT NULL
         ORDER BY c.integrated_at, c.id
         LIMIT $2",
    )
    .bind(idea_id)
    .bind(MAX_SHOWN_CONTRIBUTIONS)
    .fetch_all(pool)
    .await
}

/// A right of reply published under an idea (§7.5).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PublishedReply {
    pub author_label: String,
    pub body: String,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn replies(pool: &PgPool, idea_id: Uuid) -> sqlx::Result<Vec<PublishedReply>> {
    sqlx::query_as(
        "SELECT author_label, body, published_at, updated_at FROM rights_of_reply
         WHERE idea_id = $1 ORDER BY published_at, id LIMIT $2",
    )
    .bind(idea_id)
    .bind(crate::db::replies::MAX_REPLIES)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sort_has_a_constant_query() {
        for sort in IdeaSort::ALL {
            let sql = sort.sql();
            assert!(sql.contains("ORDER BY"));
            assert!(sql.contains("LIMIT $8 OFFSET $9"));
            assert!(!sql.contains("LIKE"));
            assert!(sql.contains("i.status = 'published'"));
        }
        assert!(COUNT_SQL.starts_with("SELECT count(*) FROM ideas"));
    }
}
