//! Shared test harness: an application wired to mocked AI services (wiremock) and to the test
//! database of `#[sqlx::test]`. Never calls a real or paid service.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::ai::decision::{DecisionClient, Screener};
use crate::ai::mistral::{MistralClient, RetryPolicy, Timeouts};
use crate::ai::prompts::{CriterionKind, Prompts, test_prompts};
use crate::antispam::altcha::{Altcha, tests::solve};
use crate::antispam::rate_limit::{RateLimiter, RateLimits};
use crate::config::{Config, Role, Secret};
use crate::domain::types::TargetKind;
use crate::http::{AppState, public_router};
use crate::jobs::worker::JobContext;

pub const SECRET: &str = "0123456789abcdef0123456789abcdef";
/// Address of the test client (documentation range).
pub const CLIENT_IP: &str = "203.0.113.7";

/// `answers` of the decision service for every criterion of a section: each criterion passes
/// with probability 0.99, except those of `p_pass` (criterion key → probability of passing).
pub fn decision_answers(prompts: &Prompts, kind: TargetKind, p_pass: &[(&str, f64)]) -> Value {
    let mut answers = Map::new();
    for criterion in prompts.screening.section(kind) {
        let p = p_pass
            .iter()
            .find(|(key, _)| *key == criterion.key)
            .map_or(0.99, |(_, p)| *p);
        let answer = match &criterion.kind {
            CriterionKind::Noul { pass_when, .. } => {
                json!({ "type": "noul", "noul": if *pass_when { p } else { 1.0 - p } })
            }
            CriterionKind::Score { levels, pass_when } => {
                let mut probabilities = Map::new();
                for index in 0..levels.len() {
                    let value = if index == levels.len() - 1 {
                        p
                    } else if index == 0 && *pass_when > 0 {
                        1.0 - p
                    } else {
                        0.0
                    };
                    probabilities.insert(index.to_string(), json!(value));
                }
                json!({ "type": "score", "probabilities": probabilities })
            }
        };
        answers.insert(criterion.key.clone(), answer);
    }
    json!({
        "model": "d3-mini",
        "answers": answers,
        "usage": { "input_tokens": 900, "output_tokens": 0 }
    })
}

/// A chat completion whose content is `content` (serialized as JSON text).
pub fn chat_response(content: &Value) -> Value {
    json!({
        "id": "cmpl-test",
        "object": "chat.completion",
        "created": 1_760_000_000,
        "model": "mistral-medium-test",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content.to_string() },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1_000, "completion_tokens": 100, "total_tokens": 1_100 }
    })
}

/// Unit vector of `mistral-embed` dimensions in the plane (e0, e1), at angle `theta`: the cosine
/// similarity of two such vectors is the cosine of the difference of their angles.
pub fn unit_vector(theta: f64) -> Vec<f32> {
    let mut v = vec![0.0f32; crate::ai::mistral::EMBEDDING_DIMENSIONS];
    v[0] = theta.cos() as f32;
    v[1] = theta.sin() as f32;
    v
}

/// An embeddings response with one vector per input.
pub fn embedding_response(vectors: &[Vec<f32>]) -> Value {
    json!({
        "id": "embd-test",
        "object": "list",
        "model": "mistral-embed",
        "data": vectors.iter().enumerate().map(|(index, v)| json!({
            "object": "embedding", "index": index, "embedding": v
        })).collect::<Vec<_>>(),
        "usage": { "prompt_tokens": 40, "total_tokens": 40 }
    })
}

/// Configuration pointing at the mocks, with `extra` variables.
pub fn config(
    role: Role,
    decision_url: Option<&str>,
    mistral_url: Option<&str>,
    extra: &[(&str, &str)],
) -> Config {
    let decision = decision_url.map(|url| format!("{url}/v1/systemone"));
    Config::from_lookup(role, |key| {
        if let Some((_, value)) = extra.iter().find(|(k, _)| *k == key) {
            return Some((*value).to_owned());
        }
        match key {
            "DATABASE_CONNECTION_STRING" => Some("postgres://unused/db".to_owned()),
            "ADMIN_API_TOKEN" | "ALTCHA_HMAC_KEY" => Some(SECRET.to_owned()),
            "DECISION_SERVICE_URL" => decision.clone(),
            "DECISION_SERVICE_TIMEOUT_SECS" => Some("2".to_owned()),
            "MISTRAL_API_KEY" => mistral_url.map(|_| "test-key".to_owned()),
            "MISTRAL_API_BASE_URL" => mistral_url.map(str::to_owned),
            _ => None,
        }
    })
    .unwrap()
}

/// Mistral client of the tests: fast retries and short timeouts.
pub fn mistral_client(config: &Config) -> Option<MistralClient> {
    MistralClient::new(&config.mistral).ok().map(|client| {
        client
            .with_retry(RetryPolicy {
                max_attempts: 2,
                base_delay: Duration::from_millis(5),
                max_delay: Duration::from_millis(20),
            })
            .with_timeouts(Timeouts {
                chat: Duration::from_secs(5),
                conversation: Duration::from_secs(5),
                embedding: Duration::from_secs(5),
            })
    })
}

/// Application under test.
pub struct TestApp {
    pub pool: PgPool,
    pub state: AppState,
    pub router: Router,
    /// Mock of the decision service.
    pub decision: MockServer,
    /// Mock of the Mistral API, when the app has a key.
    pub mistral: Option<MockServer>,
}

impl TestApp {
    /// App with a decision service mock and, if `with_mistral`, a Mistral mock.
    pub async fn new(pool: PgPool, with_mistral: bool, extra: &[(&str, &str)]) -> Self {
        let decision = MockServer::start().await;
        let mistral = if with_mistral {
            Some(MockServer::start().await)
        } else {
            None
        };
        let config = config(
            Role::Api,
            Some(&decision.uri()),
            mistral.as_ref().map(MockServer::uri).as_deref(),
            extra,
        );
        let prompts = test_prompts();
        let state = AppState {
            pool: pool.clone(),
            screener: Screener::new(
                DecisionClient::new(&config.decision_service).unwrap(),
                Arc::clone(&prompts),
            ),
            altcha: Arc::new(Altcha::with_params(
                Secret::new(SECRET),
                500,
                Duration::from_secs(600),
            )),
            rate_limiter: Arc::new(RateLimiter::new(RateLimits::from(&config.antispam))),
            mistral: mistral_client(&config),
            config: Arc::new(config),
            prompts,
        };
        Self {
            router: public_router(state.clone()),
            pool,
            state,
            decision,
            mistral,
        }
    }

    /// Job context sharing the mocks of this app.
    pub fn job_context(&self) -> JobContext {
        JobContext {
            pool: self.pool.clone(),
            config: Arc::clone(&self.state.config),
            prompts: Arc::clone(&self.state.prompts),
            screener: self.state.screener.clone(),
            mistral: self.state.mistral.clone(),
        }
    }

    /// Answers every screening of `kind` with these probabilities of passing.
    pub async fn decision_answers(&self, kind: TargetKind, p_pass: &[(&str, f64)]) {
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(decision_answers(
                &self.state.prompts,
                kind,
                p_pass,
            )))
            .mount(&self.decision)
            .await;
    }

    /// The decision service answers with an error.
    pub async fn decision_down(&self) {
        Mock::given(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&self.decision)
            .await;
    }

    /// The Mistral second opinion answers `output`, after `delay`.
    pub async fn second_opinion(&self, output: Value, delay: Duration) {
        let server = self.mistral.as_ref().expect("app built with Mistral");
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(chat_response(&output))
                    .set_delay(delay),
            )
            .mount(server)
            .await;
    }

    /// The Mistral embeddings endpoint answers `vector` for every (single) input.
    pub async fn embedding(&self, vector: Vec<f32>) {
        let server = self.mistral.as_ref().expect("app built with Mistral");
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(embedding_response(&[vector])))
            .mount(server)
            .await;
    }

    /// A fresh ALTCHA solution.
    pub fn altcha(&self) -> String {
        solve(&self.state.altcha.create_challenge())
    }

    /// POST of a JSON body from [`CLIENT_IP`], raw response.
    pub async fn post_response(&self, uri: &str, body: &Value) -> axum::response::Response {
        let request = Request::post(uri)
            .header("content-type", "application/json")
            .header("x-forwarded-for", CLIENT_IP)
            .body(Body::from(body.to_string()))
            .unwrap();
        self.router.clone().oneshot(request).await.unwrap()
    }

    /// POST of a JSON body from [`CLIENT_IP`]: status and JSON body.
    pub async fn post(&self, uri: &str, body: &Value) -> (StatusCode, Value) {
        let response = self.post_response(uri, body).await;
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    /// Statuses recorded in the history of an entity, in order.
    pub async fn history(&self, kind: TargetKind, id: Uuid) -> Vec<String> {
        let sql = match kind {
            TargetKind::Idea => "SELECT to_status FROM idea_events WHERE idea_id = $1 ORDER BY id",
            TargetKind::Contribution => {
                "SELECT to_status FROM contribution_events WHERE contribution_id = $1 ORDER BY id"
            }
            TargetKind::Report => {
                "SELECT to_status FROM report_events WHERE report_id = $1 ORDER BY id"
            }
        };
        sqlx::query_as::<_, (String,)>(sql)
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|(s,)| s)
            .collect()
    }

    /// Queued or running jobs: (kind, dedupe key, run_after in the future).
    pub async fn jobs(&self) -> Vec<(String, Option<String>, bool)> {
        sqlx::query_as(
            "SELECT kind, dedupe_key, run_after > now() FROM jobs
             WHERE status IN ('queued', 'running') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

/// Inserts an idea in `status` (not one of the statuses that need more columns: published,
/// hidden, merged, purged) and returns its id.
pub async fn idea_with_status(pool: &PgPool, status: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO ideas (original_text, status, cgu_version, cgu_accepted_at)
         VALUES ('Mutualiser les achats de fournitures entre communes voisines.', $1, '1.0',
                 current_date)
         RETURNING id",
    )
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Inserts a published idea and returns its id (slug `slug`).
pub async fn published_idea(pool: &PgPool, slug: &str) -> Uuid {
    let (idea,): (Uuid,) = sqlx::query_as(
        "INSERT INTO ideas (original_text, cgu_version, cgu_accepted_at)
         VALUES ('Mutualiser les achats de fournitures entre communes voisines.', '1.0',
                 current_date)
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let (analysis,): (Uuid,) = sqlx::query_as(
        "INSERT INTO analyses (
            idea_id, version, trigger, content, title, summary, kind, category, admin_level,
            legal_vehicle, annual_gain_low_eur, annual_gain_median_eur, annual_gain_high_eur,
            implementation_cost_low_eur, implementation_cost_median_eur,
            implementation_cost_high_eur, time_to_effect_months, feasibility, confidence,
            score_eur, net_annual_eur, feasibility_factor, confidence_factor, delay_factor,
            model, prompt_version, schema_version, scoring_version
         ) VALUES (
            $1, 1, 'initial', '{}', 'Mutualiser les achats', 'Résumé neutre de l''idée.',
            'savings', 'achats_publics', 'commune', 'deliberation', 1000, 2000, 3000, 0, 500,
            1000, 12, 4, 'medium', 1000, 1900, 0.8, 0.75, 0.6667, 'mistral-large-latest', '1',
            '1', '1'
         ) RETURNING id",
    )
    .bind(idea)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE ideas SET status = 'published', slug = $2, title = 'Mutualiser les achats',
            summary = 'Résumé neutre de l''idée.', current_analysis_id = $3, score_eur = 1000,
            published_at = now(), first_published_at = now()
         WHERE id = $1",
    )
    .bind(idea)
    .bind(slug)
    .bind(analysis)
    .execute(pool)
    .await
    .unwrap();
    idea
}
