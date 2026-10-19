//! Mistral second opinion for the gray zone of the screening (AGENTS.md §5.2, §5.3).
//!
//! [`run`] renders `prompts/second_opinion.md` with the grid of the section, the criteria the
//! decision model could not settle and the (masked) text, asks Mistral for a structured answer
//! checked by [`SecondOpinionOutput::check`], records it on the screening and settles
//! `second_opinion → accepted | rejected`.
//!
//! The api first tries it inline ([`run_inline`]): the author gets the final answer when it
//! comes within [`INLINE_WAIT`], « en cours de vérification » otherwise. The call keeps running
//! in the background past that wait, and the `second_opinion` job enqueued with the gray zone
//! takes over if it fails. The worker runs the same [`run`] for the job.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{ModerationError, Stage, Step, Target, Verdict, load_content, section_label};
use crate::ai::budget;
use crate::ai::mistral::{
    CallContext, MistralClient, MistralError, ModelTier, RetryPolicy, Timeouts,
};
use crate::ai::outputs::SecondOpinionOutput;
use crate::ai::prompts::{PromptVars, Prompts, RenderError};
use crate::db::provider_alerts;
use crate::db::status::Change;
use crate::domain::money::MicroEur;
use crate::domain::types::{Actor, AiPurpose, CriterionVerdict};

/// Time the author waits for an inline second opinion (§5.2).
pub const INLINE_WAIT: Duration = Duration::from_secs(10);
/// Bound of the inline call, which goes on in the background past [`INLINE_WAIT`]. One attempt
/// only: the job retries with backoff.
pub const INLINE_CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Delay before the fallback job runs when the api tries the second opinion inline: longer than
/// [`INLINE_CALL_TIMEOUT`], so that both do not ask Mistral at the same time.
pub const FALLBACK_DELAY: chrono::Duration = chrono::Duration::minutes(2);

/// Failure of a second opinion.
#[derive(Debug, thiserror::Error)]
pub enum SecondOpinionError {
    #[error(transparent)]
    Mistral(#[from] MistralError),
    #[error(transparent)]
    Moderation(#[from] ModerationError),
    /// Programming error: the prompt and the code disagree on the variables.
    #[error(transparent)]
    Render(#[from] RenderError),
}

impl From<sqlx::Error> for SecondOpinionError {
    fn from(err: sqlx::Error) -> Self {
        Self::Moderation(err.into())
    }
}

/// The gray-zone screening the second opinion answers.
async fn pending_screening(pool: &PgPool, target: Target) -> sqlx::Result<Option<(Uuid, Value)>> {
    sqlx::query_as(
        "SELECT id, criterion_verdicts FROM screenings
         WHERE target_kind = $1 AND target_id = $2 AND outcome = 'gray_zone'
           AND second_opinion_decision IS NULL
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(target.kind.as_str())
    .bind(target.id)
    .fetch_optional(pool)
    .await
}

/// Reason codes of the gray-zone criteria, in grid order (`{{doubtful}}`).
fn doubtful(prompts: &Prompts, target: Target, verdicts: &Value) -> String {
    prompts
        .screening
        .section(target.kind)
        .iter()
        .filter(|c| {
            verdicts.get(&c.key).and_then(Value::as_str) == Some(CriterionVerdict::Gray.as_str())
        })
        .map(|c| c.reason_code.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Asks Mistral and settles the text. Returns [`Step::Stale`] without calling Mistral when the
/// text is no longer waiting for a second opinion.
pub async fn run(
    pool: &PgPool,
    prompts: &Prompts,
    mistral: &MistralClient,
    target: Target,
    job_id: Option<i64>,
    actor: Actor,
) -> Result<Step, SecondOpinionError> {
    let Some(content) = load_content(pool, prompts, target).await? else {
        return Ok(Step::Stale);
    };
    if content.status != "second_opinion" {
        return Ok(Step::Stale);
    }
    let screening = pending_screening(pool, target).await?;
    let template = &prompts.second_opinion;
    let rendered = template.render(
        &PromptVars::new()
            .set("section", section_label(target.kind))
            .set("grid", prompts.screening.grid_for_prompt(target.kind))
            .set(
                "doubtful",
                screening
                    .as_ref()
                    .map(|(_, verdicts)| doubtful(prompts, target, verdicts))
                    .unwrap_or_default(),
            )
            .set("content", content.prompt_content(target.kind)),
    )?;
    let schema = template
        .output_schema
        .as_ref()
        .expect("second_opinion.md declares its output schema (checked at load)");
    let ctx = CallContext {
        prompt_version: Some(template.version.clone()),
        target: Some((target.kind, target.id)),
        idea_id: Some(content.idea_id),
        job_id,
        ..CallContext::new(AiPurpose::SecondOpinion)
    };
    let section = prompts.screening.section(target.kind);
    let completion = mistral
        .structured::<SecondOpinionOutput>(
            pool,
            &ctx,
            ModelTier::Review,
            &rendered,
            schema,
            |output| output.check(section),
        )
        .await?;
    let output = completion.value;

    let verdict = match output.rejection_reason() {
        Some(code) => Verdict::Reject(code.to_owned()),
        None => Verdict::Accept,
    };
    let mut change = Change::by(actor).details(json!({
        "screening_id": screening.as_ref().map(|(id, _)| id),
        "second_opinion_model": completion.model,
        "prompt_version": template.version,
    }));
    if let Verdict::Reject(code) = &verdict {
        change = change.reason(code.clone());
    }

    let mut tx = pool.begin().await?;
    let step = super::settle_in(
        &mut tx,
        target,
        Stage::SecondOpinion,
        verdict,
        &change,
        None,
    )
    .await?;
    if step != Step::Stale
        && let Some((screening_id, _)) = &screening
    {
        sqlx::query(
            "UPDATE screenings SET second_opinion_decision = $2, second_opinion_reason_code = $3,
                 second_opinion_explanation = $4, second_opinion_model = $5,
                 second_opinion_prompt_version = $6, second_opinion_at = now(),
                 final_decision = $2, decided_at = now()
             WHERE id = $1",
        )
        .bind(screening_id)
        .bind(output.decision.as_str())
        .bind(output.rejection_reason())
        .bind(&output.explanation)
        .bind(&completion.model)
        .bind(&template.version)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    if step == Step::Stale {
        tracing::info!(
            target_kind = target.kind.as_str(),
            target_id = %target.id,
            "second opinion obtained for a text settled meanwhile; ignored"
        );
    }
    Ok(step)
}

/// Tries the second opinion from the api, when Mistral is configured, its key is not known to be
/// refused (`provider_alerts`) and the daily budget allows it. Returns the step when it is settled within [`INLINE_WAIT`], `None` otherwise (the
/// call goes on in the background; the job takes over on failure).
pub async fn run_inline(
    pool: &PgPool,
    prompts: &Arc<Prompts>,
    mistral: Option<&MistralClient>,
    daily_budget: MicroEur,
    target: Target,
) -> Option<Step> {
    let client = mistral?
        .clone()
        .with_retry(RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        })
        .with_timeouts(Timeouts {
            chat: INLINE_CALL_TIMEOUT,
            ..Timeouts::default()
        });
    match budget::status(pool, daily_budget).await {
        Ok(status) if !status.exceeded() => {}
        Ok(_) => return None,
        Err(err) => {
            tracing::warn!(error = %err, "cannot read the AI budget; second opinion left to the job");
            return None;
        }
    }
    // While Mistral refuses the key, the queued job waits for it (one probe per period): no
    // inline call (§6.4).
    match provider_alerts::get(pool, provider_alerts::MISTRAL_AUTH).await {
        Ok(None) => {}
        Ok(Some(_)) => return None,
        Err(err) => {
            tracing::warn!(error = %err, "cannot read the Mistral key alert; second opinion left to the job");
            return None;
        }
    }
    let task = tokio::spawn({
        let pool = pool.clone();
        let prompts = Arc::clone(prompts);
        async move {
            let result = run(&pool, &prompts, &client, target, None, Actor::Api).await;
            if let Err(err) = &result {
                tracing::warn!(
                    target_kind = target.kind.as_str(),
                    target_id = %target.id,
                    error = %err,
                    "inline second opinion failed; the job will retry"
                );
            }
            result
        }
    });
    match tokio::time::timeout(INLINE_WAIT, task).await {
        Ok(Ok(Ok(step))) => Some(step),
        Ok(Ok(Err(_))) | Ok(Err(_)) => None,
        Err(_) => {
            tracing::info!(
                target_kind = target.kind.as_str(),
                target_id = %target.id,
                "second opinion still running; answered « en cours de vérification »"
            );
            None
        }
    }
}
