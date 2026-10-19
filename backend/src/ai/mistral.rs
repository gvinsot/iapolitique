//! Mistral API client (AGENTS.md §6): structured chat completions, web research through the
//! Conversations API, embeddings. Every HTTP attempt is recorded in `ai_calls` with its usage and
//! its estimated cost.
//!
//! Request and response formats checked against the documentation on 2026-10-18:
//! - API reference and OpenAPI spec: <https://docs.mistral.ai/api/>,
//!   <https://docs.mistral.ai/openapi.yaml> (`ChatCompletionRequest`, `ResponseFormat`,
//!   `JsonSchema`, `ChatCompletionResponse`, `UsageInfo`, `ConversationRequest`,
//!   `ConversationResponse`, `MessageOutputEntry`, `ToolExecutionEntry`, `ToolReferenceChunk`,
//!   `ConversationUsageInfo`, `EmbeddingRequest`, `EmbeddingResponse`);
//! - custom structured outputs (`response_format` `json_schema`, `strict`):
//!   <https://docs.mistral.ai/studio/conversations/structured-output/custom>;
//! - web search, available through the Conversations / Agents API only (not chat
//!   completions): <https://docs.mistral.ai/agents/tools/built-in/websearch>;
//! - embeddings (`mistral-embed`, 1024 dimensions):
//!   <https://docs.mistral.ai/capabilities/embeddings/>.
//!
//! Privacy (§7.5): requests carry the rendered prompts only (masked texts, internal ids), never
//! an IP, a fingerprint or a token; conversations are created with `store: false`. Provider
//! error bodies are never logged (they may quote the request).

use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::time::Instant;
use uuid::Uuid;

use super::prompts::{OutputSchema, RenderedPrompt};
use crate::config::{MistralConfig, MistralPrices, Secret};
use crate::db::{self, ai_calls::NewAiCall, provider_alerts};
use crate::domain::money::MicroEur;
use crate::domain::types::{AiCallStatus, AiProvider, AiPurpose, ReasoningEffort, TargetKind};

/// Dimensions of `mistral-embed` vectors (`idea_embeddings.embedding vector(1024)`).
pub const EMBEDDING_DIMENSIONS: usize = 1024;
/// Sampling temperature of structured outputs: focused, near deterministic answers.
const STRUCTURED_TEMPERATURE: f64 = 0.2;
/// Temperature of the research step.
const RESEARCH_TEMPERATURE: f64 = 0.3;

/// Retry policy of a call: every attempt is recorded; only transient failures (429, 5xx,
/// timeouts, connection errors) and invalid outputs are retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts per call, the first included (AGENTS.md §6.1: 3 at most).
    pub max_attempts: u32,
    /// Delay before the second attempt, doubled each time.
    pub base_delay: Duration,
    /// Upper bound of a delay, `Retry-After` included.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
        }
    }
}

impl RetryPolicy {
    fn delay(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        let exponential = self
            .base_delay
            .saturating_mul(2u32.saturating_pow(attempt.saturating_sub(1)));
        retry_after
            .unwrap_or(exponential)
            .max(exponential)
            .min(self.max_delay)
    }
}

/// Per-request timeouts: web research and long analyses take minutes, longer still when the
/// model thinks first (`reasoning_effort` `high`, §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub chat: Duration,
    pub conversation: Duration,
    pub embedding: Duration,
    /// Chat completion with `reasoning_effort` `high`.
    pub reasoning_chat: Duration,
    /// Conversation with `reasoning_effort` `high`.
    pub reasoning_conversation: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            chat: Duration::from_secs(180),
            conversation: Duration::from_secs(300),
            embedding: Duration::from_secs(30),
            reasoning_chat: Duration::from_secs(360),
            reasoning_conversation: Duration::from_secs(480),
        }
    }
}

impl Timeouts {
    /// The same timeout for every request (tests, inline calls).
    pub fn uniform(timeout: Duration) -> Self {
        Self {
            chat: timeout,
            conversation: timeout,
            embedding: timeout,
            reasoning_chat: timeout,
            reasoning_conversation: timeout,
        }
    }

    fn longest(&self) -> Duration {
        [
            self.chat,
            self.conversation,
            self.embedding,
            self.reasoning_chat,
            self.reasoning_conversation,
        ]
        .into_iter()
        .max()
        .unwrap_or_default()
    }

    fn chat(&self, effort: Option<ReasoningEffort>) -> Duration {
        match effort {
            Some(ReasoningEffort::High) => self.reasoning_chat,
            _ => self.chat,
        }
    }

    fn conversation(&self, effort: Option<ReasoningEffort>) -> Duration {
        match effort {
            Some(ReasoningEffort::High) => self.reasoning_conversation,
            _ => self.conversation,
        }
    }
}

/// Model family of a call: decides the model name and the unit prices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    /// `MISTRAL_ANALYSIS_MODEL`: research and structuring of analyses.
    Analysis,
    /// `MISTRAL_REVIEW_MODEL`: second opinions and report reviews.
    Review,
}

/// Failure of a Mistral call, after its retries. The `Display` never contains provider text or
/// user text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MistralError {
    #[error("MISTRAL_API_KEY is not set")]
    MissingApiKey,
    #[error("Mistral timeout")]
    Timeout,
    #[error("Mistral unreachable")]
    Unreachable,
    #[error("Mistral answered HTTP {status}")]
    Http { status: u16 },
    /// The answer is not usable (not JSON, schema violation, truncated…).
    #[error("invalid Mistral output: {0}")]
    InvalidOutput(String),
    #[error("database error while recording the call: {0}")]
    Database(String),
}

impl MistralError {
    /// The API key is refused (HTTP 401 or 403): a configuration error like a missing key, not
    /// a failure of the job. The jobs wait without consuming an attempt (`jobs::guard`), the
    /// alert is recorded in `provider_alerts` and shown on the admin dashboard.
    pub fn is_auth(&self) -> bool {
        matches!(self, Self::Http { status: 401 | 403 })
    }

    /// Worth retrying later (job backoff): rate limit, server errors, network, invalid output.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Timeout | Self::Unreachable | Self::InvalidOutput(_) | Self::Database(_) => true,
            Self::Http { status } => is_retryable_status(*status),
            Self::MissingApiKey => false,
        }
    }

    fn code(&self) -> String {
        match self {
            Self::MissingApiKey => "missing_api_key".to_owned(),
            Self::Timeout => "timeout".to_owned(),
            Self::Unreachable => "unreachable".to_owned(),
            Self::Http { status } => format!("http_{status}"),
            Self::InvalidOutput(_) => "invalid_output".to_owned(),
            Self::Database(_) => "database".to_owned(),
        }
    }

    fn status(&self) -> AiCallStatus {
        match self {
            Self::Timeout => AiCallStatus::Timeout,
            Self::InvalidOutput(_) => AiCallStatus::InvalidOutput,
            _ => AiCallStatus::Error,
        }
    }

    fn retryable(&self) -> bool {
        match self {
            Self::Timeout | Self::Unreachable | Self::InvalidOutput(_) => true,
            Self::Http { status } => is_retryable_status(*status),
            Self::MissingApiKey | Self::Database(_) => false,
        }
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || status == 408 || status >= 500
}

impl From<sqlx::Error> for MistralError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.to_string())
    }
}

/// What a call is about, recorded in `ai_calls`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContext {
    pub purpose: AiPurpose,
    /// Version of the prompt file used.
    pub prompt_version: Option<String>,
    pub target: Option<(TargetKind, Uuid)>,
    /// Must exist in `ideas` when set.
    pub idea_id: Option<Uuid>,
    pub job_id: Option<i64>,
}

impl CallContext {
    pub fn new(purpose: AiPurpose) -> Self {
        Self {
            purpose,
            prompt_version: None,
            target: None,
            idea_id: None,
            job_id: None,
        }
    }
}

/// Token usage of one attempt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub web_searches: u64,
}

impl Usage {
    fn add(&mut self, other: Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.web_searches += other.web_searches;
    }
}

/// Successful call: the value, the model that produced it and what all attempts cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion<T> {
    pub value: T,
    pub model: String,
    /// Usage summed over the attempts.
    pub usage: Usage,
    /// Estimated cost summed over the attempts.
    pub cost: MicroEur,
    pub attempts: u32,
}

/// Output of the research step: sourced memo and the pages consulted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Research {
    pub text: String,
    /// Web pages referenced by the answer, deduplicated by URL, in order of appearance.
    pub references: Vec<WebReference>,
}

/// A `tool_reference` chunk of a conversation answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebReference {
    pub title: String,
    pub url: Option<String>,
    pub source: Option<String>,
}

/// Result of one HTTP attempt before recording.
struct Attempt<T> {
    result: Result<T, MistralError>,
    usage: Usage,
    model: Option<String>,
    http_status: Option<u16>,
    retry_after: Option<Duration>,
}

/// Client of the Mistral API.
#[derive(Debug, Clone)]
pub struct MistralClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Secret,
    analysis_model: String,
    review_model: String,
    embed_model: String,
    analysis_effort: Option<ReasoningEffort>,
    review_effort: Option<ReasoningEffort>,
    prices: MistralPrices,
    retry: RetryPolicy,
    timeouts: Timeouts,
}

impl MistralClient {
    /// Builds the client; `MISTRAL_API_KEY` is required.
    pub fn new(config: &MistralConfig) -> Result<Self, MistralError> {
        let api_key = config.api_key.clone().ok_or(MistralError::MissingApiKey)?;
        let timeouts = Timeouts::default();
        let http = super::http_client(timeouts.longest()).map_err(|_| MistralError::Unreachable)?;
        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            api_key,
            analysis_model: config.analysis_model.clone(),
            review_model: config.review_model.clone(),
            embed_model: config.embed_model.clone(),
            analysis_effort: config.analysis_reasoning_effort,
            review_effort: config.review_reasoning_effort,
            prices: config.prices.clone(),
            retry: RetryPolicy::default(),
            timeouts,
        })
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    pub fn model(&self, tier: ModelTier) -> &str {
        match tier {
            ModelTier::Analysis => &self.analysis_model,
            ModelTier::Review => &self.review_model,
        }
    }

    /// `reasoning_effort` sent with the calls of `tier`; `None`: omitted (Mistral's default).
    pub fn reasoning_effort(&self, tier: ModelTier) -> Option<ReasoningEffort> {
        match tier {
            ModelTier::Analysis => self.analysis_effort,
            ModelTier::Review => self.review_effort,
        }
    }

    /// Estimated cost of a usage: tokens at the configured prices of the tier, plus the web
    /// searches. Rounded up: the budget is never underestimated.
    pub fn cost(&self, tier: Option<ModelTier>, usage: Usage) -> MicroEur {
        let (input, output) = match tier {
            Some(ModelTier::Analysis) => (
                self.prices.analysis_input_per_mtok,
                self.prices.analysis_output_per_mtok,
            ),
            Some(ModelTier::Review) => (
                self.prices.review_input_per_mtok,
                self.prices.review_output_per_mtok,
            ),
            None => (self.prices.embed_per_mtok, self.prices.embed_per_mtok),
        };
        input.per_million(usage.input_tokens)
            + output.per_million(usage.output_tokens)
            + self.prices.web_search_per_call.times(usage.web_searches)
    }

    /// Chat completion constrained by `schema` (`response_format` `json_schema`, strict), parsed
    /// into `T` and checked by `check`. An answer that is not JSON, violates the schema, does
    /// not deserialize or fails `check` is an invalid output, retried within the policy.
    pub async fn structured<T: DeserializeOwned>(
        &self,
        pool: &PgPool,
        ctx: &CallContext,
        tier: ModelTier,
        prompt: &RenderedPrompt,
        schema: &OutputSchema,
        check: impl Fn(&T) -> Result<(), String>,
    ) -> Result<Completion<T>, MistralError> {
        let effort = self.reasoning_effort(tier);
        let mut body = json!({
            "model": self.model(tier),
            "messages": [
                { "role": "system", "content": prompt.system },
                { "role": "user", "content": prompt.user },
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": { "name": schema.name, "schema": &*schema.schema, "strict": true },
            },
            "temperature": STRUCTURED_TEMPERATURE,
            "stream": false,
        });
        // A top-level field of chat completions.
        if let Some(effort) = effort {
            body["reasoning_effort"] = json!(effort.as_str());
        }
        self.run(pool, ctx, Some(tier), || async {
            let (response, http_status) = match self
                .post("/v1/chat/completions", &body, self.timeouts.chat(effort))
                .await
            {
                Ok(ok) => ok,
                Err(failure) => return failure.into(),
            };
            let parsed = parse_chat(&response);
            let result = parsed.content.and_then(|content| {
                let value: Value = serde_json::from_str(&content)
                    .map_err(|_| MistralError::InvalidOutput("answer is not JSON".into()))?;
                schema
                    .validate(&value)
                    .map_err(|err| MistralError::InvalidOutput(err.to_string()))?;
                let typed: T = serde_json::from_value(value).map_err(|err| {
                    MistralError::InvalidOutput(format!("unexpected shape: {err}"))
                })?;
                check(&typed).map_err(MistralError::InvalidOutput)?;
                Ok(typed)
            });
            Attempt {
                result,
                usage: parsed.usage,
                model: parsed.model,
                http_status: Some(http_status),
                retry_after: None,
            }
        })
        .await
    }

    /// Web research through the Conversations API with the `web_search` tool: free text plus
    /// the pages it references. The conversation is not stored by Mistral (`store: false`).
    pub async fn research(
        &self,
        pool: &PgPool,
        ctx: &CallContext,
        tier: ModelTier,
        prompt: &RenderedPrompt,
    ) -> Result<Completion<Research>, MistralError> {
        let effort = self.reasoning_effort(tier);
        let mut body = json!({
            "model": self.model(tier),
            "instructions": prompt.system,
            "inputs": prompt.user,
            "tools": [{ "type": "web_search" }],
            "completion_args": { "temperature": RESEARCH_TEMPERATURE },
            "store": false,
            "stream": false,
        });
        // Conversations take it among the completion arguments.
        if let Some(effort) = effort {
            body["completion_args"]["reasoning_effort"] = json!(effort.as_str());
        }
        self.run(pool, ctx, Some(tier), || async {
            let (response, http_status) = match self
                .post(
                    "/v1/conversations",
                    &body,
                    self.timeouts.conversation(effort),
                )
                .await
            {
                Ok(ok) => ok,
                Err(failure) => return failure.into(),
            };
            let parsed = parse_conversation(&response);
            Attempt {
                result: parsed.research,
                usage: parsed.usage,
                model: parsed.model,
                http_status: Some(http_status),
                retry_after: None,
            }
        })
        .await
    }

    /// Embeddings of `texts` with `MISTRAL_EMBED_MODEL`, in input order, each of
    /// [`EMBEDDING_DIMENSIONS`] dimensions.
    pub async fn embed(
        &self,
        pool: &PgPool,
        ctx: &CallContext,
        texts: &[&str],
    ) -> Result<Completion<Vec<Vec<f32>>>, MistralError> {
        let body = json!({ "model": self.embed_model, "input": texts });
        self.run(pool, ctx, None, || async {
            let (response, http_status) = match self
                .post("/v1/embeddings", &body, self.timeouts.embedding)
                .await
            {
                Ok(ok) => ok,
                Err(failure) => return failure.into(),
            };
            let (result, usage) = parse_embeddings(&response, texts.len());
            Attempt {
                result,
                usage,
                model: Some(self.embed_model.clone()),
                http_status: Some(http_status),
                retry_after: None,
            }
        })
        .await
    }

    /// Runs the attempts of a call, records each of them and sums usage and cost.
    async fn run<T, F, Fut>(
        &self,
        pool: &PgPool,
        ctx: &CallContext,
        tier: Option<ModelTier>,
        attempt_fn: F,
    ) -> Result<Completion<T>, MistralError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Attempt<T>>,
    {
        let default_model = match tier {
            Some(tier) => self.model(tier).to_owned(),
            None => self.embed_model.clone(),
        };
        let mut total = Usage::default();
        let mut total_cost = MicroEur::ZERO;
        let mut attempt_number = 0;
        loop {
            attempt_number += 1;
            let started = Instant::now();
            let attempt = attempt_fn().await;
            let latency = started.elapsed();
            let cost = self.cost(tier, attempt.usage);
            total.add(attempt.usage);
            total_cost = total_cost + cost;
            let model = attempt
                .model
                .clone()
                .unwrap_or_else(|| default_model.clone());
            let call = NewAiCall {
                target: ctx.target,
                idea_id: ctx.idea_id,
                job_id: ctx.job_id,
                prompt_version: ctx.prompt_version.clone(),
                input_tokens: attempt.usage.input_tokens,
                output_tokens: attempt.usage.output_tokens,
                web_search_count: attempt.usage.web_searches,
                cost,
                latency,
                status: attempt
                    .result
                    .as_ref()
                    .map_or_else(MistralError::status, |_| AiCallStatus::Success),
                http_status: attempt.http_status,
                error_code: attempt.result.as_ref().err().map(MistralError::code),
                ..NewAiCall::new(AiProvider::Mistral, ctx.purpose, model.clone())
            };
            // The attempt is paid whatever happens next: a recording failure is logged, never
            // turned into an error that a job would retry with a new paid call (BUD-1).
            if let Err(err) = db::ai_calls::record(pool, &call).await {
                tracing::error!(
                    purpose = ctx.purpose.as_str(),
                    cost_micro_eur = cost.0,
                    error = %err,
                    "Mistral call not recorded in ai_calls"
                );
            }
            self.track_auth(pool, ctx, attempt.result.as_ref().err())
                .await;
            match attempt.result {
                Ok(value) => {
                    return Ok(Completion {
                        value,
                        model,
                        usage: total,
                        cost: total_cost,
                        attempts: attempt_number,
                    });
                }
                Err(error) => {
                    tracing::warn!(
                        purpose = ctx.purpose.as_str(),
                        attempt = attempt_number,
                        error = %error.code(),
                        "Mistral call failed"
                    );
                    if !error.retryable() || attempt_number >= self.retry.max_attempts {
                        return Err(error);
                    }
                    tokio::time::sleep(self.retry.delay(attempt_number, attempt.retry_after)).await;
                }
            }
        }
    }

    /// Keeps the `mistral_auth` alert (`provider_alerts`) up to date: a refused key opens or
    /// extends it, logged as an ERROR once per period whatever the number of jobs; an answer
    /// that got past authentication ends it. Never logs the key.
    async fn track_auth(&self, pool: &PgPool, ctx: &CallContext, error: Option<&MistralError>) {
        let result = match error {
            Some(MistralError::Http {
                status: status @ (401 | 403),
            }) => provider_alerts::record_failure(pool, provider_alerts::MISTRAL_AUTH, *status)
                .await
                .map(|log| {
                    if log {
                        tracing::error!(
                            purpose = ctx.purpose.as_str(),
                            http_status = status,
                            "Mistral refused the API key: check MISTRAL_API_KEY (AI jobs wait)"
                        );
                    }
                }),
            // An answer, or a refusal of the request itself: the key was accepted.
            None | Some(MistralError::InvalidOutput(_)) => self.close_auth_alert(pool, ctx).await,
            Some(MistralError::Http { status }) if (400..500).contains(status) => {
                self.close_auth_alert(pool, ctx).await
            }
            // Timeouts, network and server errors say nothing about the key.
            Some(_) => Ok(()),
        };
        if let Err(err) = result {
            tracing::warn!(error = %err, "cannot update the Mistral key alert");
        }
    }

    async fn close_auth_alert(&self, pool: &PgPool, ctx: &CallContext) -> sqlx::Result<()> {
        if provider_alerts::clear(pool, provider_alerts::MISTRAL_AUTH).await? {
            tracing::info!(
                purpose = ctx.purpose.as_str(),
                "Mistral accepts the API key again: alert closed"
            );
        }
        Ok(())
    }

    /// POST with the API key: the JSON body and the HTTP status, or the failure (a non-2xx
    /// status carries `Retry-After`).
    async fn post(
        &self,
        path: &str,
        body: &Value,
        timeout: Duration,
    ) -> Result<(Value, u16), Failure> {
        let response = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(self.api_key.expose())
            .timeout(timeout)
            .json(body)
            .send()
            .await
            .map_err(|err| {
                Failure::new(if err.is_timeout() {
                    MistralError::Timeout
                } else {
                    MistralError::Unreachable
                })
            })?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(Failure {
                error: MistralError::Http { status },
                http_status: Some(status),
                retry_after,
            });
        }
        let failure = |error| Failure {
            error,
            http_status: Some(status),
            retry_after: None,
        };
        let bytes = match super::read_capped(response, super::MAX_MISTRAL_RESPONSE_BYTES).await {
            Ok(bytes) => bytes,
            Err(super::BodyError::TooLarge) => {
                return Err(failure(MistralError::InvalidOutput(
                    "response too large".into(),
                )));
            }
            Err(super::BodyError::Transport(err)) => {
                return Err(failure(if err.is_timeout() {
                    MistralError::Timeout
                } else {
                    MistralError::Unreachable
                }));
            }
        };
        serde_json::from_slice::<Value>(&bytes)
            .map(|value| (value, status))
            .map_err(|_| failure(MistralError::InvalidOutput("body is not JSON".into())))
    }
}

/// A request that produced no usable body.
struct Failure {
    error: MistralError,
    http_status: Option<u16>,
    retry_after: Option<Duration>,
}

impl Failure {
    fn new(error: MistralError) -> Self {
        Self {
            error,
            http_status: None,
            retry_after: None,
        }
    }
}

impl<T> From<Failure> for Attempt<T> {
    fn from(failure: Failure) -> Self {
        Self {
            result: Err(failure.error),
            usage: Usage::default(),
            model: None,
            http_status: failure.http_status,
            retry_after: failure.retry_after,
        }
    }
}

fn u64_at(value: &Value, pointer: &str) -> u64 {
    value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

/// Output tokens of a response's `usage`: `completion_tokens`, plus the reasoning tokens when the
/// response reports them apart (`completion_tokens_details.reasoning_tokens`), and never less
/// than `total_tokens` minus the input. Thinking is billed as output: counting it in every case
/// keeps the daily budget an upper bound (§6.4), at worst a slight overestimate.
fn output_tokens(response: &Value, input_tokens: u64) -> u64 {
    let completion = u64_at(response, "/usage/completion_tokens");
    let reasoning = u64_at(
        response,
        "/usage/completion_tokens_details/reasoning_tokens",
    );
    let total = u64_at(response, "/usage/total_tokens");
    completion
        .saturating_add(reasoning)
        .max(total.saturating_sub(input_tokens))
}

/// Type of a content chunk (`text`, `thinking`, `tool_reference`…).
fn chunk_type(chunk: &Value) -> Option<&str> {
    chunk.get("type").and_then(Value::as_str)
}

/// The answer of a message `content`: the string itself, or the `text` chunks of a chunk array
/// joined in order. Reasoning (`thinking` chunks, §6.1) is skipped without being read: it is
/// never stored nor logged, as it may echo visitor text. `None` when the content is neither a
/// string nor an array holding at least one text chunk.
fn answer_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(chunks) => {
            let texts: Vec<&str> = chunks
                .iter()
                .filter(|chunk| chunk_type(chunk) == Some("text"))
                .filter_map(|chunk| chunk.get("text").and_then(Value::as_str))
                .collect();
            (!texts.is_empty()).then(|| texts.concat())
        }
        _ => None,
    }
}

/// Adds a `tool_reference` chunk to `references`, unless the same page is already there.
fn push_reference(references: &mut Vec<WebReference>, chunk: &Value) {
    let text = |key: &str| chunk.get(key).and_then(Value::as_str).map(str::to_owned);
    let reference = WebReference {
        title: text("title").unwrap_or_default(),
        url: text("url"),
        source: text("source"),
    };
    let known = references.iter().any(|r| {
        r.url.is_some() && r.url == reference.url || r.url.is_none() && r.title == reference.title
    });
    if !known {
        references.push(reference);
    }
}

struct ParsedChat {
    content: Result<String, MistralError>,
    usage: Usage,
    model: Option<String>,
}

/// `choices[0].message.content` (a string, or chunks: text and reasoning) and `usage`.
fn parse_chat(response: &Value) -> ParsedChat {
    let input_tokens = u64_at(response, "/usage/prompt_tokens");
    let usage = Usage {
        input_tokens,
        output_tokens: output_tokens(response, input_tokens),
        web_searches: 0,
    };
    let model = response
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let content = (|| {
        let choice = response
            .pointer("/choices/0")
            .ok_or_else(|| MistralError::InvalidOutput("no choice".into()))?;
        match choice.get("finish_reason").and_then(Value::as_str) {
            Some("length" | "model_length") => {
                return Err(MistralError::InvalidOutput("answer truncated".into()));
            }
            Some("error") => return Err(MistralError::InvalidOutput("generation error".into())),
            _ => {}
        }
        choice
            .pointer("/message/content")
            .and_then(answer_text)
            .ok_or_else(|| MistralError::InvalidOutput("no text in the answer".into()))
    })();
    ParsedChat {
        content,
        usage,
        model,
    }
}

struct ParsedConversation {
    research: Result<Research, MistralError>,
    usage: Usage,
    model: Option<String>,
}

/// Text and references of the `message.output` entries; web searches counted from the
/// `tool.execution` entries and from `usage.connectors`, whichever is larger. A `thinking`
/// chunk (reasoning, §6.1) contributes only its `tool_reference` chunks — pages the web search
/// returned, citable like the others —, never its text.
fn parse_conversation(response: &Value) -> ParsedConversation {
    let outputs = response
        .get("outputs")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut text = String::new();
    let mut references: Vec<WebReference> = Vec::new();
    let mut executions = 0u64;
    let mut model = None;
    for entry in outputs {
        match entry.get("type").and_then(Value::as_str) {
            Some("tool.execution") => {
                if entry
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| n.starts_with("web_search"))
                {
                    executions += 1;
                }
            }
            Some("message.output") => {
                if model.is_none() {
                    model = entry
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                let Some(content) = entry.get("content") else {
                    continue;
                };
                if let Some(answer) = answer_text(content) {
                    text.push_str(&answer);
                }
                for chunk in content.as_array().map(Vec::as_slice).unwrap_or_default() {
                    match chunk_type(chunk) {
                        Some("tool_reference") => push_reference(&mut references, chunk),
                        Some("thinking") => {
                            let inner = chunk.get("thinking").and_then(Value::as_array);
                            for inner in inner.map(Vec::as_slice).unwrap_or_default() {
                                if chunk_type(inner) == Some("tool_reference") {
                                    push_reference(&mut references, inner);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let connector_searches: u64 = response
        .pointer("/usage/connectors")
        .and_then(Value::as_object)
        .map(|connectors| {
            connectors
                .iter()
                .filter(|(name, _)| name.starts_with("web_search"))
                .filter_map(|(_, count)| count.as_u64())
                .sum()
        })
        .unwrap_or(0);
    // Tool results count as input: never underestimate the cost.
    let input_tokens =
        u64_at(response, "/usage/prompt_tokens") + u64_at(response, "/usage/connector_tokens");
    let usage = Usage {
        input_tokens,
        output_tokens: output_tokens(response, input_tokens),
        web_searches: executions.max(connector_searches),
    };
    let research = if text.trim().is_empty() {
        Err(MistralError::InvalidOutput("empty research answer".into()))
    } else {
        Ok(Research {
            text: text.trim().to_owned(),
            references,
        })
    };
    ParsedConversation {
        research,
        usage,
        model,
    }
}

/// Vectors sorted by `index`, checked for count and dimensions.
fn parse_embeddings(
    response: &Value,
    expected: usize,
) -> (Result<Vec<Vec<f32>>, MistralError>, Usage) {
    let usage = Usage {
        input_tokens: u64_at(response, "/usage/prompt_tokens"),
        output_tokens: 0,
        web_searches: 0,
    };
    let result = (|| {
        let data = response
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| MistralError::InvalidOutput("no data".into()))?;
        let mut indexed = Vec::with_capacity(data.len());
        for item in data {
            let index = item
                .get("index")
                .and_then(Value::as_u64)
                .ok_or_else(|| MistralError::InvalidOutput("embedding without index".into()))?;
            let vector: Vec<f32> = item
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| MistralError::InvalidOutput("embedding missing".into()))?
                .iter()
                .map(|x| x.as_f64().map(|x| x as f32))
                .collect::<Option<_>>()
                .ok_or_else(|| MistralError::InvalidOutput("non numeric embedding".into()))?;
            if vector.len() != EMBEDDING_DIMENSIONS {
                return Err(MistralError::InvalidOutput(format!(
                    "embedding of {} dimensions, {EMBEDDING_DIMENSIONS} expected",
                    vector.len()
                )));
            }
            indexed.push((index, vector));
        }
        indexed.sort_by_key(|(index, _)| *index);
        let indexes: Vec<u64> = indexed.iter().map(|(i, _)| *i).collect();
        if indexes != (0..expected as u64).collect::<Vec<_>>() {
            return Err(MistralError::InvalidOutput(format!(
                "{} embeddings for {expected} inputs",
                indexed.len()
            )));
        }
        Ok(indexed.into_iter().map(|(_, v)| v).collect())
    })();
    (result, usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::prompts::{PromptVars, test_prompts};
    use crate::config::{Config, Role};
    use crate::domain::types::Decision;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct SecondOpinion {
        decision: Decision,
        reason_code: String,
        explanation: String,
    }

    fn client(server: &MockServer) -> MistralClient {
        client_with(server, &[])
    }

    /// The test client, with extra variables.
    fn client_with(server: &MockServer, extra: &[(&str, &str)]) -> MistralClient {
        let base = server.uri();
        let config = Config::from_lookup(Role::Worker, |key| {
            if let Some((_, value)) = extra.iter().find(|(k, _)| *k == key) {
                return Some((*value).to_owned());
            }
            match key {
                "DATABASE_CONNECTION_STRING" => Some("postgres://unused/db".into()),
                "MISTRAL_API_KEY" => Some("test-key".into()),
                "MISTRAL_API_BASE_URL" => Some(base.clone()),
                "MISTRAL_REVIEW_MODEL" => Some("mistral-medium-latest".into()),
                _ => None,
            }
        })
        .unwrap();
        MistralClient::new(&config.mistral)
            .unwrap()
            .with_retry(RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_millis(5),
                max_delay: Duration::from_millis(50),
            })
            .with_timeouts(Timeouts::uniform(Duration::from_millis(500)))
    }

    fn second_opinion_prompt() -> RenderedPrompt {
        test_prompts()
            .second_opinion
            .render(
                &PromptVars::new()
                    .set("section", "idée")
                    .set("grid", "injection — ?")
                    .set("doubtful", "lawful")
                    .set("content", "Texte"),
            )
            .unwrap()
    }

    fn ctx() -> CallContext {
        CallContext {
            prompt_version: Some("1.0.0".into()),
            target: Some((TargetKind::Idea, Uuid::nil())),
            ..CallContext::new(AiPurpose::SecondOpinion)
        }
    }

    fn chat_response(content: Value) -> Value {
        json!({
            "id": "cmpl-1",
            "object": "chat.completion",
            "created": 1_760_000_000,
            "model": "mistral-medium-2508",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": content, "tool_calls": null },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 1_000, "completion_tokens": 200, "total_tokens": 1_200 }
        })
    }

    const ACCEPT: &str = r#"{"decision":"accept","reason_code":"none","explanation":"Le texte respecte la grille."}"#;

    async fn calls(pool: &PgPool) -> Vec<Value> {
        let (rows,): (Option<Value>,) =
            sqlx::query_as("SELECT jsonb_agg(to_jsonb(c) ORDER BY id) FROM ai_calls c")
                .fetch_one(pool)
                .await
                .unwrap();
        rows.and_then(|r| r.as_array().cloned()).unwrap_or_default()
    }

    async fn structured_call(
        client: &MistralClient,
        pool: &PgPool,
    ) -> Result<Completion<SecondOpinion>, MistralError> {
        let schema = test_prompts().second_opinion.output_schema.clone().unwrap();
        client
            .structured::<SecondOpinion>(
                pool,
                &ctx(),
                ModelTier::Review,
                &second_opinion_prompt(),
                &schema,
                |o| {
                    if o.decision == Decision::Accept && o.reason_code != "none" {
                        Err("accept needs reason_code none".into())
                    } else {
                        Ok(())
                    }
                },
            )
            .await
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn structured_output_request_and_recording(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({
                "model": "mistral-medium-latest",
                // MISTRAL_REVIEW_REASONING_EFFORT defaults to `none`.
                "reasoning_effort": "none",
                "response_format": {
                    "type": "json_schema",
                    "json_schema": { "name": "second_opinion", "strict": true,
                                     "schema": { "type": "object" } }
                },
                "messages": [{ "role": "system" }, { "role": "user" }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(json!(ACCEPT))))
            .expect(1)
            .mount(&server)
            .await;
        let completion = structured_call(&client(&server), &pool).await.unwrap();
        assert_eq!(completion.value.decision, Decision::Accept);
        assert_eq!(completion.model, "mistral-medium-2508");
        assert_eq!(completion.attempts, 1);
        // Review prices default to the analysis ones: 2 €/Mtok in, 6 €/Mtok out.
        assert_eq!(completion.cost, MicroEur(2_000 + 1_200));

        let calls = calls(&pool).await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["provider"], "mistral");
        assert_eq!(calls[0]["purpose"], "second_opinion");
        assert_eq!(calls[0]["model"], "mistral-medium-2508");
        assert_eq!(calls[0]["input_tokens"], 1_000);
        assert_eq!(calls[0]["output_tokens"], 200);
        assert_eq!(calls[0]["cost_micro_eur"], 3_200);
        assert_eq!(calls[0]["status"], "success");
        assert_eq!(calls[0]["prompt_version"], "1.0.0");
        assert_eq!(calls[0]["target_kind"], "idea");
        assert_eq!(calls[0]["http_status"], 200);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn chunked_content_is_joined(pool: PgPool) {
        let server = MockServer::start().await;
        let (head, tail) = ACCEPT.split_at(20);
        Mock::given(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response(json!([
                    { "type": "text", "text": head },
                    { "type": "text", "text": tail }
                ]))),
            )
            .mount(&server)
            .await;
        let completion = structured_call(&client(&server), &pool).await.unwrap();
        assert_eq!(completion.value.reason_code, "none");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn invalid_outputs_are_retried_then_fail(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response(json!("pas du JSON"))),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response(json!(
                    r#"{"decision":"accept","reason_code":"injection","explanation":"x"}"#
                ))),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response(json!(
                    r#"{"decision":"maybe","reason_code":"none","explanation":"x"}"#
                ))),
            )
            .mount(&server)
            .await;
        let err = structured_call(&client(&server), &pool).await.unwrap_err();
        assert!(
            matches!(&err, MistralError::InvalidOutput(m) if m.contains("second_opinion.schema.json")),
            "{err}"
        );
        assert!(err.is_transient());
        let calls = calls(&pool).await;
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|c| c["status"] == "invalid_output"));
        // Invalid answers are billed all the same.
        assert!(calls.iter().all(|c| c["cost_micro_eur"] == 3_200));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn oversized_answers_are_refused_before_parsing(pool: PgPool) {
        let server = MockServer::start().await;
        let mut huge = chat_response(json!(ACCEPT));
        huge["padding"] = json!("x".repeat(crate::ai::MAX_MISTRAL_RESPONSE_BYTES));
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(huge))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(json!(ACCEPT))))
            .mount(&server)
            .await;
        let completion = structured_call(&client(&server), &pool).await.unwrap();
        assert_eq!(completion.attempts, 2);
        let calls = calls(&pool).await;
        assert_eq!(calls[0]["status"], "invalid_output");
        assert_eq!(calls[1]["status"], "success");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn truncated_answer_is_invalid(pool: PgPool) {
        let server = MockServer::start().await;
        let mut truncated = chat_response(json!("{\"decision\":"));
        truncated["choices"][0]["finish_reason"] = json!("length");
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(truncated))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(json!(ACCEPT))))
            .mount(&server)
            .await;
        let completion = structured_call(&client(&server), &pool).await.unwrap();
        assert_eq!(completion.attempts, 2);
        assert_eq!(completion.usage.input_tokens, 2_000);
        assert_eq!(completion.cost, MicroEur(6_400));
        let calls = calls(&pool).await;
        assert_eq!(calls[0]["status"], "invalid_output");
        assert_eq!(calls[0]["error_code"], "invalid_output");
        assert_eq!(calls[1]["status"], "success");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn rate_limits_and_server_errors_are_retried(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_string("upstream detail"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(json!(ACCEPT))))
            .mount(&server)
            .await;
        let completion = structured_call(&client(&server), &pool).await.unwrap();
        assert_eq!(completion.attempts, 3);
        let calls = calls(&pool).await;
        let statuses: Vec<(&Value, &Value, &Value)> = calls
            .iter()
            .map(|c| (&c["status"], &c["http_status"], &c["cost_micro_eur"]))
            .collect();
        assert_eq!(
            statuses,
            vec![
                (&json!("error"), &json!(429), &json!(0)),
                (&json!("error"), &json!(503), &json!(0)),
                (&json!("success"), &json!(200), &json!(3_200)),
            ]
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn retries_are_bounded(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .expect(3)
            .mount(&server)
            .await;
        let err = structured_call(&client(&server), &pool).await.unwrap_err();
        assert_eq!(err, MistralError::Http { status: 500 });
        assert!(err.is_transient());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn client_errors_are_not_retried(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("{\"message\":\"bad\"}"))
            .expect(1)
            .mount(&server)
            .await;
        let err = structured_call(&client(&server), &pool).await.unwrap_err();
        assert_eq!(err, MistralError::Http { status: 400 });
        assert!(!err.is_transient());
        assert_eq!(calls(&pool).await[0]["error_code"], "http_400");
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn timeouts_are_recorded(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(chat_response(json!(ACCEPT)))
                    .set_delay(Duration::from_secs(2)),
            )
            .mount(&server)
            .await;
        let client = client(&server).with_retry(RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        });
        let err = structured_call(&client, &pool).await.unwrap_err();
        assert_eq!(err, MistralError::Timeout);
        assert_eq!(calls(&pool).await[0]["status"], "timeout");
    }

    /// BUD-1: an author may delete an idea while a paid call about it runs; the call must still
    /// be recorded (with `idea_id` NULL) so that the daily budget sees its cost.
    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn cost_of_a_call_for_a_vanished_idea_is_still_recorded(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(chat_response(json!(ACCEPT)))
                    .set_delay(Duration::from_millis(300)),
            )
            .expect(1)
            .mount(&server)
            .await;
        let idea = crate::test_support::idea_with_status(&pool, "second_opinion").await;
        let client = client(&server);
        let schema = test_prompts().second_opinion.output_schema.clone().unwrap();
        let ctx = CallContext {
            idea_id: Some(idea),
            target: Some((TargetKind::Idea, idea)),
            ..ctx()
        };
        let prompt = second_opinion_prompt();
        let call = client.structured::<SecondOpinion>(
            &pool,
            &ctx,
            ModelTier::Review,
            &prompt,
            &schema,
            |_| Ok(()),
        );
        let delete = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            sqlx::query("DELETE FROM ideas WHERE id = $1")
                .bind(idea)
                .execute(&pool)
                .await
                .unwrap();
        };
        let (completion, ()) = tokio::join!(call, delete);
        assert_eq!(completion.unwrap().cost, MicroEur(3_200));

        let calls = calls(&pool).await;
        assert_eq!(calls.len(), 1, "the paid call must be recorded");
        assert_eq!(calls[0]["cost_micro_eur"], 3_200);
        assert_eq!(calls[0]["idea_id"], Value::Null);
        let budget = crate::ai::budget::status(&pool, MicroEur(1_000_000))
            .await
            .unwrap();
        assert_eq!(budget.spent, MicroEur(3_200));
    }

    fn conversation_response() -> Value {
        json!({
            "object": "conversation.response",
            "conversation_id": "conv_1",
            "outputs": [
                {
                    "object": "entry", "type": "tool.execution", "name": "web_search",
                    "arguments": "{\"query\": \"achats hospitaliers\"}", "id": "t1",
                    "created_at": "2026-10-18T10:00:00Z", "completed_at": "2026-10-18T10:00:02Z"
                },
                {
                    "object": "entry", "type": "tool.execution", "name": "web_search",
                    "arguments": "{\"query\": \"cour des comptes achats\"}", "id": "t2"
                },
                {
                    "object": "entry", "type": "message.output", "role": "assistant",
                    "model": "mistral-large-2512", "id": "m1",
                    "content": [
                        { "type": "text", "text": "1. Mesure étudiée : mutualiser les achats [S1]." },
                        { "type": "tool_reference", "tool": "web_search",
                          "title": "La fonction achats des hôpitaux",
                          "url": "https://www.ccomptes.fr/fr/publications/achats-hopitaux",
                          "source": "brave" },
                        { "type": "text", "text": "\n2. Situation actuelle : programme PHARE." },
                        { "type": "tool_reference", "tool": "web_search",
                          "title": "La fonction achats des hôpitaux",
                          "url": "https://www.ccomptes.fr/fr/publications/achats-hopitaux" }
                    ]
                }
            ],
            "usage": {
                "prompt_tokens": 3_000, "completion_tokens": 800, "total_tokens": 9_800,
                "connector_tokens": 6_000, "connectors": { "web_search": 2 }
            }
        })
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn research_uses_the_conversations_api(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/conversations"))
            .and(body_partial_json(json!({
                "model": "mistral-large-latest",
                "tools": [{ "type": "web_search" }],
                // MISTRAL_ANALYSIS_REASONING_EFFORT defaults to `high`, inside completion_args.
                "completion_args": { "temperature": 0.3, "reasoning_effort": "high" },
                "store": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(conversation_response()))
            .expect(1)
            .mount(&server)
            .await;
        let prompt = test_prompts()
            .analysis_research
            .render(
                &PromptVars::new()
                    .set("today", "2026-10-18")
                    .set("idea_text", "Idée"),
            )
            .unwrap();
        let ctx = CallContext {
            prompt_version: Some(test_prompts().analysis_research.version.clone()),
            ..CallContext::new(AiPurpose::AnalysisResearch)
        };
        let completion = client(&server)
            .research(&pool, &ctx, ModelTier::Analysis, &prompt)
            .await
            .unwrap();
        let research = &completion.value;
        assert!(research.text.starts_with("1. Mesure étudiée"));
        assert!(research.text.ends_with("programme PHARE."));
        assert_eq!(research.references.len(), 1);
        assert_eq!(research.references[0].source.as_deref(), Some("brave"));
        assert_eq!(completion.model, "mistral-large-2512");
        assert_eq!(completion.usage.web_searches, 2);
        assert_eq!(completion.usage.input_tokens, 9_000);
        // 9 000 tokens at 2 €/Mtok + 800 at 6 €/Mtok + 2 searches at 0.03 €.
        assert_eq!(completion.cost, MicroEur(18_000 + 4_800 + 60_000));
        let calls = calls(&pool).await;
        assert_eq!(calls[0]["web_search_count"], 2);
        assert_eq!(calls[0]["purpose"], "analysis_research");
    }

    #[test]
    fn web_searches_counted_from_entries_or_connectors() {
        let mut response = conversation_response();
        response["usage"]["connectors"] = json!({ "web_search": 5 });
        assert_eq!(parse_conversation(&response).usage.web_searches, 5);
        response["usage"]["connectors"] = Value::Null;
        assert_eq!(parse_conversation(&response).usage.web_searches, 2);
        response["outputs"] = json!([{ "type": "message.output", "content": "  " }]);
        assert!(parse_conversation(&response).research.is_err());
        response["outputs"] = json!([{ "type": "message.output", "content": "Mémo." }]);
        assert_eq!(
            parse_conversation(&response).research.unwrap().text,
            "Mémo."
        );
    }

    /// Responses shaped after the Mistral API reference (`ThinkChunk`, `MessageOutputEntry`,
    /// `CompletionTokensDetails`), with and without reasoning. Their thinking holds the marker
    /// [`THINKING_MARKER`], which must never reach a result, a record or a log.
    fn fixture(name: &str) -> Value {
        let text = match name {
            "chat_high" => include_str!("../../tests/fixtures/mistral/chat_reasoning_high.json"),
            "chat_none" => include_str!("../../tests/fixtures/mistral/chat_reasoning_none.json"),
            "chat_malformed" => {
                include_str!("../../tests/fixtures/mistral/chat_reasoning_malformed.json")
            }
            "conversation_high" => {
                include_str!("../../tests/fixtures/mistral/conversation_reasoning_high.json")
            }
            "conversation_none" => {
                include_str!("../../tests/fixtures/mistral/conversation_reasoning_none.json")
            }
            "conversation_malformed" => {
                include_str!("../../tests/fixtures/mistral/conversation_reasoning_malformed.json")
            }
            other => panic!("unknown fixture {other}"),
        };
        serde_json::from_str(text).unwrap()
    }

    const THINKING_MARKER: &str = "PENSEE-PRIVEE";

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reasoning_chat_answers_are_read_from_their_text_chunks(pool: PgPool) {
        // (fixture, output tokens recorded): the reasoning tokens count as output.
        for (name, output_tokens) in [("chat_high", 500), ("chat_none", 200)] {
            sqlx::query("TRUNCATE ai_calls")
                .execute(&pool)
                .await
                .unwrap();
            let server = MockServer::start().await;
            Mock::given(path("/v1/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(fixture(name)))
                .expect(1)
                .mount(&server)
                .await;
            let (completion, logs) =
                crate::http::security_tests::capture_logs(structured_call(&client(&server), &pool))
                    .await;
            let completion = completion.unwrap();
            assert_eq!(completion.value.decision, Decision::Accept, "{name}");
            assert_eq!(completion.value.explanation, "Le texte respecte la grille.");
            assert_eq!(completion.usage.output_tokens, output_tokens, "{name}");
            // 1 000 input tokens at 2 €/Mtok, output at 6 €/Mtok.
            assert_eq!(completion.cost, MicroEur(2_000 + 6 * output_tokens as i64));
            let calls = calls(&pool).await;
            assert_eq!(calls[0]["output_tokens"], output_tokens, "{name}");
            assert_eq!(calls[0]["model"], "mistral-medium-3-5");
            let seen = format!("{completion:?} {} {logs}", Value::Array(calls));
            assert!(!seen.contains(THINKING_MARKER), "{name}: {seen}");
        }
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reasoning_chat_answer_without_text_is_invalid(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("chat_malformed")))
            .expect(3)
            .mount(&server)
            .await;
        let (result, logs) =
            crate::http::security_tests::capture_logs(structured_call(&client(&server), &pool))
                .await;
        // The thinking holds a valid answer: it must not be taken for one.
        let err = result.unwrap_err();
        assert_eq!(
            err,
            MistralError::InvalidOutput("no text in the answer".into())
        );
        let calls = calls(&pool).await;
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|c| c["status"] == "invalid_output"));
        let seen = format!("{err} {} {logs}", Value::Array(calls));
        assert!(!seen.contains(THINKING_MARKER), "{seen}");
    }

    #[test]
    fn reasoning_conversation_answers_keep_text_and_references_only() {
        let high = parse_conversation(&fixture("conversation_high"));
        let research = high.research.unwrap();
        assert_eq!(
            research.text,
            "1. Mesure étudiée : mutualiser les achats [S1].\n2. Situation actuelle : programme \
             PHARE [S2]."
        );
        // A page the web search returned while thinking is a page consulted, citable.
        let urls: Vec<_> = research
            .references
            .iter()
            .map(|r| r.url.as_deref().unwrap())
            .collect();
        assert_eq!(
            urls,
            [
                "https://www.economie.gouv.fr/dae/programme-phare",
                "https://www.ccomptes.fr/fr/publications/achats-hopitaux"
            ]
        );
        assert!(!format!("{research:?}").contains(THINKING_MARKER));
        assert_eq!(high.model.as_deref(), Some("mistral-large-4-0"));
        assert_eq!(
            high.usage,
            Usage {
                input_tokens: 9_000,
                output_tokens: 2_500,
                web_searches: 1
            }
        );

        let none = parse_conversation(&fixture("conversation_none"));
        let research = none.research.unwrap();
        assert_eq!(
            research.text,
            "1. Mesure étudiée : mutualiser les achats [S1]."
        );
        assert_eq!(research.references.len(), 1);
        assert_eq!(none.usage.output_tokens, 800);

        // Only thinking, and a text chunk without text: no memo, whatever the thinking says.
        let malformed = parse_conversation(&fixture("conversation_malformed"));
        assert_eq!(
            malformed.research.unwrap_err(),
            MistralError::InvalidOutput("empty research answer".into())
        );
        assert_eq!(malformed.usage.output_tokens, 4_000);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reasoning_research_is_recorded_without_its_thinking(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/conversations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("conversation_high")))
            .expect(1)
            .mount(&server)
            .await;
        let prompt = test_prompts()
            .analysis_research
            .render(
                &PromptVars::new()
                    .set("today", "2026-10-19")
                    .set("idea_text", "Idée"),
            )
            .unwrap();
        let ctx = CallContext::new(AiPurpose::AnalysisResearch);
        let client = client(&server);
        let (completion, logs) = crate::http::security_tests::capture_logs(client.research(
            &pool,
            &ctx,
            ModelTier::Analysis,
            &prompt,
        ))
        .await;
        let completion = completion.unwrap();
        // 9 000 tokens at 2 €/Mtok + 2 500 at 6 €/Mtok + 1 search at 0.03 €.
        assert_eq!(completion.cost, MicroEur(18_000 + 15_000 + 30_000));
        let calls = calls(&pool).await;
        assert_eq!(calls[0]["output_tokens"], 2_500);
        let seen = format!("{completion:?} {} {logs}", Value::Array(calls));
        assert!(!seen.contains(THINKING_MARKER), "{seen}");
    }

    async fn structured_with_tier(
        client: &MistralClient,
        pool: &PgPool,
        tier: ModelTier,
    ) -> Result<Completion<SecondOpinion>, MistralError> {
        let schema = test_prompts().second_opinion.output_schema.clone().unwrap();
        client
            .structured::<SecondOpinion>(
                pool,
                &ctx(),
                tier,
                &second_opinion_prompt(),
                &schema,
                |_| Ok(()),
            )
            .await
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn reasoning_effort_is_sent_per_tier(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/chat/completions"))
            .and(body_partial_json(json!({
                "model": "mistral-large-latest",
                "reasoning_effort": "high"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("chat_high")))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/chat/completions"))
            .and(body_partial_json(json!({
                "model": "mistral-medium-latest",
                "reasoning_effort": "high"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("chat_high")))
            .expect(1)
            .mount(&server)
            .await;
        let client = client_with(
            &server,
            &[
                ("MISTRAL_ANALYSIS_REASONING_EFFORT", "high"),
                ("MISTRAL_REVIEW_REASONING_EFFORT", "HIGH"),
            ],
        );
        assert_eq!(
            client.reasoning_effort(ModelTier::Review),
            Some(ReasoningEffort::High)
        );
        for tier in [ModelTier::Analysis, ModelTier::Review] {
            let completion = structured_with_tier(&client, &pool, tier).await.unwrap();
            assert_eq!(completion.value.decision, Decision::Accept, "{tier:?}");
        }
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn empty_reasoning_effort_is_omitted(pool: PgPool) {
        let server = MockServer::start().await;
        let without_effort = |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            body.get("reasoning_effort").is_none()
                && body.pointer("/completion_args/reasoning_effort").is_none()
        };
        Mock::given(path("/v1/chat/completions"))
            .and(without_effort)
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("chat_none")))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/v1/conversations"))
            .and(without_effort)
            .and(body_partial_json(
                json!({ "completion_args": { "temperature": 0.3 } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("conversation_none")))
            .expect(1)
            .mount(&server)
            .await;
        let client = client_with(
            &server,
            &[
                ("MISTRAL_ANALYSIS_REASONING_EFFORT", ""),
                ("MISTRAL_REVIEW_REASONING_EFFORT", ""),
            ],
        );
        assert_eq!(client.reasoning_effort(ModelTier::Analysis), None);
        structured_with_tier(&client, &pool, ModelTier::Review)
            .await
            .unwrap();
        let prompt = test_prompts()
            .analysis_research
            .render(
                &PromptVars::new()
                    .set("today", "2026-10-19")
                    .set("idea_text", "Idée"),
            )
            .unwrap();
        client
            .research(
                &pool,
                &CallContext::new(AiPurpose::AnalysisResearch),
                ModelTier::Analysis,
                &prompt,
            )
            .await
            .unwrap();
    }

    #[test]
    fn thinking_gets_longer_timeouts() {
        let timeouts = Timeouts::default();
        let high = Some(ReasoningEffort::High);
        assert_eq!(timeouts.chat(high), Duration::from_secs(360));
        assert_eq!(timeouts.conversation(high), Duration::from_secs(480));
        for effort in [None, Some(ReasoningEffort::Off)] {
            assert_eq!(timeouts.chat(effort), Duration::from_secs(180));
            assert_eq!(timeouts.conversation(effort), Duration::from_secs(300));
        }
        // The client-wide bound never cuts a request short.
        assert_eq!(timeouts.longest(), Duration::from_secs(480));
    }

    #[test]
    fn reasoning_tokens_always_count_as_output() {
        let usage = |usage: Value| json!({ "usage": usage });
        // Reported apart and left out of `completion_tokens`.
        let apart = usage(json!({
            "prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 150,
            "completion_tokens_details": { "reasoning_tokens": 30 }
        }));
        assert_eq!(output_tokens(&apart, 100), 50);
        // Not reported: `completion_tokens` already holds them.
        let within = usage(json!({
            "prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150
        }));
        assert_eq!(output_tokens(&within, 100), 50);
        // Only the total accounts for them.
        let total_only = usage(json!({
            "prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 150
        }));
        assert_eq!(output_tokens(&total_only, 100), 50);
        // Nothing usable: no tokens, never a panic.
        assert_eq!(output_tokens(&json!({}), 0), 0);
        assert_eq!(output_tokens(&usage(json!({ "total_tokens": 10 })), 100), 0);
    }

    #[test]
    fn answer_text_skips_reasoning() {
        assert_eq!(answer_text(&json!("{}")).as_deref(), Some("{}"));
        assert_eq!(
            answer_text(&json!([
                { "type": "thinking", "thinking": [{ "type": "text", "text": "secret" }] },
                { "type": "text", "text": "a" },
                { "type": "image_url", "image_url": "https://example.org/x.png" },
                { "type": "text", "text": "b" }
            ]))
            .as_deref(),
            Some("ab")
        );
        assert_eq!(
            answer_text(&json!([{ "type": "thinking", "thinking": [] }])),
            None
        );
        assert_eq!(answer_text(&json!([])), None);
        assert_eq!(answer_text(&Value::Null), None);
        assert_eq!(answer_text(&json!({ "text": "x" })), None);
    }

    fn embedding_response(dims: usize, indexes: &[u64]) -> Value {
        json!({
            "id": "emb-1", "object": "list", "model": "mistral-embed",
            "data": indexes.iter().map(|i| json!({
                "object": "embedding", "index": i,
                "embedding": vec![*i as f64 / 10.0; dims]
            })).collect::<Vec<_>>(),
            "usage": { "prompt_tokens": 1_000_000, "completion_tokens": 0, "total_tokens": 1_000_000 }
        })
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn embeddings_are_ordered_and_checked(pool: PgPool) {
        let server = MockServer::start().await;
        Mock::given(path("/v1/embeddings"))
            .and(body_partial_json(
                json!({ "model": "mistral-embed", "input": ["a", "b"] }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(embedding_response(EMBEDDING_DIMENSIONS, &[1, 0])),
            )
            .mount(&server)
            .await;
        let ctx = CallContext::new(AiPurpose::Embedding);
        let completion = client(&server)
            .embed(&pool, &ctx, &["a", "b"])
            .await
            .unwrap();
        assert_eq!(completion.value.len(), 2);
        assert_eq!(completion.value[0][0], 0.0);
        assert!((completion.value[1][0] - 0.1).abs() < 1e-6);
        // 0.1 €/Mtok.
        assert_eq!(completion.cost, MicroEur(100_000));
        assert_eq!(calls(&pool).await[0]["model"], "mistral-embed");
    }

    #[test]
    fn embeddings_with_wrong_shapes_are_invalid() {
        let (result, _) = parse_embeddings(&embedding_response(512, &[0]), 1);
        assert!(matches!(result, Err(MistralError::InvalidOutput(m)) if m.contains("512")));
        let (result, _) = parse_embeddings(&embedding_response(EMBEDDING_DIMENSIONS, &[0]), 2);
        assert!(result.is_err());
        let (result, _) = parse_embeddings(&json!({}), 1);
        assert!(result.is_err());
    }

    #[test]
    fn missing_api_key_is_reported() {
        let config = Config::from_lookup(Role::Worker, |key| {
            (key == "DATABASE_CONNECTION_STRING").then(|| "postgres://unused/db".to_owned())
        })
        .unwrap();
        assert_eq!(
            MistralClient::new(&config.mistral).unwrap_err(),
            MistralError::MissingApiKey
        );
    }

    #[test]
    fn retry_delays_are_bounded() {
        let policy = RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(10),
        };
        assert_eq!(policy.delay(1, None), Duration::from_secs(2));
        assert_eq!(policy.delay(2, None), Duration::from_secs(4));
        assert_eq!(policy.delay(4, None), Duration::from_secs(10));
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(7))),
            Duration::from_secs(7)
        );
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(3_600))),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn api_key_is_not_in_debug_output() {
        let server_uri = "http://127.0.0.1:1";
        let config = Config::from_lookup(Role::Worker, |key| match key {
            "DATABASE_CONNECTION_STRING" => Some("postgres://unused/db".into()),
            "MISTRAL_API_KEY" => Some("very-secret-key".into()),
            "MISTRAL_API_BASE_URL" => Some(server_uri.into()),
            _ => None,
        })
        .unwrap();
        let client = MistralClient::new(&config.mistral).unwrap();
        assert!(!format!("{client:?}").contains("very-secret-key"));
    }
}
