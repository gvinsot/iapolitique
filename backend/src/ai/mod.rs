//! AI layer (AGENTS.md §5.3, §6, §13.4).
//!
//! - [`prompts`]: the versioned files of `prompts/`, loaded and validated at startup, rendered
//!   with their variables, and exposed for the public methodology page.
//! - [`budget`]: daily AI budget (Europe/Paris day), postponing Mistral jobs once reached.
//! - [`decision`]: client of the decision model (`POST /v1/systemone`) and screening rules.
//! - [`mistral`]: Mistral client (structured outputs, web research, embeddings) with retries,
//!   cost estimation and `ai_calls` records.
//! - [`outputs`]: typed Mistral outputs and the checks a JSON schema cannot express.

use std::time::Duration;

pub mod budget;
pub mod decision;
pub mod mistral;
pub mod outputs;
pub mod prompts;

/// Installs `ring` as the process-wide rustls crypto provider (reqwest is built without one, so
/// that a single TLS stack is compiled). Idempotent.
pub fn install_crypto_provider() {
    // Fails only when a provider is already installed, which is what we want.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// HTTP client for the AI services: rustls with the system roots, overall `timeout` per request.
pub fn http_client(timeout: Duration) -> reqwest::Result<reqwest::Client> {
    install_crypto_provider();
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_secs(10)))
        .user_agent(concat!("iapolitique/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Largest Mistral response accepted (§14.5): a full analysis with sources is a few tens of
/// kilobytes; anything far beyond is refused before parsing.
pub const MAX_MISTRAL_RESPONSE_BYTES: usize = 2 << 20;
/// Largest decision service response accepted: answers to a dozen questions.
pub const MAX_DECISION_RESPONSE_BYTES: usize = 256 << 10;

/// Why a response body could not be read.
#[derive(Debug)]
pub enum BodyError {
    /// Larger than the accepted maximum (announced or received).
    TooLarge,
    /// Transport error (timeout, connection reset…).
    Transport(reqwest::Error),
}

/// Reads a response body of at most `max` bytes, without buffering more than that: an announced
/// `Content-Length` above it is refused at once, a body that grows beyond it is cut short.
pub async fn read_capped(
    mut response: reqwest::Response,
    max: usize,
) -> Result<Vec<u8>, BodyError> {
    if response
        .content_length()
        .is_some_and(|length| length > max as u64)
    {
        return Err(BodyError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(BodyError::Transport)? {
        if body.len() + chunk.len() > max {
            return Err(BodyError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bodies_are_read_up_to_a_maximum() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(path("/small"))
            .respond_with(ResponseTemplate::new(200).set_body_string("0123456789"))
            .mount(&server)
            .await;
        Mock::given(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(10_000)))
            .mount(&server)
            .await;
        let client = http_client(Duration::from_secs(5)).unwrap();
        let get = |p: &str| client.get(format!("{}{p}", server.uri())).send();
        let body = read_capped(get("/small").await.unwrap(), 10).await.unwrap();
        assert_eq!(body, b"0123456789");
        assert!(matches!(
            read_capped(get("/small").await.unwrap(), 9).await,
            Err(BodyError::TooLarge)
        ));
        assert!(matches!(
            read_capped(get("/big").await.unwrap(), 9_999).await,
            Err(BodyError::TooLarge)
        ));
    }

    #[test]
    fn builds_a_tls_client() {
        http_client(Duration::from_secs(5)).unwrap();
        // A second installation is harmless.
        install_crypto_provider();
        http_client(Duration::from_secs(5)).unwrap();
    }
}
