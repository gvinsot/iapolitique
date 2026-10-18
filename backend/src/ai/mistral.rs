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
use crate::db::{self, ai_calls::NewAiCall};
use crate::domain::money::MicroEur;
use crate::domain::types::{AiCallStatus, AiProvider, AiPurpose, TargetKind};

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

/// Per-request timeouts: web research and long analyses take minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub chat: Duration,
    pub conversation: Duration,
    pub embedding: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            chat: Duration::from_secs(180),
            conversation: Duration::from_secs(300),
            embedding: Duration::from_secs(30),
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
    prices: MistralPrices,
    retry: RetryPolicy,
    timeouts: Timeouts,
}

impl MistralClient {
    /// Builds the client; `MISTRAL_API_KEY` is required.
    pub fn new(config: &MistralConfig) -> Result<Self, MistralError> {
        let api_key = config.api_key.clone().ok_or(MistralError::MissingApiKey)?;
        let timeouts = Timeouts::default();
        let longest = timeouts.conversation.max(timeouts.chat);
        let http = super::http_client(longest).map_err(|_| MistralError::Unreachable)?;
        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            api_key,
            analysis_model: config.analysis_model.clone(),
            review_model: config.review_model.clone(),
            embed_model: config.embed_model.clone(),
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
        let body = json!({
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
        self.run(pool, ctx, Some(tier), || async {
            let (response, http_status) = match self
                .post("/v1/chat/completions", &body, self.timeouts.chat)
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
        let body = json!({
            "model": self.model(tier),
            "instructions": prompt.system,
            "inputs": prompt.user,
            "tools": [{ "type": "web_search" }],
            "completion_args": { "temperature": RESEARCH_TEMPERATURE },
            "store": false,
            "stream": false,
        });
        self.run(pool, ctx, Some(tier), || async {
            let (response, http_status) = match self
                .post("/v1/conversations", &body, self.timeouts.conversation)
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
            db::ai_calls::insert(pool, &call).await?;
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
        match response.json::<Value>().await {
            Ok(value) => Ok((value, status)),
            Err(err) => Err(Failure {
                error: if err.is_timeout() {
                    MistralError::Timeout
                } else {
                    MistralError::InvalidOutput("body is not JSON".into())
                },
                http_status: Some(status),
                retry_after: None,
            }),
        }
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

struct ParsedChat {
    content: Result<String, MistralError>,
    usage: Usage,
    model: Option<String>,
}

/// `choices[0].message.content` (a string or text chunks) and `usage`.
fn parse_chat(response: &Value) -> ParsedChat {
    let usage = Usage {
        input_tokens: u64_at(response, "/usage/prompt_tokens"),
        output_tokens: u64_at(response, "/usage/completion_tokens"),
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
        match choice.pointer("/message/content") {
            Some(Value::String(text)) => Ok(text.clone()),
            Some(Value::Array(chunks)) => Ok(chunks
                .iter()
                .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|c| c.get("text").and_then(Value::as_str))
                .collect::<String>()),
            _ => Err(MistralError::InvalidOutput("no message content".into())),
        }
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
/// `tool.execution` entries and from `usage.connectors`, whichever is larger.
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
                match entry.get("content") {
                    Some(Value::String(s)) => text.push_str(s),
                    Some(Value::Array(chunks)) => {
                        for chunk in chunks {
                            match chunk.get("type").and_then(Value::as_str) {
                                Some("text") => {
                                    if let Some(s) = chunk.get("text").and_then(Value::as_str) {
                                        text.push_str(s);
                                    }
                                }
                                Some("tool_reference") => {
                                    let reference = WebReference {
                                        title: chunk
                                            .get("title")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                        url: chunk
                                            .get("url")
                                            .and_then(Value::as_str)
                                            .map(str::to_owned),
                                        source: chunk
                                            .get("source")
                                            .and_then(Value::as_str)
                                            .map(str::to_owned),
                                    };
                                    let known = references.iter().any(|r| {
                                        r.url.is_some() && r.url == reference.url
                                            || r.url.is_none() && r.title == reference.title
                                    });
                                    if !known {
                                        references.push(reference);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
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
    let usage = Usage {
        input_tokens: u64_at(response, "/usage/prompt_tokens")
            + u64_at(response, "/usage/connector_tokens"),
        output_tokens: u64_at(response, "/usage/completion_tokens"),
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
        let base = server.uri();
        let config = Config::from_lookup(Role::Worker, |key| match key {
            "DATABASE_CONNECTION_STRING" => Some("postgres://unused/db".into()),
            "MISTRAL_API_KEY" => Some("test-key".into()),
            "MISTRAL_API_BASE_URL" => Some(base.clone()),
            "MISTRAL_REVIEW_MODEL" => Some("mistral-medium-latest".into()),
            _ => None,
        })
        .unwrap();
        MistralClient::new(&config.mistral)
            .unwrap()
            .with_retry(RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_millis(5),
                max_delay: Duration::from_millis(50),
            })
            .with_timeouts(Timeouts {
                chat: Duration::from_millis(500),
                conversation: Duration::from_millis(500),
                embedding: Duration::from_millis(500),
            })
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
