//! Logging setup: JSON lines on stdout by default (`LOG_FORMAT=text` for local development).
//!
//! Privacy rules (AGENTS.md §7.1): never log IP addresses, tokens, identifying headers or the
//! text of ideas, contributions and reports. Log internal identifiers only. HTTP spans record
//! the matched route template (`/api/track/{token}`), never the concrete URI.
//!
//! Panics are logged by [`log_panic`] (location only): the default hook would print the panic
//! message to stderr, outside the JSON format, and a message may quote request data, such as
//! the visitor text sliced by an out-of-bounds index (LOG-6).

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

/// Panic hook of the api and the worker: one `error` event with the location of the panic,
/// never its payload (which may quote request data).
pub fn log_panic(info: &std::panic::PanicHookInfo<'_>) {
    let location = info
        .location()
        .map(|l| format!("{}:{}", l.file(), l.line()))
        .unwrap_or_default();
    tracing::error!(target: "panic", %location, "panic");
}

/// Replaces the default panic hook by [`log_panic`].
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(log_panic));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LOG-6: a panic is logged with its location, never with its message.
    #[test]
    fn panics_are_logged_without_their_payload() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buffer {
            fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(data);
                Ok(data.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(move || writer.clone())
            .finish();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(log_panic));
        let result = tracing::subscriber::with_default(subscriber, || {
            std::panic::catch_unwind(|| {
                let text = String::from("Texte-secret du visiteur");
                panic!("bad slice of {text}");
            })
        });
        std::panic::set_hook(previous);
        assert!(result.is_err());
        let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("\"panic\""), "{logs}");
        assert!(logs.contains("telemetry.rs:"), "{logs}");
        assert!(!logs.contains("Texte-secret"), "{logs}");
    }
}
