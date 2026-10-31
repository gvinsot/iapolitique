//! Web search through an MCP server (AGENTS.md §6.1): the research step of an analysis gives
//! the model a `search_web` function; each call is run here as a JSON-RPC `tools/call` of the
//! configured search tool (`WEB_SEARCH_MCP_URL`, `WEB_SEARCH_MCP_TOOL`; PulsarTeam's browser
//! MCP by default) over the Streamable HTTP transport of MCP
//! (<https://modelcontextprotocol.io/specification/2025-06-18/basic/transports>).
//!
//! - The request goes straight to `tools/call`, as the default server accepts; a server that
//!   asks for a session first (HTTP 400/404, or a JSON-RPC error) gets the `initialize`
//!   handshake, then the call once more with its `Mcp-Session-Id`.
//! - The answer is JSON or a stream of server-sent events (`Accept` names both); only the
//!   `text` items of `result.content` are read.
//! - The text is Markdown: `[text](url)` links, a `#` heading line holding the title of a
//!   result, other lines its extracts. DuckDuckGo redirections (`duckduckgo.com/l/?uddg=…`)
//!   are unwrapped to the page they lead to; other DuckDuckGo links (ads) are dropped.
//!
//! Security (§14.1): the backend only calls the configured URL, never a page of the results
//! (the server searches, the backend reads its answer); the pages it returns are the only
//! citable ones (INJ-3), whatever the model writes. Privacy (§7.5): the query is written by the
//! model and never logged; the key is a secret.

use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::Secret;

/// Longest wait of one search.
pub const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest MCP response accepted (§14.5).
const MAX_RESPONSE_BYTES: usize = 1 << 20;
/// Longest query sent (characters).
pub const MAX_QUERY_CHARS: usize = 300;
/// Results kept per search, and the length of their fields given to the model.
const MAX_RESULTS: usize = 10;
const MAX_TITLE_CHARS: usize = 300;
const MAX_SNIPPET_CHARS: usize = 500;
const MAX_URL_CHARS: usize = 2_048;
/// Text given to the model when the answer holds no link (characters).
const MAX_RAW_TEXT_CHARS: usize = 4_000;
/// `MCP-Protocol-Version` sent after an `initialize`.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Settings of the search MCP server (`WEB_SEARCH_MCP_*`).
#[derive(Debug, Clone)]
pub struct McpSearchConfig {
    pub url: String,
    pub tool: String,
    pub api_key: Option<Secret>,
}

/// One result of a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    /// Empty when the server gave none (the site stands for it, §6.2).
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Why a search gave nothing. The `Display` never holds a server or query text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    #[error("search server unreachable")]
    Unreachable,
    #[error("search timed out")]
    Timeout,
    #[error("search server answered HTTP {0}")]
    Http(u16),
    #[error("search server response too large")]
    TooLarge,
    #[error("invalid search server response: {0}")]
    InvalidResponse(&'static str),
    #[error("search server error (JSON-RPC {0})")]
    Rpc(i64),
    #[error("the search tool reported an error")]
    ToolError,
}

impl SearchError {
    /// Short code for the logs.
    pub fn code(&self) -> String {
        match self {
            Self::Unreachable => "unreachable".into(),
            Self::Timeout => "timeout".into(),
            Self::Http(status) => format!("http_{status}"),
            Self::TooLarge => "too_large".into(),
            Self::InvalidResponse(_) => "invalid_response".into(),
            Self::Rpc(code) => format!("rpc_{code}"),
            Self::ToolError => "tool_error".into(),
        }
    }
}

/// Client of the search tool of an MCP server.
#[derive(Debug, Clone)]
pub struct McpSearch {
    http: reqwest::Client,
    url: String,
    tool: String,
    api_key: Option<Secret>,
}

/// A JSON-RPC answer, with the session the server opened (`initialize`).
struct RpcAnswer {
    result: Result<Value, SearchError>,
    session: Option<String>,
}

impl McpSearch {
    pub fn new(config: &McpSearchConfig) -> Result<Self, SearchError> {
        let http = super::http_client(SEARCH_TIMEOUT).map_err(|_| SearchError::Unreachable)?;
        Ok(Self {
            http,
            url: config.url.clone(),
            tool: config.tool.clone(),
            api_key: config.api_key.clone(),
        })
    }

    /// Name of the MCP tool called.
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// Runs one search: its results (at most [`MAX_RESULTS`]), and the text of an answer that
    /// holds no link (bounded), for the model.
    pub async fn search(&self, query: &str) -> Result<SearchAnswer, SearchError> {
        let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
        let call = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": self.tool, "arguments": { "query": query } },
        });
        let first = self.rpc(&call, None).await;
        let result = match first.result {
            Err(SearchError::Http(400 | 404) | SearchError::Rpc(_)) => {
                // The server may want a session first.
                let session = self.initialize().await?;
                self.rpc(&call, session.as_deref()).await.result?
            }
            other => other?,
        };
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(SearchError::ToolError);
        }
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .ok_or(SearchError::InvalidResponse("no content"))?
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(SearchAnswer::from_markdown(&text))
    }

    /// `initialize` then `notifications/initialized`: the session id, if the server gave one.
    async fn initialize(&self) -> Result<Option<String>, SearchError> {
        let init = json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "iapolitique", "version": env!("CARGO_PKG_VERSION") },
            },
        });
        let answer = self.rpc(&init, None).await;
        answer.result?;
        let notice = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        // A notification has no answer worth reading (202 Accepted).
        let _ = self.send(&notice, answer.session.as_deref()).await;
        Ok(answer.session)
    }

    async fn send(
        &self,
        body: &Value,
        session: Option<&str>,
    ) -> Result<reqwest::Response, SearchError> {
        let mut request = self
            .http
            .post(&self.url)
            .timeout(SEARCH_TIMEOUT)
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .json(body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key.expose());
        }
        if let Some(session) = session {
            request = request
                .header("Mcp-Session-Id", session)
                .header("MCP-Protocol-Version", PROTOCOL_VERSION);
        }
        request.send().await.map_err(|err| {
            if err.is_timeout() {
                SearchError::Timeout
            } else {
                SearchError::Unreachable
            }
        })
    }

    /// One JSON-RPC request with an `id`: its `result`, or why there is none.
    async fn rpc(&self, body: &Value, session: Option<&str>) -> RpcAnswer {
        let fail = |error| RpcAnswer {
            result: Err(error),
            session: None,
        };
        let response = match self.send(body, session).await {
            Ok(response) => response,
            Err(error) => return fail(error),
        };
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return fail(SearchError::Http(status));
        }
        let new_session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.len() <= 200 && v.bytes().all(|b| b.is_ascii_graphic()))
            .map(str::to_owned);
        let event_stream = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let bytes = match super::read_capped(response, MAX_RESPONSE_BYTES).await {
            Ok(bytes) => bytes,
            Err(super::BodyError::TooLarge) => return fail(SearchError::TooLarge),
            Err(super::BodyError::Transport(err)) => {
                return fail(if err.is_timeout() {
                    SearchError::Timeout
                } else {
                    SearchError::Unreachable
                });
            }
        };
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        RpcAnswer {
            result: rpc_result(&bytes, event_stream, &id),
            session: new_session.or_else(|| session.map(str::to_owned)),
        }
    }
}

/// The `result` of the JSON-RPC answer to `id`, in a JSON body or among server-sent events.
fn rpc_result(body: &[u8], event_stream: bool, id: &Value) -> Result<Value, SearchError> {
    let messages: Vec<Value> = if event_stream {
        let text = std::str::from_utf8(body).map_err(|_| SearchError::InvalidResponse("utf-8"))?;
        // Events are separated by a blank line; their `data:` lines join with a line feed.
        text.replace("\r\n", "\n")
            .split("\n\n")
            .filter_map(|event| {
                let data: Vec<&str> = event
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(|data| data.strip_prefix(' ').unwrap_or(data))
                    .collect();
                (!data.is_empty())
                    .then(|| serde_json::from_str(&data.join("\n")).ok())
                    .flatten()
            })
            .collect()
    } else {
        match serde_json::from_slice(body).map_err(|_| SearchError::InvalidResponse("json"))? {
            Value::Array(batch) => batch,
            message => vec![message],
        }
    };
    let answer = messages
        .into_iter()
        .find(|m| {
            m.get("id") == Some(id) && (m.get("result").is_some() || m.get("error").is_some())
        })
        .ok_or(SearchError::InvalidResponse("no answer"))?;
    if let Some(error) = answer.get("error") {
        return Err(SearchError::Rpc(
            error.get("code").and_then(Value::as_i64).unwrap_or(0),
        ));
    }
    Ok(answer["result"].clone())
}

/// What a search gave: its results, and the bounded text of an answer without any link.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchAnswer {
    pub results: Vec<SearchResult>,
    pub raw_text: Option<String>,
}

impl SearchAnswer {
    /// Results of a Markdown answer ([module documentation](self)).
    pub fn from_markdown(text: &str) -> Self {
        let link = Regex::new(r"\[([^\[\]]*)\]\(([^()\s]+)\)").expect("valid regex");
        let mut results: Vec<SearchResult> = Vec::new();
        for line in text.lines() {
            let heading = line.trim_start().starts_with('#');
            for capture in link.captures_iter(line) {
                let Some(url) = page_url(&capture[2]) else {
                    continue;
                };
                let label = clean(&capture[1], usize::MAX);
                let index = match results.iter().position(|r| r.url == url) {
                    Some(index) => index,
                    None if results.len() < MAX_RESULTS => {
                        results.push(SearchResult {
                            title: String::new(),
                            url,
                            snippet: String::new(),
                        });
                        results.len() - 1
                    }
                    None => continue,
                };
                let result = &mut results[index];
                if label.is_empty() {
                    continue;
                }
                if heading && result.title.is_empty() {
                    result.title = clean(&label, MAX_TITLE_CHARS);
                } else if !heading && result.snippet.chars().count() < MAX_SNIPPET_CHARS {
                    let joined = if result.snippet.is_empty() {
                        label
                    } else {
                        format!("{} {label}", result.snippet)
                    };
                    result.snippet = clean(&joined, MAX_SNIPPET_CHARS);
                }
            }
        }
        let raw_text = (results.is_empty() && !text.trim().is_empty())
            .then(|| clean(text, MAX_RAW_TEXT_CHARS));
        Self { results, raw_text }
    }

    /// The answer as given to the model: one block per result, never an instruction.
    pub fn for_model(&self) -> String {
        if self.results.is_empty() {
            return match &self.raw_text {
                Some(text) => {
                    format!("Aucun lien dans la réponse de l'outil. Texte reçu :\n{text}")
                }
                None => "Aucun résultat.".to_owned(),
            };
        }
        self.results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let title = if r.title.is_empty() {
                    "(sans titre)"
                } else {
                    &r.title
                };
                format!(
                    "{}. {title}\n   adresse : {}\n   extrait : {}",
                    i + 1,
                    r.url,
                    if r.snippet.is_empty() {
                        "(aucun)"
                    } else {
                        &r.snippet
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The page a result link leads to: the link itself (http or https), or the target of a
/// DuckDuckGo redirection; `None` for other DuckDuckGo links (ads, internal pages), other
/// schemes and over-long addresses.
fn page_url(link: &str) -> Option<String> {
    let parsed = url::Url::parse(link).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let target = if host == "duckduckgo.com" || host.ends_with(".duckduckgo.com") {
        if !parsed.path().starts_with("/l/") {
            return None;
        }
        let target = parsed
            .query_pairs()
            .find(|(key, _)| key == "uddg")
            .map(|(_, value)| value.into_owned())?;
        url::Url::parse(&target).ok()?
    } else {
        parsed
    };
    let host = target.host_str()?.to_ascii_lowercase();
    let usable = matches!(target.scheme(), "http" | "https")
        && !(host == "duckduckgo.com" || host.ends_with(".duckduckgo.com"))
        && target.as_str().len() <= MAX_URL_CHARS;
    usable.then(|| target.to_string())
}

/// `text` on one line: control, bidi and invisible characters become spaces, runs of spaces
/// one space, cut to `max` characters.
fn clean(text: &str, max: usize) -> String {
    let invisible = |c: char| {
        c.is_control()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
    };
    let spaced: String = text
        .chars()
        .map(|c| if invisible(c) { ' ' } else { c })
        .collect();
    let words = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() > max {
        words.chars().take(max).collect::<String>() + "…"
    } else {
        words
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// The answer of PulsarTeam's `search_web` (2026-10-31), shortened.
    pub(crate) const PULSARTEAM_ANSWER: &str = "# Search Results for: achats hospitaliers\n\n\
[Les établissements publics de santé : 25 Md€ de dépenses par an.](https://duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.ccomptes.fr%2Ffr%2Fpublications%2Fles%2Dachats%2Dhospitaliers&rut=e7)\n\
##  [La Cour des comptes pointe le manque de performance des achats hospitaliers](https://duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.lagazettedescommunes.com%2F41936%2Fachats%2F&rut=f9)\n\
[Dans son rapport, la Cour déplore la faible mutualisation.](https://duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.lagazettedescommunes.com%2F41936%2Fachats%2F&rut=f9)\n\
[Annonce](https://duckduckgo.com/y.js?ad_domain=example.com)\n\
## [Lien direct](https://www.vie-publique.fr/rapport/267888)\n\
[Lien piégé](javascript:alert(1))\n";

    fn client(server: &MockServer, key: Option<&str>) -> McpSearch {
        McpSearch::new(&McpSearchConfig {
            url: format!("{}/api/mcp/browser", server.uri()),
            tool: "search_web".into(),
            api_key: key.map(Secret::new),
        })
        .unwrap()
    }

    fn tool_answer(text: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1,
                "result": { "content": [{ "type": "text", "text": text }], "isError": false } })
    }

    #[test]
    fn markdown_results_are_grouped_and_unwrapped() {
        let answer = SearchAnswer::from_markdown(PULSARTEAM_ANSWER);
        assert_eq!(
            answer.results,
            [
                SearchResult {
                    title: String::new(),
                    url: "https://www.ccomptes.fr/fr/publications/les-achats-hospitaliers".into(),
                    snippet: "Les établissements publics de santé : 25 Md€ de dépenses par an."
                        .into(),
                },
                SearchResult {
                    title: "La Cour des comptes pointe le manque de performance des achats \
                            hospitaliers"
                        .into(),
                    url: "https://www.lagazettedescommunes.com/41936/achats/".into(),
                    snippet: "Dans son rapport, la Cour déplore la faible mutualisation.".into(),
                },
                SearchResult {
                    title: "Lien direct".into(),
                    url: "https://www.vie-publique.fr/rapport/267888".into(),
                    snippet: String::new(),
                },
            ]
        );
        assert_eq!(answer.raw_text, None);
        let text = answer.for_model();
        assert!(text.starts_with("1. (sans titre)\n   adresse : https://www.ccomptes.fr/"));
        assert!(!text.contains("duckduckgo") && !text.contains("javascript"));
    }

    #[test]
    fn results_are_bounded_and_on_one_line() {
        let many: String = (0..30)
            .map(|i| format!("## [Titre\u{202E} {i}](https://example.org/{i})\n"))
            .collect();
        let answer = SearchAnswer::from_markdown(&many);
        assert_eq!(answer.results.len(), MAX_RESULTS);
        assert!(answer.results.iter().all(|r| !r.title.contains('\u{202E}')));
        let long = format!("[{}](https://example.org/x)", "a".repeat(5_000));
        let answer = SearchAnswer::from_markdown(&long);
        assert_eq!(
            answer.results[0].snippet.chars().count(),
            MAX_SNIPPET_CHARS + 1
        );
        // No link: the text, bounded, and no page.
        let answer = SearchAnswer::from_markdown(&"x".repeat(10_000));
        assert!(answer.results.is_empty());
        assert_eq!(
            answer.raw_text.unwrap().chars().count(),
            MAX_RAW_TEXT_CHARS + 1
        );
        assert_eq!(
            SearchAnswer::from_markdown("  ").for_model(),
            "Aucun résultat."
        );
    }

    #[test]
    fn only_web_pages_are_kept() {
        assert_eq!(page_url("https://duckduckgo.com/y.js?u3=x"), None);
        assert_eq!(
            page_url("https://duckduckgo.com/l/?uddg=javascript%3Aalert(1)"),
            None
        );
        assert_eq!(page_url("ftp://example.org/x"), None);
        assert_eq!(
            page_url("https://html.duckduckgo.com/l/?uddg=http%3A%2F%2Fexample.org%2Fa%3Fb%3D1"),
            Some("http://example.org/a?b=1".into())
        );
        assert_eq!(
            page_url(&format!("https://e.org/{}", "a".repeat(3_000))),
            None
        );
    }

    #[test]
    fn json_and_event_stream_answers_are_read() {
        let id = json!(1);
        let answer = tool_answer("ok");
        assert_eq!(
            rpc_result(answer.to_string().as_bytes(), false, &id).unwrap()["content"][0]["text"],
            "ok"
        );
        let stream = format!(
            "event: message\ndata: {}\n\nevent: message\ndata: {}\n\n",
            json!({ "jsonrpc": "2.0", "method": "notifications/progress" }),
            answer
        );
        assert_eq!(
            rpc_result(stream.as_bytes(), true, &id).unwrap()["content"][0]["text"],
            "ok"
        );
        let error =
            json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32602, "message": "x" } });
        assert_eq!(
            rpc_result(error.to_string().as_bytes(), false, &id),
            Err(SearchError::Rpc(-32602))
        );
        assert_eq!(
            rpc_result(b"not json", false, &id),
            Err(SearchError::InvalidResponse("json"))
        );
    }

    #[tokio::test]
    async fn search_calls_the_tool_with_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/mcp/browser"))
            .and(header("authorization", "Bearer secret-key"))
            .and(body_partial_json(json!({
                "jsonrpc": "2.0", "method": "tools/call",
                "params": { "name": "search_web", "arguments": { "query": "achats" } },
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_answer(PULSARTEAM_ANSWER)))
            .expect(1)
            .mount(&server)
            .await;
        let answer = client(&server, Some("secret-key"))
            .search("achats")
            .await
            .unwrap();
        assert_eq!(answer.results.len(), 3);
        assert!(!format!("{:?}", client(&server, Some("secret-key"))).contains("secret-key"));
    }

    #[tokio::test]
    async fn a_server_asking_for_a_session_gets_the_handshake() {
        let server = MockServer::start().await;
        // Without a session the call is refused…
        Mock::given(body_partial_json(json!({ "method": "tools/call" })))
            .and(header("mcp-session-id", "s-1"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    "event: message\ndata: {}\n\n",
                    tool_answer(PULSARTEAM_ANSWER)
                ),
                "text/event-stream",
            ))
            .with_priority(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(body_partial_json(json!({ "method": "tools/call" })))
            .respond_with(ResponseTemplate::new(400))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(body_partial_json(json!({ "method": "initialize" })))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("mcp-session-id", "s-1")
                    .set_body_json(json!({ "jsonrpc": "2.0", "id": 0, "result": {} })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(body_partial_json(
            json!({ "method": "notifications/initialized" }),
        ))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;
        let answer = client(&server, None).search("achats").await.unwrap();
        assert_eq!(answer.results.len(), 3);
    }

    #[tokio::test]
    async fn search_failures_are_reported_without_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("API key required"))
            .mount(&server)
            .await;
        let error = client(&server, None).search("achats").await.unwrap_err();
        assert_eq!(error, SearchError::Http(401));
        assert_eq!(error.code(), "http_401");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "content": [{ "type": "text", "text": "quota" }], "isError": true },
            })))
            .mount(&server)
            .await;
        assert_eq!(
            client(&server, None).search("achats").await,
            Err(SearchError::ToolError)
        );
    }
}
