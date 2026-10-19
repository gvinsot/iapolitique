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

/// A Conversations API answer (research step) whose memo is `memo`, after two web searches,
/// with `prompt_tokens` input tokens.
pub fn research_response(memo: &str, prompt_tokens: u64) -> Value {
    json!({
        "object": "conversation.response",
        "conversation_id": "conv-test",
        "outputs": [
            { "object": "entry", "type": "tool.execution", "name": "web_search", "id": "t1" },
            { "object": "entry", "type": "tool.execution", "name": "web_search", "id": "t2" },
            {
                "object": "entry", "type": "message.output", "role": "assistant",
                "model": "mistral-large-test", "id": "m1",
                "content": [
                    { "type": "text", "text": memo },
                    // The pages cited by `analysis_output()`.
                    { "type": "tool_reference", "tool": "web_search", "title": "Source",
                      "url": "https://www.ccomptes.fr/fr/publications/la-fonction-achats-des-hopitaux" },
                    { "type": "tool_reference", "tool": "web_search", "title": "Source 2",
                      "url": "https://sante.gouv.fr/groupements-hospitaliers-de-territoire" }
                ]
            }
        ],
        "usage": {
            "prompt_tokens": prompt_tokens, "completion_tokens": 800,
            "total_tokens": prompt_tokens + 800, "connectors": { "web_search": 2 }
        }
    })
}

/// A valid output of `prompts/analysis.md` (`tests/fixtures/mistral/analysis.json`).
pub fn analysis_output() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/mistral/analysis.json"))
        .expect("valid fixture")
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

    /// The research step (Conversations API) answers `memo`.
    pub async fn research(&self, memo: &str) {
        let server = self.mistral.as_ref().expect("app built with Mistral");
        Mock::given(method("POST"))
            .and(path("/v1/conversations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(research_response(memo, 3_000)))
            .mount(server)
            .await;
    }

    /// Every structured chat completion answers `output`.
    pub async fn structured(&self, output: &Value) {
        let server = self.mistral.as_ref().expect("app built with Mistral");
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(output)))
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

/// Publication columns of an idea of the portal tests ([`listed_idea`]).
#[derive(Debug, Clone)]
pub struct IdeaSpec<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub summary: &'a str,
    pub admin_level: &'a str,
    pub category: &'a str,
    pub kind: &'a str,
    /// Territory name and INSEE code.
    pub territory: Option<(&'a str, &'a str)>,
    pub annual_gain_median_eur: i64,
    pub feasibility: i16,
    pub time_to_effect_months: i32,
    pub score_eur: i64,
    /// Days since the publication.
    pub published_days_ago: i32,
    pub status: &'a str,
}

impl Default for IdeaSpec<'_> {
    fn default() -> Self {
        Self {
            slug: "mutualiser-les-achats",
            title: "Mutualiser les achats courants des hôpitaux publics",
            summary: "Regrouper les achats courants des hôpitaux au sein de centrales régionales.",
            admin_level: "securite_sociale",
            category: "maladie",
            kind: "savings",
            territory: None,
            annual_gain_median_eur: 2_000_000,
            feasibility: 4,
            time_to_effect_months: 18,
            score_eur: 1_000_000,
            published_days_ago: 1,
            status: "published",
        }
    }
}

/// Inserts an idea with a complete current analysis (content: the analysis fixture) and the
/// publication columns of `spec`, then sets its status (`published` or `hidden`). Returns its
/// id.
pub async fn listed_idea(pool: &PgPool, spec: &IdeaSpec<'_>) -> Uuid {
    let idea: Uuid = sqlx::query_scalar(
        "INSERT INTO ideas (original_text, cgu_version, cgu_accepted_at)
         VALUES ('Regrouper les achats des hôpitaux. Contact : [donnée masquée]', '1.0',
                 current_date)
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let (territory_name, territory_code) = spec.territory.unzip();
    let analysis: Uuid = sqlx::query_scalar(
        "INSERT INTO analyses (
            idea_id, version, trigger, content, title, summary, kind, category, admin_level,
            territory_name, territory_code, legal_vehicle, annual_gain_low_eur,
            annual_gain_median_eur, annual_gain_high_eur, implementation_cost_low_eur,
            implementation_cost_median_eur, implementation_cost_high_eur,
            time_to_effect_months, feasibility, confidence, score_eur, net_annual_eur,
            feasibility_factor, confidence_factor, delay_factor, model, prompt_version,
            research_prompt_version, schema_version, scoring_version, web_search_count
         ) VALUES (
            $1, 1, 'initial', $2, $3, $4, $5, $6, $7, $8, $9, 'organisation_interne',
            $10 / 2, $10, $10 * 2, 0, 100000, 200000, $11, $12, 'medium', $13, $10 - 20000,
            0.8, 0.75, 0.57, 'mistral-large-latest', '1.0.0', '1.0.0', 'sha256:0123456789ab',
            '1', 2
         ) RETURNING id",
    )
    .bind(idea)
    .bind(analysis_output())
    .bind(spec.title)
    .bind(spec.summary)
    .bind(spec.kind)
    .bind(spec.category)
    .bind(spec.admin_level)
    .bind(territory_name)
    .bind(territory_code)
    .bind(spec.annual_gain_median_eur)
    .bind(spec.time_to_effect_months)
    .bind(spec.feasibility)
    .bind(spec.score_eur)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE ideas SET status = $3, slug = $2, title = a.title, summary = a.summary,
             current_analysis_id = a.id, admin_level = a.admin_level,
             territory_name = a.territory_name, territory_code = a.territory_code,
             category = a.category, kind = a.kind,
             annual_gain_median_eur = a.annual_gain_median_eur,
             implementation_cost_median_eur = a.implementation_cost_median_eur,
             time_to_effect_months = a.time_to_effect_months, feasibility = a.feasibility,
             confidence = a.confidence, score_eur = a.score_eur,
             net_annual_eur = a.net_annual_eur, feasibility_factor = a.feasibility_factor,
             confidence_factor = a.confidence_factor, delay_factor = a.delay_factor,
             scoring_version = a.scoring_version, last_analyzed_at = a.created_at,
             published_at = now() - make_interval(days => $4),
             first_published_at = now() - make_interval(days => $4),
             hidden_at = CASE WHEN $3 = 'hidden' THEN now() END,
             hidden_reason = CASE WHEN $3 = 'hidden' THEN 'admin' END
         FROM analyses a WHERE a.id = $5 AND ideas.id = $1",
    )
    .bind(idea)
    .bind(spec.slug)
    .bind(spec.status)
    .bind(spec.published_days_ago)
    .bind(analysis)
    .execute(pool)
    .await
    .unwrap();
    idea
}

/// GET on `router`: status, headers and JSON body (`Null` when empty or not JSON).
pub async fn get_json(
    router: &Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut request = Request::get(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

/// Calls the admin API of `app` (port 8081 router) with the admin token: status and JSON body
/// (`Null` when empty). `body` is sent as JSON when given.
pub async fn admin_call(
    app: &TestApp,
    method: axum::http::Method,
    uri: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let router = crate::http::admin_router(app.state.clone(), Secret::new(SECRET));
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {SECRET}"));
    let body = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// GET on the admin API with the token.
pub async fn admin_get(app: &TestApp, uri: &str) -> (StatusCode, Value) {
    admin_call(app, axum::http::Method::GET, uri, None).await
}

/// Actions recorded in `admin_actions`: (action, target id), oldest first.
pub async fn admin_actions(pool: &PgPool) -> Vec<(String, Option<String>)> {
    sqlx::query_as("SELECT action, target_id FROM admin_actions ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}
