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
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer as _};

use crate::config::{LogConfig, LogFormat};
use crate::jobs::logs::{JobLogLayer, JobLogReceiver};

/// Installs the global tracing subscriber. Fails if the filter is invalid or if a subscriber
/// is already installed. With `job_logs` (the worker), the lines logged while a job runs are
/// also copied for the admin ([`crate::jobs::logs`]): the receiver goes to its writer.
pub fn init(log: &LogConfig, job_logs: bool) -> anyhow::Result<Option<JobLogReceiver>> {
    let filter = EnvFilter::try_new(&log.filter)
        .map_err(|err| anyhow!("RUST_LOG: invalid filter ({err})"))?;
    let stdout = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    let stdout = match log.format {
        LogFormat::Json => stdout
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .boxed(),
        LogFormat::Text => stdout.boxed(),
    };
    let (jobs, receiver) = if job_logs {
        let (layer, receiver) = JobLogLayer::new();
        (Some(layer), Some(receiver))
    } else {
        (None, None)
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(stdout)
        .with(jobs)
        .try_init()
        .map_err(|err| anyhow!("cannot install the log subscriber: {err}"))?;
    Ok(receiver)
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
