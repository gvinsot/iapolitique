//! Analysis of an idea by Mistral (AGENTS.md §6, §13.4), job `analyze_idea`.
//!
//! Two steps (§13.4): `analysis_research.md` through the Conversations API with `web_search`
//! gives a sourced memo, then `analysis.md` structures it with a strict `json_schema` answer,
//! checked by [`AnalysisDraft::check`] (ranges ordered and ≥ 0, category allowed for the
//! administrative level, cited `[Sx]` present, at least one source…). An invalid answer is asked
//! again by the client (3 attempts in all, §6.1). Sources (INJ-3): the pages the research
//! consulted, then the sources of the analysis being replaced, are numbered `R1`, `R2`… in the
//! structuring prompt ([`ConsultedPages`]); the answer cites them by identifier only and the
//! published title, publisher and URL come from that list ([`AnalysisDraft::resolve`]). A wrong
//! identifier is asked again with a feedback listing the valid ones. The score is then computed
//! ([`crate::domain::scoring`]) and the analysis stored as a new version with its provenance
//! (prompt versions, schema fingerprint, scoring version, model, web searches).
//!
//! A research that consulted no page (no citable `tool_reference` nor search result: the model
//! may answer without searching) is asked once more in the same run. Still none, and nothing citable from the
//! analysis being replaced: the job is retried later by the queue (transient, the attempt
//! counts, with its backoff), and the last attempt gives up (`analysis_failed`). An analysis is
//! never structured nor published without a page to cite.
//!
//! - **First analysis** (`queued_analysis → analyzing → published`): the publication columns
//!   are copied to the idea, a slug is chosen (suffix `-2`, `-3`… on collision) and the idea
//!   is published. When the analysis cannot be obtained (invalid output after the client's
//!   attempts, non transient Mistral error, last attempt of the job) the idea goes to
//!   `analysis_failed` (admin). A refused API key (401/403) is not such an error: the job waits
//!   without consuming an attempt (`jobs::guard`).
//! - **Re-analysis** of a published or hidden idea (§5.7: contribution, merge, report review
//!   `update`, admin): the previous analysis, the accepted contributions not yet considered and
//!   the pending corrections of report reviews are given to the prompts; the idea keeps its
//!   status, slug and current analysis until the new one replaces it; the contributions become
//!   `integrated` (with the neutral summary written by the analysis) and the corrections
//!   applied. A failure leaves the published analysis in place (the job dies, admin).
//!
//! Inputs that arrive while an analysis runs cannot enqueue another job (same dedupe key): the
//! job runs again at once when some remain. The daily budget is checked before each Mistral
//! call; the job then waits for the next Paris midnight (the idea stays `analyzing`).

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use super::guard::{self, Gate};
use super::pipeline::{self, AnalyzePayload};
use super::worker::{HandlerFuture, JobContext, JobHandler};
use super::{Job, JobError, JobKind, JobOutcome, queue};
use crate::ai::mistral::{
    CallContext, MistralClient, MistralError, ModelTier, consulted_page_count,
};
use crate::ai::outputs::{AnalysisDraft, AnalysisOutput, ConsultedPages};
use crate::ai::prompts::{ANALYSIS_SOURCE_REF, OutputSchema, PromptVars, Prompts, Taxonomy};
use crate::db::analyses::{self, NewAnalysis, PendingContribution, Provenance, StoreError};
use crate::db::status::{self, Change, StatusError};
use crate::domain::scoring::{self, ScoreInput};
use crate::domain::status::IdeaStatus;
use crate::domain::types::{Actor, AiPurpose, AnalysisTrigger, ContributionKind, TargetKind};

/// French label of a merged duplicate given to the analysis as a contribution.
const DUPLICATE_LABEL: &str = "Doublon (même idée proposée par un autre internaute)";

/// Fingerprint of an output schema, stored as `analyses.schema_version`: the schema file has no
/// version of its own, its content identifies it.
pub fn schema_version(schema: &OutputSchema) -> String {
    let bytes = serde_json::to_vec(&*schema.schema).expect("a JSON value serializes");
    let digest = Sha256::digest(&bytes);
    let hex: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// `{{contributions}}`: one line per contribution, « identifiant — type — texte ».
fn contributions_for_prompt(taxonomy: &Taxonomy, contributions: &[PendingContribution]) -> String {
    contributions
        .iter()
        .map(|c| {
            let label = if c.kind == ContributionKind::Duplicate.as_str() {
                DUPLICATE_LABEL
            } else {
                Taxonomy::label(&taxonomy.contribution_kinds, &c.kind).unwrap_or(&c.kind)
            };
            let text = c.text.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("{} — {label} — {text}", c.id)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether this is the first analysis of the idea or a re-analysis of a published one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    First,
    Update,
}

/// Why an analysis could not be produced.
#[derive(Debug)]
enum Failure {
    /// Worth retrying through the job queue.
    Retry(String),
    /// Give up: `analysis_failed` for a first analysis.
    GiveUp(String),
    /// Mistral refused the API key (401/403): a configuration error, the job waits without
    /// consuming an attempt and the idea keeps its status (§6.4, `jobs::guard`).
    KeyRefused,
}

fn classify(err: MistralError, job: &Job) -> Failure {
    let last_attempt = job.attempts >= job.max_attempts;
    match err {
        err if err.is_auth() => Failure::KeyRefused,
        // The client already asked again (§6.1): an output still invalid is a failure.
        MistralError::InvalidOutput(_) => Failure::GiveUp(err.to_string()),
        err if err.is_transient() && !last_attempt => Failure::Retry(err.to_string()),
        err => Failure::GiveUp(err.to_string()),
    }
}

fn status_error(err: StatusError) -> JobError {
    match err {
        StatusError::Database(err) => err.into(),
        other => JobError::Permanent(other.to_string()),
    }
}

fn store_error(err: StoreError) -> JobError {
    match err {
        StoreError::Database(err) => store_db_error(err),
        StoreError::Status(err) => status_error(err),
    }
}

/// A value PostgreSQL cannot store (`22P05` untranslatable character, `22021` character not in
/// repertoire, `22001` string too long) will fail again: the analysis fails for good instead of
/// rerunning its paid calls (INJ-6). Other database errors stay transient.
fn store_db_error(err: sqlx::Error) -> JobError {
    let code = err.as_database_error().and_then(|e| e.code());
    if matches!(code.as_deref(), Some("22P05" | "22021" | "22001")) {
        JobError::Permanent(format!("analysis cannot be stored: {err}"))
    } else {
        err.into()
    }
}

/// Handler of [`JobKind::AnalyzeIdea`].
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalyzeHandler;

impl JobHandler for AnalyzeHandler {
    fn kind(&self) -> JobKind {
        JobKind::AnalyzeIdea
    }

    fn handle<'a>(&'a self, ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a> {
        Box::pin(async move {
            let AnalyzePayload { idea_id, trigger } = job.payload_as()?;
            let pool = &ctx.pool;
            let Some(idea) = analyses::idea(pool, idea_id).await? else {
                return Ok(JobOutcome::Done);
            };
            let status: IdeaStatus =
                idea.status
                    .parse()
                    .map_err(|err: crate::domain::UnknownVariant| {
                        JobError::Permanent(err.to_string())
                    })?;
            let mode = match status {
                IdeaStatus::QueuedAnalysis | IdeaStatus::Analyzing => Mode::First,
                IdeaStatus::Published | IdeaStatus::Hidden => Mode::Update,
                _ => return Ok(JobOutcome::Done),
            };
            let contributions = analyses::pending_contributions(pool, idea_id).await?;
            let corrections = analyses::pending_corrections(pool, idea_id).await?;
            if mode == Mode::Update
                && contributions.is_empty()
                && corrections.is_empty()
                && trigger != AnalysisTrigger::Admin
            {
                // What triggered it was deleted meanwhile, or already taken into account.
                return Ok(JobOutcome::Done);
            }
            let Some(text) = idea.text.clone() else {
                return Ok(JobOutcome::Done);
            };
            let client = match guard::mistral_gate(ctx, self.kind()).await? {
                Gate::Ready(client) => client,
                Gate::Postpone(outcome) => return Ok(outcome),
            };
            if status == IdeaStatus::QueuedAnalysis && !start(pool, idea_id).await? {
                return Ok(JobOutcome::Done);
            }

            let inputs = Inputs {
                idea_id,
                trigger,
                text,
                idea: &idea,
                contributions: &contributions,
                corrections: &corrections,
            };
            match analyse(ctx, client, job, &inputs).await {
                Ok(Analysed::Postponed(outcome)) => Ok(outcome),
                Ok(Analysed::Vanished) => {
                    tracing::info!(idea_id = %idea_id, "idea deleted during its analysis; stopped");
                    Ok(JobOutcome::Done)
                }
                Ok(Analysed::Ready(result)) => {
                    if let Err(err) = publish(pool, &inputs, mode, &result).await {
                        if let (JobError::Permanent(message), Mode::First) = (&err, mode) {
                            fail(pool, idea_id, message).await?;
                        }
                        return Err(err);
                    }
                    if analyses::has_pending_inputs(pool, idea_id).await? {
                        // Arrived while this analysis ran: their job was absorbed by this one.
                        return Ok(JobOutcome::RetryAt(chrono::Utc::now()));
                    }
                    Ok(JobOutcome::Done)
                }
                Err(Failure::Retry(message)) => Err(JobError::Transient(message)),
                Err(Failure::KeyRefused) => guard::auth_postponed(ctx).await,
                Err(Failure::GiveUp(message)) => {
                    if mode == Mode::First {
                        fail(pool, idea_id, &message).await?;
                    }
                    tracing::warn!(idea_id = %idea_id, error = %message, "analysis failed");
                    Err(JobError::Permanent(message))
                }
            }
        })
    }
}

/// What the analysis reads.
struct Inputs<'a> {
    idea_id: Uuid,
    trigger: AnalysisTrigger,
    text: String,
    idea: &'a analyses::IdeaToAnalyze,
    contributions: &'a [PendingContribution],
    corrections: &'a [analyses::PendingCorrection],
}

impl Inputs<'_> {
    /// Variables shared by both prompts.
    fn vars(&self, prompts: &Prompts) -> PromptVars {
        let taxonomy = &prompts.taxonomy;
        let admin_level = self
            .idea
            .admin_level
            .as_deref()
            .map(|key| taxonomy.admin_level(key).map_or(key, |l| l.label.as_str()));
        let corrections = self
            .corrections
            .iter()
            .map(|c| c.corrections.trim())
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        PromptVars::new()
            .set("today", self.idea.today.format("%Y-%m-%d").to_string())
            .set("idea_text", self.text.as_str())
            .set_opt("admin_level", admin_level)
            .set_opt("territory", self.idea.territory.as_deref())
            .set(
                "contributions",
                contributions_for_prompt(taxonomy, self.contributions),
            )
            .set_opt(
                "previous_analysis",
                self.idea.previous_analysis.as_ref().map(|v| v.to_string()),
            )
            .set("report", corrections)
    }

    fn contribution_ids(&self) -> Vec<String> {
        self.contributions
            .iter()
            .map(|c| c.id.to_string())
            .collect()
    }
}

/// A validated analysis and its provenance.
struct AnalysisResult {
    output: AnalysisOutput,
    score: scoring::ScoreBreakdown,
    provenance: Provenance,
}

enum Analysed {
    Ready(Box<AnalysisResult>),
    /// Budget reached between the two steps.
    Postponed(JobOutcome),
    /// The idea was deleted by its author during the research: no further paid call.
    Vanished,
}

/// Error of a research that consulted no page, asked twice in this run: retried later by the
/// queue while the job has attempts left, a failure at the last one.
fn no_page_failure(job: &Job) -> Failure {
    if job.attempts >= job.max_attempts {
        Failure::GiveUp(format!(
            "the web research consulted no page (asked twice in each of {} attempts): nothing \
             can be cited",
            job.attempts
        ))
    } else {
        Failure::Retry("the web research consulted no page (asked twice): retried later".to_owned())
    }
}

/// Before a paid step: the daily budget (`Postponed`) and the idea, which its author may delete
/// at any time (`Vanished`): never pay for a step about a vanished idea (BUD-1).
async fn before_paid_step(ctx: &JobContext, idea_id: Uuid) -> Result<Option<Analysed>, Failure> {
    match guard::budget_gate(ctx).await {
        Ok(Some(outcome)) => return Ok(Some(Analysed::Postponed(outcome))),
        Ok(None) => {}
        Err(err) => return Err(Failure::Retry(err.to_string())),
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM ideas WHERE id = $1)")
        .bind(idea_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|err| Failure::Retry(err.to_string()))?;
    Ok((!exists).then_some(Analysed::Vanished))
}

/// `queued_analysis → analyzing`. Returns whether the analysis should go on.
async fn start(pool: &PgPool, idea_id: Uuid) -> Result<bool, JobError> {
    let mut tx = pool.begin().await?;
    let go_on = match status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?
    {
        Some(IdeaStatus::QueuedAnalysis) => {
            status::transition(
                &mut tx,
                idea_id,
                IdeaStatus::Analyzing,
                &Change::by(Actor::Worker),
            )
            .await
            .map_err(status_error)?;
            true
        }
        Some(IdeaStatus::Analyzing) => true,
        _ => false,
    };
    tx.commit().await?;
    Ok(go_on)
}

/// Research, then structuring, then score.
async fn analyse(
    ctx: &JobContext,
    client: &MistralClient,
    job: &Job,
    inputs: &Inputs<'_>,
) -> Result<Analysed, Failure> {
    let prompts = &ctx.prompts;
    let pool = &ctx.pool;
    let render_failure = |err: crate::ai::prompts::RenderError| Failure::GiveUp(err.to_string());
    let call = |purpose: AiPurpose, version: &str| CallContext {
        prompt_version: Some(version.to_owned()),
        target: Some((TargetKind::Idea, inputs.idea_id)),
        idea_id: Some(inputs.idea_id),
        job_id: Some(job.id),
        ..CallContext::new(purpose)
    };

    let research_template = &prompts.analysis_research;
    let rendered = research_template
        .render(&inputs.vars(prompts))
        .map_err(render_failure)?;
    let research_ctx = call(AiPurpose::AnalysisResearch, &research_template.version);
    let mut research = client
        .research(pool, &research_ctx, ModelTier::Analysis, &rendered)
        .await
        .map_err(|err| classify(err, job))?;
    let mut research_searches = research.usage.web_searches;
    if consulted_page_count(&research.value.references) == 0 {
        // The model decides itself whether to search (Mistral's web search guide): asked once
        // more, never more in one run.
        tracing::warn!(
            idea_id = %inputs.idea_id,
            web_searches = research.usage.web_searches,
            "the web research consulted no page; asking once more"
        );
        if let Some(stop) = before_paid_step(ctx, inputs.idea_id).await? {
            return Ok(stop);
        }
        research = client
            .research(pool, &research_ctx, ModelTier::Analysis, &rendered)
            .await
            .map_err(|err| classify(err, job))?;
        research_searches += research.usage.web_searches;
    }
    if let Some(stop) = before_paid_step(ctx, inputs.idea_id).await? {
        return Ok(stop);
    }

    // The only citable pages (INJ-3): those the web research consulted, then the sources of the
    // analysis being replaced (stored with their URL, whatever their format version).
    let previous_sources = inputs
        .idea
        .previous_analysis
        .as_ref()
        .and_then(|a| a.get("sources"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let url = s.get("url").and_then(serde_json::Value::as_str)?;
            let title = s.get("title").and_then(serde_json::Value::as_str);
            Some((title.unwrap_or_default(), url))
        });
    let pages = ConsultedPages::new(
        research
            .value
            .references
            .iter()
            .filter_map(|r| r.url.as_deref().map(|url| (r.title.as_str(), url)))
            .chain(previous_sources),
    );
    if pages.is_empty() {
        return Err(no_page_failure(job));
    }

    let template = &prompts.analysis;
    let schema = template
        .output_schema
        .as_ref()
        .expect("analysis.md declares its output schema (checked at load)");
    // Strict mode answers one of the identifiers only; the checks catch a model that does not.
    let request_schema = schema
        .constrained(ANALYSIS_SOURCE_REF, &pages.ids())
        .expect("analysis.schema.json has a string `ref` per source (checked at load)");
    let rendered = template
        .render(
            &inputs
                .vars(prompts)
                .set("categories", prompts.taxonomy.categories_for_prompt())
                .set("pages", pages.for_prompt())
                .set("research", research.value.text.as_str()),
        )
        .map_err(render_failure)?;
    let contribution_ids = inputs.contribution_ids();
    let completion = client
        .structured_with_feedback::<AnalysisDraft>(
            pool,
            &call(AiPurpose::AnalysisStructuring, &template.version),
            ModelTier::Analysis,
            &rendered,
            &request_schema,
            |draft| draft.check(&prompts.taxonomy, &contribution_ids, &pages),
        )
        .await
        .map_err(|err| classify(err, job))?;
    let output = completion.value.resolve(&pages);
    let score = scoring::compute(&ScoreInput {
        annual_gain_median_eur: output.annual_gain_eur.median,
        implementation_cost_median_eur: output.implementation_cost_eur.median,
        feasibility: output.feasibility.score,
        confidence: output.confidence,
        time_to_effect_months: output.time_to_effect_months,
    })
    .map_err(|err| Failure::GiveUp(format!("invalid Mistral output: {err}")))?;
    Ok(Analysed::Ready(Box::new(AnalysisResult {
        provenance: Provenance {
            trigger: inputs.trigger,
            model: completion.model,
            prompt_version: template.version.clone(),
            research_prompt_version: research_template.version.clone(),
            schema_version: schema_version(schema),
            web_search_count: research_searches + completion.usage.web_searches,
        },
        output,
        score,
    })))
}

/// Stores and publishes the analysis, if the idea is still where the analysis left it.
async fn publish(
    pool: &PgPool,
    inputs: &Inputs<'_>,
    mode: Mode,
    result: &AnalysisResult,
) -> Result<(), JobError> {
    let idea_id = inputs.idea_id;
    let mut tx = pool.begin().await?;
    let current = status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?;
    let expected = match (mode, current) {
        (Mode::First, Some(IdeaStatus::Analyzing)) => true,
        (Mode::Update, Some(status)) => status.has_publication(),
        _ => false,
    };
    if !expected {
        tracing::info!(idea_id = %idea_id, "analysis obtained for an idea changed meanwhile; ignored");
        return Ok(());
    }
    let contributions: Vec<Uuid> = inputs.contributions.iter().map(|c| c.id).collect();
    let corrections: Vec<Uuid> = inputs.corrections.iter().map(|c| c.review_id).collect();
    let analysis_id = analyses::store(
        &mut tx,
        &NewAnalysis {
            idea_id,
            output: &result.output,
            score: &result.score,
            provenance: &result.provenance,
            contributions: &contributions,
            corrections: &corrections,
        },
        &Change::by(Actor::Worker),
    )
    .await
    .map_err(store_error)?;
    if current == Some(IdeaStatus::Analyzing) || current == Some(IdeaStatus::Published) {
        queue::enqueue(&mut *tx, &pipeline::exports_job()).await?;
    }
    tx.commit().await?;
    tracing::info!(
        idea_id = %idea_id,
        analysis_id = %analysis_id,
        trigger = inputs.trigger.as_str(),
        "analysis published"
    );
    Ok(())
}

/// `analyzing → analysis_failed` (first analysis only).
async fn fail(pool: &PgPool, idea_id: Uuid, message: &str) -> Result<(), JobError> {
    let mut tx = pool.begin().await?;
    if status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?
        == Some(IdeaStatus::Analyzing)
    {
        status::transition(
            &mut tx,
            idea_id,
            IdeaStatus::AnalysisFailed,
            &Change::by(Actor::Worker)
                .reason("analysis_failed")
                .details(json!({ "error": message })),
        )
        .await
        .map_err(status_error)?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests;
