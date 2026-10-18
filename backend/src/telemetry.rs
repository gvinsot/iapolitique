//! Logging setup: JSON lines on stdout by default (`LOG_FORMAT=text` for local development).
//!
//! Privacy rules (AGENTS.md §7.1): never log IP addresses, tokens, identifying headers or the
//! text of ideas, contributions and reports. Log internal identifiers only. HTTP spans record
//! the matched route template (`/api/track/{token}`), never the concrete URI.

use anyhow::anyhow;
use tracing_subscriber::EnvFilter;

use crate::config::{LogConfig, LogFormat};

/// Installs the global tracing subscriber. Fails if the filter is invalid or if a subscriber
/// is already installed.
pub fn init(log: &LogConfig) -> anyhow::Result<()> {
    let filter = EnvFilter::try_new(&log.filter)
        .map_err(|err| anyhow!("RUST_LOG: invalid filter ({err})"))?;
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stdout);
    let result = match log.format {
        LogFormat::Json => builder
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .try_init(),
        LogFormat::Text => builder.try_init(),
    };
    result.map_err(|err| anyhow!("cannot install the log subscriber: {err}"))
}
