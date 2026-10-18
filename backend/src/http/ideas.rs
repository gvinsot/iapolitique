//! Public portal (AGENTS.md §6.2, §6.3, §7.5, §13.2): the ranking of the published ideas and
//! the page of one idea.
//!
//! Only published ideas are visible: a hidden, purged or unknown idea answers
//! `404 idea_not_found`. The answers carry no internal identifier and no raw contribution text
//! (only the neutral summaries written by the analysis, §5.5). Generated content is marked as
//! such (`ai`, AI Act art. 50, §7.5).

use std::collections::BTreeMap;
use std::str::FromStr;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::AppState;
use super::cache;
use super::error::{ApiError, ErrorBody};
use super::extract::{ApiPath, ApiQuery};
use super::pagination::PageRequest;
use crate::ai::prompts::Taxonomy;
use crate::db::portal::{self, AnalysisRow, IdeaFilter, IdeaSort, ListedIdea, SearchMode};
use crate::domain::slug::is_valid_slug;
use crate::domain::status::IdeaStatus;
use crate::domain::text_enum;
use crate::domain::types::{
    AdminLevel, Confidence, ContributionKind, IdeaKind, ImpactDirection, LegalVehicle,
};
use crate::domain::validation::TERRITORY_MAX_CHARS;
use crate::jobs::exports::{
    AI_NOTICE, AI_PROVIDER, ATTRIBUTION, DEFAULT_PUBLIC_HOST, canonical_url,
};

/// Longest search text, in characters.
pub const SEARCH_MAX_CHARS: usize = 200;
/// Label of the score: computed, never an opinion poll (§7.5).
pub const SCORE_LABEL: &str = "score calculé par algorithme";
/// Mention next to the original text of the author (§7.5).
pub const ORIGINAL_TEXT_NOTICE: &str =
    "Contribution d'un internaute (texte original, données personnelles masquées)";

fn idea_not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "idea_not_found",
        "Cette idée n'existe pas ou n'est plus publiée.",
    )
}

pub(crate) fn invalid_query(message: &'static str) -> ApiError {
    ApiError::bad_request("invalid_query", message)
}

/// A value read from the database that the code does not know (newer schema): logged without
/// content, answered as an internal error.
fn known<T: FromStr>(value: &str, what: &'static str) -> Result<T, ApiError> {
    value.parse().map_err(|_| {
        tracing::error!(field = what, "unknown value in the database");
        ApiError::internal()
    })
}

/// Host of the canonical URLs (`TRAEFIK_HOST`).
pub(crate) fn public_host(state: &AppState) -> &str {
    state
        .config
        .public_host
        .as_deref()
        .unwrap_or(DEFAULT_PUBLIC_HOST)
}

// ---------------------------------------------------------------------------------------------
// Shared shapes
// ---------------------------------------------------------------------------------------------

/// Territory targeted by an idea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TerritoryRef {
    #[schema(example = "Lyon")]
    pub name: String,
    /// INSEE code, empty when unknown.
    #[schema(example = "69123")]
    pub insee_code: String,
}

impl TerritoryRef {
    fn new(name: Option<String>, code: Option<String>) -> Option<Self> {
        let name = name.filter(|n| !n.trim().is_empty())?;
        Some(Self {
            name,
            insee_code: code.unwrap_or_default(),
        })
    }
}

/// A category of `prompts/taxonomy.yaml` with its French label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CategoryRef {
    #[schema(example = "achats_publics")]
    pub key: String,
    #[schema(example = "Achats publics")]
    pub label: String,
}

impl CategoryRef {
    fn new(taxonomy: &Taxonomy, key: String) -> Self {
        let label = taxonomy
            .category(&key)
            .map_or_else(|| key.clone(), |(_, entry)| entry.label.clone());
        Self { key, label }
    }
}

/// Low, median and high estimate, in whole euros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct EuroRange {
    pub low: i64,
    pub median: i64,
    pub high: i64,
}

// ---------------------------------------------------------------------------------------------
// GET /api/ideas
// ---------------------------------------------------------------------------------------------

/// Filters, search and pagination of the ranking.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct IdeaListQuery {
    /// Order of the ranking (`score` by default).
    #[serde(default)]
    pub sort: Option<IdeaSort>,
    #[serde(default)]
    pub admin_level: Option<AdminLevel>,
    /// Category key of the taxonomy (`GET /api/taxonomy`).
    #[serde(default)]
    #[param(example = "achats_publics")]
    pub category: Option<String>,
    #[serde(default)]
    pub kind: Option<IdeaKind>,
    /// Territory name (part of it, accents and case ignored) or INSEE code, 120 characters.
    #[serde(default)]
    #[param(example = "Lyon")]
    pub territory: Option<String>,
    /// Search in the neutral titles and summaries (French, accents ignored, approaching
    /// results when nothing matches exactly), 200 characters.
    #[serde(default)]
    pub q: Option<String>,
    /// Page number, from 1.
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    /// Ideas per page, 1 to 50 (20 by default).
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

/// Trimmed optional text, `None` when blank, refused beyond `max` characters.
pub(crate) fn optional_text(
    raw: Option<String>,
    max: usize,
    message: &'static str,
) -> Result<Option<String>, ApiError> {
    let Some(text) = raw else {
        return Ok(None);
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        Ok(None)
    } else if text.chars().count() > max || text.contains('\0') {
        Err(invalid_query(message))
    } else {
        Ok(Some(text))
    }
}

impl IdeaListQuery {
    fn filter(self, taxonomy: &Taxonomy) -> Result<(IdeaFilter, IdeaSort, PageRequest), ApiError> {
        let pages = PageRequest::parse(self.page, self.per_page)?;
        let category = match self.category.map(|c| c.trim().to_owned()) {
            Some(c) if c.is_empty() => None,
            Some(c) if taxonomy.category(&c).is_some() => Some(c),
            Some(_) => return Err(invalid_query("Catégorie inconnue.")),
            None => None,
        };
        let filter = IdeaFilter {
            admin_level: self.admin_level.map(|l| l.as_str().to_owned()),
            category,
            kind: self.kind.map(|k| k.as_str().to_owned()),
            territory: optional_text(
                self.territory,
                TERRITORY_MAX_CHARS,
                "Le territoire recherché doit contenir au plus 120 caractères.",
            )?,
            q: optional_text(
                self.q,
                SEARCH_MAX_CHARS,
                "La recherche doit contenir au plus 200 caractères.",
            )?,
        };
        Ok((filter, self.sort.unwrap_or(IdeaSort::Score), pages))
    }
}

text_enum! {
    /// How the search text matched.
    pub enum SearchMatch {
        /// Words of the search found in the titles and summaries.
        Exact => "exact",
        /// Nothing matched exactly: approaching results (similar spelling).
        Approximate => "approximate",
    }
}

/// An idea of the ranking (card).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct IdeaListItem {
    pub slug: String,
    /// Neutral title written by the analysis.
    pub title: String,
    pub summary: String,
    pub admin_level: AdminLevel,
    #[schema(required = true)]
    pub territory: Option<TerritoryRef>,
    pub category: CategoryRef,
    pub kind: IdeaKind,
    /// Gross annual gain once in full effect.
    pub annual_gain_eur: EuroRange,
    /// 1 to 5.
    pub feasibility: u8,
    pub time_to_effect_months: u32,
    pub confidence: Confidence,
    /// Composite score in euros per year (« score calculé par algorithme »).
    pub score: i64,
    /// How many times the idea was proposed (merged duplicates included).
    pub proposal_count: u32,
    /// Integrated improvements and objections.
    pub contribution_count: u32,
    /// Date of the current analysis.
    pub updated_at: DateTime<Utc>,
}

/// A page of the ranking.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct IdeaList {
    pub items: Vec<IdeaListItem>,
    pub page: u32,
    pub per_page: u32,
    /// Ideas matching the filters, all pages included.
    pub total: u64,
    pub total_pages: u32,
    /// How the search matched, `null` without search.
    #[schema(required = true)]
    pub search: Option<SearchMatch>,
}

fn count(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

fn list_item(taxonomy: &Taxonomy, idea: ListedIdea) -> Result<IdeaListItem, ApiError> {
    Ok(IdeaListItem {
        admin_level: known(&idea.admin_level, "admin_level")?,
        kind: known(&idea.kind, "kind")?,
        confidence: known(&idea.confidence, "confidence")?,
        territory: TerritoryRef::new(idea.territory_name, idea.territory_code),
        category: CategoryRef::new(taxonomy, idea.category),
        annual_gain_eur: EuroRange {
            low: idea.annual_gain_low_eur,
            median: idea.annual_gain_median_eur,
            high: idea.annual_gain_high_eur,
        },
        feasibility: u8::try_from(idea.feasibility).unwrap_or(0),
        time_to_effect_months: u32::try_from(idea.time_to_effect_months).unwrap_or(0),
        score: idea.score_eur,
        proposal_count: count(idea.proposal_count),
        contribution_count: count(idea.contribution_count),
        updated_at: idea.updated_at,
        slug: idea.slug,
        title: idea.title,
        summary: idea.summary,
    })
}

/// Ranking of the published ideas.
#[utoipa::path(
    get,
    path = "/api/ideas",
    tag = "portal",
    operation_id = "listIdeas",
    params(IdeaListQuery),
    responses(
        (status = 200, description = "Une page du classement.", body = IdeaList),
        (status = 400, description = "Paramètre invalide (`invalid_query`).", body = ErrorBody)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<IdeaListQuery>,
) -> Result<Response, ApiError> {
    let taxonomy = &state.prompts.taxonomy;
    let (filter, sort, pages) = query.filter(taxonomy)?;
    let listing = portal::list(&state.pool, &filter, sort, pages.limit(), pages.offset()).await?;
    let items = listing
        .ideas
        .into_iter()
        .map(|idea| list_item(taxonomy, idea))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cache::json(
        IdeaList {
            items,
            page: pages.page,
            per_page: pages.per_page,
            total: u64::try_from(listing.total).unwrap_or(0),
            total_pages: pages.page_count(listing.total),
            search: listing.search.map(|mode| match mode {
                SearchMode::FullText => SearchMatch::Exact,
                SearchMode::Trigram => SearchMatch::Approximate,
            }),
        },
        cache::SHORT,
    ))
}

// ---------------------------------------------------------------------------------------------
// GET /api/ideas/{slug}
// ---------------------------------------------------------------------------------------------

/// Masked original text of the author, shown apart from the generated content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct OriginalText {
    pub text: String,
    /// Personal data was masked automatically (`[donnée masquée]`).
    pub personal_data_masked: bool,
    #[schema(example = "Contribution anonyme – iapolitique.fr")]
    pub attribution: String,
    /// Heading shown above the text.
    pub notice: String,
}

/// An estimate with its detailed calculation (citing the sources `[Sx]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AmountEstimate {
    pub low: i64,
    pub median: i64,
    pub high: i64,
    pub calculation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct FeasibilityAssessment {
    /// 1 to 5.
    pub score: u8,
    pub obstacles: Vec<String>,
}

/// Impact of the idea on one public of the taxonomy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ImpactAssessment {
    /// Key of `impact_publics` in the taxonomy.
    pub public: String,
    /// French label of the public.
    pub label: String,
    pub effect: ImpactDirection,
    pub explanation: String,
}

/// A source cited by the analysis. Its URL comes from the AI: render it as a link only when
/// its scheme is `https:` or `http:` (§14.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct SourceReference {
    /// `S1`, `S2`… as cited in the texts.
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub used_for: String,
}

/// Generated analysis of the idea (§6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct IdeaAnalysis {
    pub improved_proposal: String,
    pub existing_situation: String,
    pub legal_vehicle: LegalVehicle,
    pub annual_gain_eur: AmountEstimate,
    pub implementation_cost_eur: AmountEstimate,
    pub time_to_effect_months: u32,
    pub feasibility: FeasibilityAssessment,
    pub confidence: Confidence,
    pub confidence_explanation: String,
    pub assumptions: Vec<String>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub risks: Vec<String>,
    /// One entry per public of the taxonomy, in its order.
    pub impacts: Vec<ImpactAssessment>,
    pub sources: Vec<SourceReference>,
    /// What changed since the previous analysis (empty for the first one).
    pub changes_since_previous: String,
}

/// Components of the composite score (§6.3), « score calculé par algorithme ».
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ScoreComponents {
    #[schema(example = "score calculé par algorithme")]
    pub label: String,
    pub scoring_version: String,
    pub net_annual_eur: i64,
    pub feasibility_factor: f64,
    pub confidence_factor: f64,
    pub delay_factor: f64,
    pub score: i64,
}

/// Machine-readable marking of the generated content (AI Act art. 50, §7.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AiMarking {
    /// Always `true`: title, summary, analysis and contribution summaries are generated.
    pub ai_generated: bool,
    #[schema(example = "Mistral AI")]
    pub provider: String,
    pub model: String,
    pub generated_at: DateTime<Utc>,
    /// Version of the analysis of this idea (1 for the first one).
    pub analysis_version: u32,
    /// Version of `prompts/analysis.md`.
    pub prompt_version: String,
    /// Version of `prompts/analysis_research.md`.
    #[schema(required = true)]
    pub research_prompt_version: Option<String>,
    /// Fingerprint of `prompts/analysis.schema.json`.
    pub schema_version: String,
    pub scoring_version: String,
    pub web_search_count: u32,
    /// Always `false`: no systematic human review.
    pub human_reviewed: bool,
    /// Label shown before every generated block.
    pub notice: String,
}

/// A contribution taken into account by an analysis: neutral summary, never the raw text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ContributionSummary {
    pub kind: ContributionKind,
    pub summary: String,
    /// Whether the analysis changed because of it.
    pub taken_into_account: bool,
    /// Answer of the analysis to the contribution.
    pub response: String,
    #[schema(required = true)]
    pub integrated_on: Option<NaiveDate>,
}

/// Right of reply published by the editor under the idea (LCEN art. 1-1 III). Plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RightOfReply {
    /// How the person replying is designated, as they asked.
    pub author_label: String,
    pub body: String,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Page of a published idea.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct IdeaDetail {
    pub slug: String,
    /// Neutral title written by the analysis.
    pub title: String,
    pub summary: String,
    pub admin_level: AdminLevel,
    #[schema(required = true)]
    pub territory: Option<TerritoryRef>,
    pub category: CategoryRef,
    pub kind: IdeaKind,
    pub original: OriginalText,
    pub analysis: IdeaAnalysis,
    pub score: ScoreComponents,
    pub contributions: Vec<ContributionSummary>,
    pub rights_of_reply: Vec<RightOfReply>,
    pub ai: AiMarking,
    /// How many times the idea was proposed (merged duplicates included).
    pub proposal_count: u32,
    pub contribution_count: u32,
    /// Day of the first publication (Europe/Paris).
    #[schema(required = true)]
    pub published_on: Option<NaiveDate>,
    /// Date of the current analysis.
    pub updated_at: DateTime<Utc>,
    /// `https://{TRAEFIK_HOST}/idees/{slug}`.
    pub canonical_url: String,
}

/// Body of the redirection of a merged idea to the idea it was merged into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct IdeaRedirect {
    /// Slug of the idea to show instead.
    pub slug: String,
}

/// Content of an analysis as stored (`analyses.content`, §6.2), read leniently: a field
/// missing in an older analysis is shown empty rather than failing the page.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StoredAnalysis {
    improved_proposal: String,
    existing_situation: String,
    annual_gain_eur: StoredEstimate,
    implementation_cost_eur: StoredEstimate,
    feasibility: StoredFeasibility,
    confidence_explanation: String,
    assumptions: Vec<String>,
    pros: Vec<String>,
    cons: Vec<String>,
    risks: Vec<String>,
    impacts: BTreeMap<String, StoredImpact>,
    sources: Vec<SourceReference>,
    changes_since_previous: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StoredEstimate {
    calculation: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StoredFeasibility {
    obstacles: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StoredImpact {
    effect: String,
    explanation: String,
}

/// Impacts in the order of the publics of the taxonomy, then any other public.
fn impacts(
    taxonomy: &Taxonomy,
    mut stored: BTreeMap<String, StoredImpact>,
) -> Vec<ImpactAssessment> {
    let mut impacts = Vec::with_capacity(stored.len());
    let mut push = |public: String, label: String, impact: StoredImpact| {
        impacts.push(ImpactAssessment {
            public,
            label,
            effect: impact.effect.parse().unwrap_or(ImpactDirection::Uncertain),
            explanation: impact.explanation,
        });
    };
    for entry in &taxonomy.impact_publics {
        if let Some(impact) = stored.remove(&entry.key) {
            push(entry.key.clone(), entry.label.clone(), impact);
        }
    }
    for (public, impact) in stored {
        push(public.clone(), public, impact);
    }
    impacts
}

fn analysis_view(
    taxonomy: &Taxonomy,
    row: &AnalysisRow,
    idea_id: uuid::Uuid,
) -> Result<IdeaAnalysis, ApiError> {
    let stored: StoredAnalysis = serde_json::from_value(row.content.clone()).unwrap_or_else(|_| {
        tracing::warn!(%idea_id, "analysis content unreadable, shown empty");
        StoredAnalysis::default()
    });
    Ok(IdeaAnalysis {
        improved_proposal: stored.improved_proposal,
        existing_situation: stored.existing_situation,
        legal_vehicle: known(&row.legal_vehicle, "legal_vehicle")?,
        annual_gain_eur: AmountEstimate {
            low: row.annual_gain_low_eur,
            median: row.annual_gain_median_eur,
            high: row.annual_gain_high_eur,
            calculation: stored.annual_gain_eur.calculation,
        },
        implementation_cost_eur: AmountEstimate {
            low: row.implementation_cost_low_eur,
            median: row.implementation_cost_median_eur,
            high: row.implementation_cost_high_eur,
            calculation: stored.implementation_cost_eur.calculation,
        },
        time_to_effect_months: u32::try_from(row.time_to_effect_months).unwrap_or(0),
        feasibility: FeasibilityAssessment {
            score: u8::try_from(row.feasibility).unwrap_or(0),
            obstacles: stored.feasibility.obstacles,
        },
        confidence: known(&row.confidence, "confidence")?,
        confidence_explanation: stored.confidence_explanation,
        assumptions: stored.assumptions,
        pros: stored.pros,
        cons: stored.cons,
        risks: stored.risks,
        impacts: impacts(taxonomy, stored.impacts),
        sources: stored.sources,
        changes_since_previous: stored.changes_since_previous,
    })
}

/// Page of a published idea, with its current analysis, its score components, the summaries
/// of the integrated contributions and the rights of reply. The slug of an idea merged into
/// another one answers `301` towards the other idea (its `Location` and the body give its slug).
#[utoipa::path(
    get,
    path = "/api/ideas/{slug}",
    tag = "portal",
    operation_id = "getIdea",
    params(("slug" = String, Path, description = "Slug de l'idée")),
    responses(
        (status = 200, description = "L'idée publiée.", body = IdeaDetail),
        (status = 301, description = "Idée fusionnée dans une autre : voir `Location`.",
         body = IdeaRedirect),
        (status = 404, description = "Idée inconnue, masquée ou purgée (`idea_not_found`).",
         body = ErrorBody)
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    ApiPath(slug): ApiPath<String>,
) -> Result<Response, ApiError> {
    if !is_valid_slug(&slug) {
        return Err(idea_not_found());
    }
    let page = portal::idea_page(&state.pool, &slug)
        .await?
        .ok_or_else(idea_not_found)?;
    let status: IdeaStatus = known(&page.status, "status")?;
    if status == IdeaStatus::Merged {
        let parent = page.parent_slug.ok_or_else(idea_not_found)?;
        return redirect(parent);
    }
    if !status.is_public() {
        return Err(idea_not_found());
    }
    let (Some(analysis_id), Some(original_text)) = (page.current_analysis_id, page.original_text)
    else {
        tracing::error!(idea_id = %page.id, "published idea without analysis or text");
        return Err(ApiError::internal());
    };
    let row = portal::analysis(&state.pool, analysis_id)
        .await?
        .ok_or_else(ApiError::internal)?;
    let taxonomy = &state.prompts.taxonomy;
    let contributions = portal::integrated_contributions(&state.pool, page.id)
        .await?
        .into_iter()
        .map(|c| {
            Ok(ContributionSummary {
                kind: known(&c.kind, "contribution kind")?,
                summary: c.summary,
                taken_into_account: c.taken_into_account,
                response: c.response,
                integrated_on: c.integrated_on,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let rights_of_reply = portal::replies(&state.pool, page.id)
        .await?
        .into_iter()
        .map(|r| RightOfReply {
            author_label: r.author_label,
            body: r.body,
            published_at: r.published_at,
            updated_at: r.updated_at,
        })
        .collect();

    let detail = IdeaDetail {
        analysis: analysis_view(taxonomy, &row, page.id)?,
        admin_level: known(&row.admin_level, "admin_level")?,
        kind: known(&row.kind, "kind")?,
        territory: TerritoryRef::new(row.territory_name.clone(), row.territory_code.clone()),
        category: CategoryRef::new(taxonomy, row.category.clone()),
        original: OriginalText {
            text: original_text,
            personal_data_masked: true,
            attribution: ATTRIBUTION.to_owned(),
            notice: ORIGINAL_TEXT_NOTICE.to_owned(),
        },
        score: ScoreComponents {
            label: SCORE_LABEL.to_owned(),
            scoring_version: row.scoring_version.clone(),
            net_annual_eur: row.net_annual_eur,
            feasibility_factor: row.feasibility_factor,
            confidence_factor: row.confidence_factor,
            delay_factor: row.delay_factor,
            score: row.score_eur,
        },
        contributions,
        rights_of_reply,
        ai: AiMarking {
            ai_generated: true,
            provider: AI_PROVIDER.to_owned(),
            model: row.model.clone(),
            generated_at: row.created_at,
            analysis_version: u32::try_from(row.version).unwrap_or(0),
            prompt_version: row.prompt_version.clone(),
            research_prompt_version: row.research_prompt_version.clone(),
            schema_version: row.schema_version.clone(),
            scoring_version: row.scoring_version.clone(),
            web_search_count: u32::try_from(row.web_search_count).unwrap_or(0),
            human_reviewed: false,
            notice: AI_NOTICE.to_owned(),
        },
        proposal_count: count(page.proposal_count),
        contribution_count: count(page.contribution_count),
        published_on: page.published_on,
        updated_at: row.created_at,
        canonical_url: canonical_url(public_host(&state), &page.slug),
        title: row.title,
        summary: row.summary,
        slug: page.slug,
    };
    Ok(cache::json(detail, cache::SHORT))
}

/// `301` to the page of the idea a duplicate was merged into. The slug comes from the
/// database (`[a-z0-9-]`, CHECK constraint), never from the request.
fn redirect(parent_slug: String) -> Result<Response, ApiError> {
    let location = HeaderValue::from_str(&format!("/api/ideas/{parent_slug}"))
        .map_err(|_| ApiError::internal())?;
    let mut response = cache::with_cache_control(
        (
            StatusCode::MOVED_PERMANENTLY,
            axum::Json(IdeaRedirect { slug: parent_slug }),
        )
            .into_response(),
        cache::SHORT,
    );
    response.headers_mut().insert(header::LOCATION, location);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_texts_are_normalized_and_bounded() {
        assert_eq!(optional_text(None, 5, "x").unwrap(), None);
        assert_eq!(optional_text(Some("   ".into()), 5, "x").unwrap(), None);
        assert_eq!(
            optional_text(Some("  a \n b ".into()), 5, "x").unwrap(),
            Some("a b".to_owned())
        );
        assert!(optional_text(Some("abcdef".into()), 5, "x").is_err());
        assert!(optional_text(Some("a\0b".into()), 5, "x").is_err());
    }
}
