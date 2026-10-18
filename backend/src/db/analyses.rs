//! Analyses of ideas (AGENTS.md §6): what an analysis reads (the idea, the accepted
//! contributions not yet taken into account, the pending corrections of report reviews) and
//! how its result is stored (versioned `analyses` row, publication columns copied to `ideas`,
//! contributions marked as integrated, corrections marked as applied).

use chrono::NaiveDate;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::status::{self, Change, StatusError};
use crate::ai::outputs::AnalysisOutput;
use crate::domain::scoring::ScoreBreakdown;
use crate::domain::slug;
use crate::domain::status::{ContributionStatus, IdeaStatus};
use crate::domain::types::AnalysisTrigger;

/// Contributions given to one analysis at most (prompt size); the next ones are taken by the
/// next run.
pub const MAX_CONTRIBUTIONS_PER_ANALYSIS: i64 = 20;

/// The idea as the analysis reads it.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct IdeaToAnalyze {
    pub status: String,
    /// Masked original text.
    pub text: Option<String>,
    pub admin_level: Option<String>,
    pub territory: Option<String>,
    /// Content of the current analysis (re-analysis).
    pub previous_analysis: Option<Value>,
    /// Today in Europe/Paris (`{{today}}`).
    pub today: NaiveDate,
}

pub async fn idea<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<Option<IdeaToAnalyze>> {
    sqlx::query_as(
        "SELECT i.status, i.original_text AS text, i.submitted_admin_level AS admin_level,
                i.submitted_territory AS territory, a.content AS previous_analysis,
                (now() AT TIME ZONE 'Europe/Paris')::date AS today
         FROM ideas i LEFT JOIN analyses a ON a.id = i.current_analysis_id
         WHERE i.id = $1",
    )
    .bind(idea_id)
    .fetch_optional(executor)
    .await
}

/// An accepted contribution waiting for an analysis. For a merged duplicate (`doublon`), the
/// text is the masked text of the merged idea.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PendingContribution {
    pub id: Uuid,
    pub kind: String,
    pub text: String,
}

pub async fn pending_contributions<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<Vec<PendingContribution>> {
    sqlx::query_as(
        "SELECT c.id, c.kind, coalesce(c.text, s.original_text) AS text
         FROM contributions c LEFT JOIN ideas s ON s.id = c.source_idea_id
         WHERE c.idea_id = $1 AND c.status = 'accepted'
           AND coalesce(c.text, s.original_text) IS NOT NULL
         ORDER BY c.created_at, c.id
         LIMIT $2",
    )
    .bind(idea_id)
    .bind(MAX_CONTRIBUTIONS_PER_ANALYSIS)
    .fetch_all(executor)
    .await
}

/// Corrections of report reviews (`update`) not yet taken into account by an analysis.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PendingCorrection {
    pub review_id: Uuid,
    pub corrections: String,
}

pub async fn pending_corrections<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<Vec<PendingCorrection>> {
    sqlx::query_as(
        "SELECT id AS review_id, corrections FROM report_reviews
         WHERE idea_id = $1 AND outcome = 'update' AND applied_in_analysis_id IS NULL
         ORDER BY created_at, id",
    )
    .bind(idea_id)
    .fetch_all(executor)
    .await
}

/// Whether contributions or corrections still wait for an analysis of the idea (same
/// selection as [`pending_contributions`] and [`pending_corrections`]).
pub async fn has_pending_inputs<'e>(
    executor: impl PgExecutor<'e>,
    idea_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM contributions c LEFT JOIN ideas s ON s.id = c.source_idea_id
                        WHERE c.idea_id = $1 AND c.status = 'accepted'
                          AND coalesce(c.text, s.original_text) IS NOT NULL)
             OR EXISTS (SELECT 1 FROM report_reviews WHERE idea_id = $1 AND outcome = 'update'
                            AND applied_in_analysis_id IS NULL)",
    )
    .bind(idea_id)
    .fetch_one(executor)
    .await
}

/// Versions and provenance of an analysis (§6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub trigger: AnalysisTrigger,
    /// Model of the structuring step.
    pub model: String,
    /// Version of `analysis.md`.
    pub prompt_version: String,
    /// Version of `analysis_research.md`.
    pub research_prompt_version: String,
    /// Fingerprint of `analysis.schema.json`.
    pub schema_version: String,
    pub web_search_count: u64,
}

/// A validated analysis to store.
#[derive(Debug, Clone)]
pub struct NewAnalysis<'a> {
    pub idea_id: Uuid,
    pub output: &'a AnalysisOutput,
    pub score: &'a ScoreBreakdown,
    pub provenance: &'a Provenance,
    /// Contributions given to the prompt: marked as integrated.
    pub contributions: &'a [Uuid],
    /// Report reviews whose corrections were given to the prompt: marked as applied.
    pub corrections: &'a [Uuid],
}

/// Error of [`store`].
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Status(#[from] StatusError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// First free slug for `title`: its slug, then `-2`, `-3`…
pub async fn free_slug(conn: &mut PgConnection, title: &str) -> sqlx::Result<String> {
    let base = slug::slugify(title);
    let mut candidate = base.clone();
    let mut n = 2;
    loop {
        let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM ideas WHERE slug = $1)")
            .bind(&candidate)
            .fetch_one(&mut *conn)
            .await?;
        if !taken {
            return Ok(candidate);
        }
        candidate = slug::with_suffix(&base, n);
        n += 1;
    }
}

/// Stores an analysis in the caller's transaction and publishes it: new version, publication
/// columns of the idea, first publication (`analyzing → published`) when the idea is being
/// analysed for the first time, contributions integrated, corrections applied. The caller has
/// locked the idea and checked its status. The history records `change` with the id of the
/// analysis as details. Returns the id of the analysis.
pub async fn store(
    conn: &mut PgConnection,
    analysis: &NewAnalysis<'_>,
    change: &Change,
) -> Result<Uuid, StoreError> {
    let NewAnalysis {
        idea_id,
        output,
        score,
        provenance,
        contributions,
        corrections,
    } = analysis;
    let idea_id = *idea_id;
    let content = serde_json::to_value(output).map_err(|err| sqlx::Error::Encode(err.into()))?;
    let territory_name = non_empty(&output.territory.name);
    let territory_code = non_empty(&output.territory.insee_code);
    let web_searches = i32::try_from(provenance.web_search_count).unwrap_or(i32::MAX);
    let feasibility = i16::from(output.feasibility.score);
    let months = i32::try_from(output.time_to_effect_months).unwrap_or(i32::MAX);

    let analysis_id: Uuid = sqlx::query_scalar(
        "INSERT INTO analyses (
            idea_id, version, trigger, content, title, summary, kind, category, admin_level,
            territory_name, territory_code, legal_vehicle,
            annual_gain_low_eur, annual_gain_median_eur, annual_gain_high_eur,
            implementation_cost_low_eur, implementation_cost_median_eur,
            implementation_cost_high_eur, time_to_effect_months, feasibility, confidence,
            sources, score_eur, net_annual_eur, feasibility_factor, confidence_factor,
            delay_factor, model, prompt_version, research_prompt_version, schema_version,
            scoring_version, web_search_count
         ) VALUES (
            $1, (SELECT coalesce(max(version), 0) + 1 FROM analyses WHERE idea_id = $1),
            $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19,
            $20, $3->'sources', $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31
         ) RETURNING id",
    )
    .bind(idea_id)
    .bind(provenance.trigger.as_str())
    .bind(&content)
    .bind(&output.title)
    .bind(&output.summary)
    .bind(output.kind.as_str())
    .bind(&output.category)
    .bind(output.admin_level.as_str())
    .bind(territory_name)
    .bind(territory_code)
    .bind(output.legal_vehicle.as_str())
    .bind(output.annual_gain_eur.low)
    .bind(output.annual_gain_eur.median)
    .bind(output.annual_gain_eur.high)
    .bind(output.implementation_cost_eur.low)
    .bind(output.implementation_cost_eur.median)
    .bind(output.implementation_cost_eur.high)
    .bind(months)
    .bind(feasibility)
    .bind(output.confidence.as_str())
    .bind(score.score)
    .bind(score.net_annual_eur)
    .bind(score.feasibility_factor)
    .bind(score.confidence_factor)
    .bind(score.delay_factor)
    .bind(&provenance.model)
    .bind(&provenance.prompt_version)
    .bind(&provenance.research_prompt_version)
    .bind(&provenance.schema_version)
    .bind(score.scoring_version)
    .bind(web_searches)
    .fetch_one(&mut *conn)
    .await?;

    let (status, slug): (String, Option<String>) =
        sqlx::query_as("SELECT status, slug FROM ideas WHERE id = $1")
            .bind(idea_id)
            .fetch_one(&mut *conn)
            .await?;
    let first_publication = status == IdeaStatus::Analyzing.as_str();
    let slug = match slug {
        Some(slug) => slug,
        None => free_slug(conn, &output.title).await?,
    };
    // Publication columns first: `ideas_publication_complete` is checked when the status
    // becomes `published`.
    sqlx::query(
        "UPDATE ideas SET current_analysis_id = $2, slug = $3, title = $4, summary = $5,
             admin_level = $6, territory_name = $7, territory_code = $8, category = $9,
             kind = $10, annual_gain_median_eur = $11, implementation_cost_median_eur = $12,
             time_to_effect_months = $13, feasibility = $14, confidence = $15,
             score_eur = $16, net_annual_eur = $17, feasibility_factor = $18,
             confidence_factor = $19, delay_factor = $20, scoring_version = $21,
             last_analyzed_at = now(),
             published_at = CASE WHEN $22 THEN now() ELSE published_at END,
             first_published_at = CASE WHEN $22 THEN coalesce(first_published_at, now())
                                       ELSE first_published_at END
         WHERE id = $1",
    )
    .bind(idea_id)
    .bind(analysis_id)
    .bind(&slug)
    .bind(&output.title)
    .bind(&output.summary)
    .bind(output.admin_level.as_str())
    .bind(territory_name)
    .bind(territory_code)
    .bind(&output.category)
    .bind(output.kind.as_str())
    .bind(output.annual_gain_eur.median)
    .bind(output.implementation_cost_eur.median)
    .bind(months)
    .bind(feasibility)
    .bind(output.confidence.as_str())
    .bind(score.score)
    .bind(score.net_annual_eur)
    .bind(score.feasibility_factor)
    .bind(score.confidence_factor)
    .bind(score.delay_factor)
    .bind(score.scoring_version)
    .bind(first_publication)
    .execute(&mut *conn)
    .await?;
    let change = Change {
        details: serde_json::json!({ "analysis_id": analysis_id }),
        ..change.clone()
    };
    if first_publication {
        status::transition(conn, idea_id, IdeaStatus::Published, &change).await?;
    }

    for &contribution_id in *contributions {
        let still_accepted = status::lock::<ContributionStatus>(conn, contribution_id).await?
            == Some(ContributionStatus::Accepted);
        if !still_accepted {
            continue;
        }
        let summary = output
            .contributions_considered
            .iter()
            .find(|c| c.contribution_id == contribution_id.to_string())
            .map(|c| c.summary.trim())
            .filter(|s| !s.is_empty());
        // `contributions_integrated_analysis`: the analysis is set before the status.
        sqlx::query(
            "UPDATE contributions SET integrated_in_analysis_id = $2, integrated_at = now(),
                 neutral_summary = $3
             WHERE id = $1",
        )
        .bind(contribution_id)
        .bind(analysis_id)
        .bind(summary)
        .execute(&mut *conn)
        .await?;
        status::transition(
            conn,
            contribution_id,
            ContributionStatus::Integrated,
            &change,
        )
        .await?;
    }
    sqlx::query("UPDATE report_reviews SET applied_in_analysis_id = $2 WHERE id = ANY($1)")
        .bind(*corrections)
        .bind(analysis_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "UPDATE ideas SET contribution_count = (
             SELECT count(*) FROM contributions
             WHERE idea_id = $1 AND status = 'integrated' AND kind <> 'doublon')
         WHERE id = $1",
    )
    .bind(idea_id)
    .execute(&mut *conn)
    .await?;
    Ok(analysis_id)
}
