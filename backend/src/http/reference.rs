//! Public reference data (AGENTS.md §6.5, §13.2): the methodology as used in production
//! (prompts, screening grid with the thresholds in force, taxonomy, score formula and every
//! version), the taxonomy, and public counters.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::response::Response;
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use super::AppState;
use super::cache;
use super::error::ApiError;
use crate::ai::decision::ThresholdOverrides;
use crate::ai::prompts::{
    Criterion, CriterionKind, MethodologyFile, Prompts, Taxonomy, TaxonomyEntry,
};
use crate::config::MistralConfig;
use crate::db::provider_alerts;
use crate::domain::rules::{self, CGU_VERSION, RuleReference};
use crate::domain::scoring::{
    AMORTIZATION_YEARS, DELAY_HALF_LIFE_MONTHS, FORMULA_FR, MAX_FEASIBILITY, SCORING_VERSION,
};
use crate::domain::types::{Confidence, ReasoningEffort, TargetKind};
use crate::jobs::analysis::schema_version;
use crate::jobs::exports::EXPORT_SCHEMA_VERSION;
use crate::moderation::section_label;

// ---------------------------------------------------------------------------------------------
// Methodology
// ---------------------------------------------------------------------------------------------

/// A confidence level and its factor in the score.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ConfidenceFactor {
    pub level: Confidence,
    pub factor: f64,
}

/// The composite score (§6.3).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ScoringMethod {
    pub version: String,
    /// The formula in French, as displayed (plain text, several lines).
    pub formula: String,
    /// Years over which the implementation cost is spread.
    pub amortization_years: u32,
    pub max_feasibility: u32,
    /// Delay (months) at which the delay factor is halved.
    pub delay_half_life_months: u32,
    pub confidence_factors: Vec<ConfidenceFactor>,
}

/// Answers of a yes/no criterion, as described to the decision model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct YesNoLabels {
    pub yes: String,
    pub no: String,
}

/// A criterion of the screening grid with the thresholds applied in production.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ScreeningCriterion {
    pub key: String,
    /// `noul` (yes/no question) or `score` (ordered levels).
    #[schema(example = "noul")]
    pub question_type: String,
    /// Question asked to the decision model.
    pub instructions: String,
    /// Meaning of the answers of a `noul` question.
    #[schema(required = true)]
    pub answers: Option<YesNoLabels>,
    /// Levels of a `score` question, lowest first (empty for `noul`).
    pub levels: Vec<String>,
    /// `noul`: the answer that passes the criterion.
    #[schema(required = true)]
    pub pass_when: Option<bool>,
    /// `score`: index of the lowest level that passes.
    #[schema(required = true)]
    pub pass_from_level: Option<u32>,
    /// Below this probability of passing, the criterion fails.
    pub reject_below: f64,
    /// From this probability of passing, the criterion passes; in between: second opinion.
    pub accept_above: f64,
    /// The thresholds differ from `screening.yaml` (adjusted by the editor for calibration).
    pub thresholds_adjusted: bool,
    pub reason_code: String,
    /// Message given to the author when this criterion causes a refusal.
    pub reason_message: String,
    #[schema(required = true)]
    pub rule: Option<RuleReference>,
}

/// The criteria applied to one kind of text, in the order of evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ScreeningSection {
    pub kind: TargetKind,
    #[schema(example = "idée")]
    pub label: String,
    pub criteria: Vec<ScreeningCriterion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ScreeningMethod {
    pub version: String,
    pub sections: Vec<ScreeningSection>,
}

/// A Mistral model in use and the reasoning effort it is sent (§6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MethodologyModel {
    #[schema(example = "mistral-large-latest")]
    pub model: String,
    /// `reasoning_effort` really sent with each call: `high` (full thinking before the answer)
    /// or `none` (minimal thinking); `null` when the parameter is omitted (Mistral's default),
    /// by configuration or because the model refused it.
    #[schema(required = true)]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// `reasoning_effort` configured for this model; it differs from `reasoning_effort` when
    /// the model refused the parameter (its calls then go without it). Always sent; declared
    /// optional so that clients built before it keep compiling.
    #[schema(required = false)]
    pub configured_reasoning_effort: Option<ReasoningEffort>,
}

impl MethodologyModel {
    fn new(model: &str, configured: Option<ReasoningEffort>, refused: &[String]) -> Self {
        Self {
            model: model.to_owned(),
            reasoning_effort: configured.filter(|_| !refused.iter().any(|m| m == model)),
            configured_reasoning_effort: configured,
        }
    }
}

/// The Mistral models configured in production (§6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MethodologyModels {
    /// Both steps of an analysis: web research, then structuring.
    pub analysis: MethodologyModel,
    /// Second opinion on the gray zone and review of reports.
    pub review: MethodologyModel,
    /// Embeddings of the duplicate detection (no reasoning).
    #[schema(example = "mistral-embed")]
    pub embedding: String,
}

impl MethodologyModels {
    /// The configured models and efforts, `refused`: the models known to refuse
    /// `reasoning_effort` (`mistral_reasoning_refusals`), sent without it.
    pub fn from_config(config: &MistralConfig, refused: &[String]) -> Self {
        Self {
            analysis: MethodologyModel::new(
                &config.analysis_model,
                config.analysis_reasoning_effort,
                refused,
            ),
            review: MethodologyModel::new(
                &config.review_model,
                config.review_reasoning_effort,
                refused,
            ),
            embedding: config.embed_model.clone(),
        }
    }
}

/// Prompts, grid, taxonomy and formula as used in production (§6.5).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct MethodologyResponse {
    pub screening_version: String,
    pub taxonomy_version: String,
    /// Prompt name (`analysis`, `analysis_research`, `second_opinion`, `report_review`) →
    /// version.
    pub prompt_versions: BTreeMap<String, String>,
    /// Fingerprint of `analysis.schema.json` stored with each analysis.
    pub analysis_schema_version: String,
    pub export_schema_version: String,
    /// Version of the publication rules (`/regles`).
    pub cgu_version: String,
    /// Mistral models and reasoning efforts in use.
    pub models: MethodologyModels,
    pub scoring: ScoringMethod,
    pub screening: ScreeningMethod,
    /// Full text of every file of `prompts/` (to display as plain text).
    pub files: Vec<MethodologyFile>,
}

fn criterion(
    overrides: &ThresholdOverrides,
    kind: TargetKind,
    criterion: &Criterion,
) -> ScreeningCriterion {
    let thresholds = overrides.effective(kind, criterion);
    let (question_type, answers, levels, pass_when, pass_from_level) = match &criterion.kind {
        CriterionKind::Noul { labels, pass_when } => (
            "noul",
            labels.as_ref().map(|l| YesNoLabels {
                yes: l.yes.clone(),
                no: l.no.clone(),
            }),
            Vec::new(),
            Some(*pass_when),
            None,
        ),
        CriterionKind::Score { levels, pass_when } => (
            "score",
            None,
            levels.clone(),
            None,
            Some(u32::try_from(*pass_when).unwrap_or(u32::MAX)),
        ),
    };
    ScreeningCriterion {
        key: criterion.key.clone(),
        question_type: question_type.to_owned(),
        instructions: criterion.instructions.clone(),
        answers,
        levels,
        pass_when,
        pass_from_level,
        reject_below: thresholds.reject_below,
        accept_above: thresholds.accept_above,
        thresholds_adjusted: thresholds != criterion.thresholds,
        reason_code: criterion.reason_code.clone(),
        reason_message: criterion.reason_message.clone(),
        rule: rules::article_for_reason(&criterion.reason_code).map(RuleReference::from),
    }
}

/// The screening grid with the thresholds in force (settings, §13.6).
pub fn screening_method(prompts: &Prompts, overrides: &ThresholdOverrides) -> ScreeningMethod {
    ScreeningMethod {
        version: prompts.screening.version.clone(),
        sections: TargetKind::ALL
            .iter()
            .map(|&kind| ScreeningSection {
                kind,
                label: section_label(kind).to_owned(),
                criteria: prompts
                    .screening
                    .section(kind)
                    .iter()
                    .map(|c| criterion(overrides, kind, c))
                    .collect(),
            })
            .collect(),
    }
}

fn scoring_method() -> ScoringMethod {
    ScoringMethod {
        version: SCORING_VERSION.to_owned(),
        formula: FORMULA_FR.to_owned(),
        amortization_years: u32::try_from(AMORTIZATION_YEARS).unwrap_or(u32::MAX),
        max_feasibility: u32::from(MAX_FEASIBILITY),
        delay_half_life_months: DELAY_HALF_LIFE_MONTHS,
        confidence_factors: Confidence::ALL
            .iter()
            .map(|&level| ConfidenceFactor {
                level,
                factor: match level {
                    Confidence::Low => 0.5,
                    Confidence::Medium => 0.75,
                    Confidence::High => 1.0,
                },
            })
            .collect(),
    }
}

/// The methodology: prompts, screening grid (with the thresholds in force), taxonomy, score
/// formula and their versions, read from what the service actually uses.
#[utoipa::path(
    get,
    path = "/api/methodology",
    tag = "reference",
    operation_id = "getMethodology",
    responses((status = 200, description = "Méthodologie en vigueur.", body = MethodologyResponse))
)]
pub async fn methodology(State(state): State<AppState>) -> Result<Response, ApiError> {
    let prompts = &state.prompts;
    let overrides = ThresholdOverrides::load(&state.pool).await?;
    let refused: Vec<String> = provider_alerts::reasoning_refusals(&state.pool)
        .await?
        .into_iter()
        .map(|refusal| refusal.model)
        .collect();
    let methodology = &prompts.methodology;
    Ok(cache::json(
        MethodologyResponse {
            screening_version: methodology.screening_version.clone(),
            taxonomy_version: methodology.taxonomy_version.clone(),
            prompt_versions: methodology.prompt_versions.clone(),
            analysis_schema_version: prompts
                .analysis
                .output_schema
                .as_ref()
                .map(schema_version)
                .unwrap_or_default(),
            export_schema_version: EXPORT_SCHEMA_VERSION.to_owned(),
            cgu_version: CGU_VERSION.to_owned(),
            models: MethodologyModels::from_config(&state.config.mistral, &refused),
            scoring: scoring_method(),
            screening: screening_method(prompts, &overrides),
            files: methodology.files.clone(),
        },
        cache::MEDIUM,
    ))
}

// ---------------------------------------------------------------------------------------------
// Taxonomy
// ---------------------------------------------------------------------------------------------

/// An entry of a taxonomy list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TaxonomyItem {
    pub key: String,
    pub label: String,
    #[schema(required = true)]
    pub description: Option<String>,
}

impl From<&TaxonomyEntry> for TaxonomyItem {
    fn from(entry: &TaxonomyEntry) -> Self {
        Self {
            key: entry.key.clone(),
            label: entry.label.clone(),
            description: entry.description.clone(),
        }
    }
}

/// An administrative level and the category groups it accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminLevelItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub category_groups: Vec<String>,
}

/// A category, its group and the administrative levels that accept it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CategoryItem {
    pub key: String,
    pub label: String,
    #[schema(required = true)]
    pub description: Option<String>,
    /// `etat`, `securite_sociale`, `collectivites` or `transverse`.
    pub group: String,
    pub admin_levels: Vec<String>,
}

/// A refusal reason of the screening grid and the rule it applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RejectionReason {
    pub code: String,
    pub message: String,
    #[schema(required = true)]
    pub rule: Option<RuleReference>,
}

/// `prompts/taxonomy.yaml` and the refusal reasons of `prompts/screening.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TaxonomyResponse {
    pub version: String,
    pub admin_levels: Vec<AdminLevelItem>,
    /// Every category, in file order.
    pub categories: Vec<CategoryItem>,
    pub impact_publics: Vec<TaxonomyItem>,
    pub impact_effects: Vec<TaxonomyItem>,
    pub idea_kinds: Vec<TaxonomyItem>,
    pub confidence_levels: Vec<TaxonomyItem>,
    pub legal_vehicles: Vec<TaxonomyItem>,
    pub report_reasons: Vec<TaxonomyItem>,
    pub contribution_kinds: Vec<TaxonomyItem>,
    pub rejection_reasons: Vec<RejectionReason>,
}

fn items(entries: &[TaxonomyEntry]) -> Vec<TaxonomyItem> {
    entries.iter().map(TaxonomyItem::from).collect()
}

pub fn taxonomy_response(prompts: &Prompts) -> TaxonomyResponse {
    let taxonomy: &Taxonomy = &prompts.taxonomy;
    let categories = taxonomy
        .categories
        .iter()
        .flat_map(|(group, entries)| {
            let levels: Vec<String> = taxonomy
                .admin_levels
                .iter()
                .filter(|l| l.category_groups.iter().any(|g| g == group))
                .map(|l| l.key.clone())
                .collect();
            entries.iter().map(move |entry| CategoryItem {
                key: entry.key.clone(),
                label: entry.label.clone(),
                description: entry.description.clone(),
                group: group.clone(),
                admin_levels: levels.clone(),
            })
        })
        .collect();
    let mut rejection_reasons: Vec<RejectionReason> = Vec::new();
    for kind in TargetKind::ALL {
        for criterion in prompts.screening.section(*kind) {
            if rejection_reasons
                .iter()
                .all(|r| r.code != criterion.reason_code)
            {
                rejection_reasons.push(RejectionReason {
                    code: criterion.reason_code.clone(),
                    message: criterion.reason_message.clone(),
                    rule: rules::article_for_reason(&criterion.reason_code)
                        .map(RuleReference::from),
                });
            }
        }
    }
    TaxonomyResponse {
        version: taxonomy.version.clone(),
        admin_levels: taxonomy
            .admin_levels
            .iter()
            .map(|l| AdminLevelItem {
                key: l.key.clone(),
                label: l.label.clone(),
                description: l.description.clone(),
                category_groups: l.category_groups.clone(),
            })
            .collect(),
        categories,
        impact_publics: items(&taxonomy.impact_publics),
        impact_effects: items(&taxonomy.impact_effects),
        idea_kinds: items(&taxonomy.idea_kinds),
        confidence_levels: items(&taxonomy.confidence_levels),
        legal_vehicles: items(&taxonomy.legal_vehicles),
        report_reasons: items(&taxonomy.report_reasons),
        contribution_kinds: items(&taxonomy.contribution_kinds),
        rejection_reasons,
    }
}

/// Administrative levels, categories, impact publics, labels of the enumerations and refusal
/// reasons.
#[utoipa::path(
    get,
    path = "/api/taxonomy",
    tag = "reference",
    operation_id = "getTaxonomy",
    responses((status = 200, description = "Taxonomie en vigueur.", body = TaxonomyResponse))
)]
pub async fn taxonomy(State(state): State<AppState>) -> Response {
    cache::json(taxonomy_response(&state.prompts), cache::MEDIUM)
}

// ---------------------------------------------------------------------------------------------
// Public counters
// ---------------------------------------------------------------------------------------------

/// Number of published ideas of one key (administrative level or kind).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema, sqlx::FromRow)]
pub struct KeyCount {
    pub key: String,
    pub count: i64,
}

/// Public counters. Aggregates only: no text, no identifier, no fine timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PublicStats {
    /// Ideas received and still known (texts deleted or withdrawn by their authors, and the
    /// duplicates merged into a withdrawn idea, are not counted).
    pub ideas_received: i64,
    pub ideas_published: i64,
    /// Being screened, deduplicated or analysed.
    pub ideas_in_progress: i64,
    /// Refused by the screening (texts erased after 30 days, the count stays).
    pub ideas_rejected: i64,
    /// New ideas recognized as duplicates of a published one.
    pub duplicates_merged: i64,
    /// Improvements and objections taken into account by an analysis.
    pub contributions_integrated: i64,
    /// Published ideas per administrative level.
    pub published_by_admin_level: Vec<KeyCount>,
    /// Published ideas per kind (`savings`, `revenue`, `mixed`).
    pub published_by_kind: Vec<KeyCount>,
    /// Day of the most recent publication (Europe/Paris).
    #[schema(required = true)]
    pub last_published_on: Option<NaiveDate>,
    /// Day these counters were computed (Europe/Paris).
    pub computed_on: NaiveDate,
}

async fn stats_of(pool: &PgPool) -> sqlx::Result<PublicStats> {
    #[derive(sqlx::FromRow)]
    struct Totals {
        ideas_received: i64,
        ideas_published: i64,
        ideas_in_progress: i64,
        ideas_rejected: i64,
        duplicates_merged: i64,
        contributions_integrated: i64,
        last_published_on: Option<NaiveDate>,
        computed_on: NaiveDate,
    }
    let totals: Totals = sqlx::query_as(
        "SELECT count(*) AS ideas_received,
                count(*) FILTER (WHERE i.status = 'published') AS ideas_published,
                count(*) FILTER (WHERE i.status IN ('submitted', 'screening', 'second_opinion',
                    'accepted', 'dedup', 'queued_analysis', 'analyzing', 'analysis_failed'))
                    AS ideas_in_progress,
                count(*) FILTER (WHERE i.rejected_at IS NOT NULL) AS ideas_rejected,
                count(*) FILTER (WHERE i.status = 'merged') AS duplicates_merged,
                (SELECT count(*) FROM contributions c JOIN ideas ci ON ci.id = c.idea_id
                 WHERE c.status = 'integrated' AND c.kind <> 'doublon'
                   AND ci.withdrawn_at IS NULL) AS contributions_integrated,
                (max(i.published_at) FILTER (WHERE i.status = 'published')
                    AT TIME ZONE 'Europe/Paris')::date AS last_published_on,
                (now() AT TIME ZONE 'Europe/Paris')::date AS computed_on
         FROM ideas i LEFT JOIN ideas p ON p.id = i.parent_idea_id
         WHERE i.withdrawn_at IS NULL AND p.withdrawn_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    let published_by_admin_level = sqlx::query_as(
        "SELECT admin_level AS key, count(*) AS count FROM ideas
         WHERE status = 'published' AND admin_level IS NOT NULL
         GROUP BY admin_level ORDER BY count(*) DESC, admin_level",
    )
    .fetch_all(pool)
    .await?;
    let published_by_kind = sqlx::query_as(
        "SELECT kind AS key, count(*) AS count FROM ideas
         WHERE status = 'published' AND kind IS NOT NULL
         GROUP BY kind ORDER BY count(*) DESC, kind",
    )
    .fetch_all(pool)
    .await?;
    Ok(PublicStats {
        ideas_received: totals.ideas_received,
        ideas_published: totals.ideas_published,
        ideas_in_progress: totals.ideas_in_progress,
        ideas_rejected: totals.ideas_rejected,
        duplicates_merged: totals.duplicates_merged,
        contributions_integrated: totals.contributions_integrated,
        published_by_admin_level,
        published_by_kind,
        last_published_on: totals.last_published_on,
        computed_on: totals.computed_on,
    })
}

/// Public counters of the platform.
#[utoipa::path(
    get,
    path = "/api/stats",
    tag = "reference",
    operation_id = "getStats",
    responses((status = 200, description = "Compteurs publics.", body = PublicStats))
)]
pub async fn stats(State(state): State<AppState>) -> Result<Response, ApiError> {
    Ok(cache::json(stats_of(&state.pool).await?, cache::SHORT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::prompts::test_prompts;
    use crate::db::settings;
    use crate::test_support::{IdeaSpec, TestApp, get_json, idea_with_status, listed_idea};
    use axum::http::{StatusCode, header};
    use serde_json::json;

    #[test]
    fn taxonomy_lists_categories_with_their_levels() {
        let prompts = test_prompts();
        let taxonomy = taxonomy_response(&prompts);
        let category = taxonomy
            .categories
            .iter()
            .find(|c| c.key == "achats_publics")
            .unwrap();
        assert_eq!(category.label, "Achats publics");
        assert!(!category.admin_levels.is_empty());
        assert_eq!(
            taxonomy.categories.len(),
            prompts.taxonomy.category_keys().len()
        );
        let codes: Vec<&str> = taxonomy
            .rejection_reasons
            .iter()
            .map(|r| r.code.as_str())
            .collect();
        assert_eq!(codes, prompts.screening.reason_codes());
        assert!(taxonomy.rejection_reasons.iter().all(|r| r.rule.is_some()));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn methodology_shows_the_thresholds_in_force(pool: PgPool) {
        let app = TestApp::new(pool.clone(), false, &[]).await;
        let prompts = &app.state.prompts;
        let first = &prompts.screening.idea[0];
        settings::set(
            &pool,
            settings::SCREENING_THRESHOLDS,
            &json!({ "idea": { first.key.clone(): { "reject_below": 0.01 } } }),
        )
        .await
        .unwrap();

        let (status, headers, body) = get_json(&app.router, "/api/methodology", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
        assert_eq!(
            body["screening_version"],
            prompts.screening.version.as_str()
        );
        assert_eq!(body["scoring"]["version"], SCORING_VERSION);
        assert_eq!(body["scoring"]["formula"], FORMULA_FR);
        assert_eq!(
            body["scoring"]["confidence_factors"][0],
            json!({ "level": "low", "factor": 0.5 })
        );
        assert_eq!(body["cgu_version"], CGU_VERSION);
        assert!(
            body["analysis_schema_version"]
                .as_str()
                .unwrap()
                .starts_with("sha256:")
        );
        assert_eq!(
            body["prompt_versions"]["analysis"],
            prompts.analysis.version.as_str()
        );
        let sections = body["screening"]["sections"].as_array().unwrap();
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0]["kind"], "idea");
        let criterion = &sections[0]["criteria"][0];
        assert_eq!(criterion["key"], first.key.as_str());
        assert_eq!(criterion["reject_below"], 0.01);
        assert_eq!(criterion["accept_above"], first.thresholds.accept_above);
        assert_eq!(criterion["thresholds_adjusted"], true);
        assert_eq!(sections[0]["criteria"][1]["thresholds_adjusted"], false);
        let files = body["files"].as_array().unwrap();
        assert!(files.iter().any(|f| f["file"] == "screening.yaml"));
        assert!(files.iter().any(|f| f["file"] == "analysis.schema.json"));
        assert_eq!(
            body["models"],
            json!({
                "analysis": { "model": "mistral-large-latest", "reasoning_effort": "high",
                              "configured_reasoning_effort": "high" },
                "review": { "model": "mistral-large-latest", "reasoning_effort": "none",
                            "configured_reasoning_effort": "none" },
                "embedding": "mistral-embed"
            })
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn methodology_publishes_the_models_and_efforts_in_use(pool: PgPool) {
        // The analysis model refused the parameter: its calls go without it (§6.1).
        provider_alerts::record_reasoning_refusal(&pool, "mistral-large-latest", 400)
            .await
            .unwrap();
        let app = TestApp::new(
            pool,
            false,
            &[
                ("MISTRAL_REVIEW_MODEL", "mistral-medium-3-5"),
                ("MISTRAL_REVIEW_REASONING_EFFORT", "high"),
            ],
        )
        .await;
        let (status, _, body) = get_json(&app.router, "/api/methodology", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["models"]["analysis"],
            json!({ "model": "mistral-large-latest", "reasoning_effort": null,
                    "configured_reasoning_effort": "high" })
        );
        assert_eq!(
            body["models"]["review"],
            json!({ "model": "mistral-medium-3-5", "reasoning_effort": "high",
                    "configured_reasoning_effort": "high" })
        );
        // Omitted by configuration.
        let app = TestApp::new(
            app.pool.clone(),
            false,
            &[("MISTRAL_ANALYSIS_REASONING_EFFORT", "")],
        )
        .await;
        let (_, _, body) = get_json(&app.router, "/api/methodology", &[]).await;
        assert_eq!(
            body["models"]["analysis"],
            json!({ "model": "mistral-large-latest", "reasoning_effort": null,
                    "configured_reasoning_effort": null })
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn taxonomy_and_stats_routes(pool: PgPool) {
        listed_idea(&pool, &IdeaSpec::default()).await;
        listed_idea(
            &pool,
            &IdeaSpec {
                slug: "deuxieme",
                admin_level: "commune",
                category: "achats_publics",
                kind: "revenue",
                ..IdeaSpec::default()
            },
        )
        .await;
        listed_idea(
            &pool,
            &IdeaSpec {
                slug: "masquee",
                status: "hidden",
                ..IdeaSpec::default()
            },
        )
        .await;
        idea_with_status(&pool, "queued_analysis").await;
        sqlx::query(
            "INSERT INTO ideas (original_text, status, rejection_reason_code, rejected_at)
             VALUES ('Texte refusé pour hors sujet.', 'rejected', 'off_topic', now())",
        )
        .execute(&pool)
        .await
        .unwrap();
        let app = TestApp::new(pool, false, &[]).await;

        let (status, headers, body) = get_json(&app.router, "/api/taxonomy", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
        assert_eq!(body["admin_levels"].as_array().unwrap().len(), 7);
        assert!(!body["report_reasons"].as_array().unwrap().is_empty());

        let (status, headers, body) = get_json(&app.router, "/api/stats", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=60");
        assert_eq!(body["ideas_received"], 5);
        assert_eq!(body["ideas_published"], 2);
        assert_eq!(body["ideas_in_progress"], 1);
        assert_eq!(body["ideas_rejected"], 1);
        assert_eq!(body["duplicates_merged"], 0);
        assert_eq!(
            body["published_by_kind"],
            json!([{ "key": "revenue", "count": 1 }, { "key": "savings", "count": 1 }])
        );
        assert!(body["last_published_on"].is_string());
    }
}
