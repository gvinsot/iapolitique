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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_tls_client() {
        http_client(Duration::from_secs(5)).unwrap();
        // A second installation is harmless.
        install_crypto_provider();
        http_client(Duration::from_secs(5)).unwrap();
    }
}
