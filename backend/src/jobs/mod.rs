//! Background jobs: a PostgreSQL queue (`jobs` table) consumed by the worker process.
//!
//! - [`queue`]: enqueue, claim (`FOR UPDATE SKIP LOCKED`), complete, fail with exponential
//!   backoff, reclaim abandoned locks.
//! - [`worker`]: the consuming loop, its handler registry and graceful shutdown.
//! - [`schedule`]: daily recurring jobs, enqueued once whatever the number of replicas.
//! - [`moderation`]: screening retries and Mistral second opinions.
//! - [`analysis`]: Mistral analysis (research, structuring, score) and publication of ideas.
//! - [`dedup`]: duplicate detection of accepted ideas (embeddings, pgvector).
//! - [`guard`]: API key and daily budget checks of the handlers that call Mistral.
//! - [`pipeline`]: payloads, dedupe keys and constructors of the submission pipeline jobs.
//! - [`reports`]: grouped AI review of the reports of an idea, escalation of serious content.
//! - [`purge`]: the daily retention job (AGENTS.md §7.3).
//!
//! Payloads and error messages carry **internal identifiers only**, never user text, IPs or
//! tokens (§7.1): they are stored in clear and visible from the admin.

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::domain::text_enum;

pub mod analysis;
pub mod dedup;
pub mod guard;
pub mod health;
pub mod moderation;
pub mod pipeline;
pub mod purge;
pub mod queue;
pub mod reports;
pub mod schedule;
pub mod worker;

/// Queue priorities: a higher value is claimed first (AGENTS.md §6.4:
/// second opinion > report > new idea > contribution > admin relaunch).
pub mod priority {
    pub const SECOND_OPINION: i16 = 50;
    pub const REPORT: i16 = 40;
    pub const NEW_IDEA: i16 = 30;
    pub const CONTRIBUTION: i16 = 20;
    pub const ADMIN: i16 = 10;
    /// Housekeeping (purge, exports): cheap, runs when nothing else waits.
    pub const MAINTENANCE: i16 = 0;
}

/// Default number of attempts before a job is declared dead.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 5;

text_enum! {
    /// Every kind of job. Handlers are registered per kind in [`worker::Registry`].
    pub enum JobKind {
        /// Retry the screening of an idea when the decision service was unavailable.
        ScreenIdea => "screen_idea",
        /// Mistral second opinion for a screening in the gray zone.
        SecondOpinion => "second_opinion",
        /// Embedding and nearest-neighbour search of an accepted idea.
        DedupIdea => "dedup_idea",
        /// (Re-)analysis of an idea by Mistral.
        AnalyzeIdea => "analyze_idea",
        /// Retry the screening of a contribution.
        ScreenContribution => "screen_contribution",
        /// Retry the screening of a report.
        ScreenReport => "screen_report",
        /// Grouped AI review of the pending reports of an idea.
        ReviewReports => "review_reports",
        /// Regeneration of the open data exports.
        GenerateExports => "generate_exports",
        /// Daily retention purge.
        Purge => "purge",
    }
}

impl JobKind {
    /// Priority used when the caller does not choose one.
    pub const fn default_priority(self) -> i16 {
        match self {
            Self::SecondOpinion => priority::SECOND_OPINION,
            Self::ScreenReport | Self::ReviewReports => priority::REPORT,
            Self::ScreenIdea | Self::DedupIdea | Self::AnalyzeIdea => priority::NEW_IDEA,
            Self::ScreenContribution => priority::CONTRIBUTION,
            Self::GenerateExports | Self::Purge => priority::MAINTENANCE,
        }
    }

    /// Whether handling this kind calls the Mistral API (and therefore needs the API key and
    /// counts against the daily budget).
    pub const fn requires_mistral(self) -> bool {
        matches!(
            self,
            Self::SecondOpinion | Self::DedupIdea | Self::AnalyzeIdea | Self::ReviewReports
        )
    }
}

text_enum! {
    /// Job status (`jobs.status`).
    pub enum JobStatus {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        /// Failed permanently or exhausted its attempts.
        Dead => "dead",
    }
}

/// A job to enqueue.
#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub kind: JobKind,
    pub payload: Value,
    pub priority: i16,
    pub max_attempts: i32,
    pub run_after: Option<DateTime<Utc>>,
    /// At most one queued or running job may share this key.
    pub dedupe_key: Option<String>,
}

impl NewJob {
    pub fn new(kind: JobKind) -> Self {
        Self {
            kind,
            payload: Value::Object(serde_json::Map::new()),
            priority: kind.default_priority(),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            run_after: None,
            dedupe_key: None,
        }
    }

    /// Sets the payload; it must serialize to a JSON object.
    pub fn payload(mut self, payload: Value) -> Self {
        debug_assert!(payload.is_object(), "job payloads are JSON objects");
        self.payload = payload;
        self
    }

    pub fn priority(mut self, priority: i16) -> Self {
        self.priority = priority;
        self
    }

    pub fn max_attempts(mut self, max_attempts: i32) -> Self {
        self.max_attempts = max_attempts.max(1);
        self
    }

    pub fn run_after(mut self, at: DateTime<Utc>) -> Self {
        self.run_after = Some(at);
        self
    }

    pub fn dedupe_key(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }
}

/// A claimed job.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    #[sqlx(try_from = "String")]
    pub kind: JobKind,
    pub payload: Value,
    pub priority: i16,
    /// Attempts so far, **including** the current one.
    pub attempts: i32,
    pub max_attempts: i32,
    pub dedupe_key: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Job {
    /// Deserializes the payload; a malformed payload is a permanent failure.
    pub fn payload_as<T: DeserializeOwned>(&self) -> Result<T, JobError> {
        serde_json::from_value(self.payload.clone())
            .map_err(|err| JobError::Permanent(format!("invalid payload: {err}")))
    }
}

/// Successful result of a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Done,
    /// Run again later without consuming an attempt (e.g. daily AI budget reached).
    RetryAt(DateTime<Utc>),
}

/// Failure of a handler. Messages are stored in `jobs.last_error`: no user text in them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JobError {
    /// Worth retrying with backoff (network, 5xx, database hiccup…).
    #[error("{0}")]
    Transient(String),
    /// Retrying cannot help (invalid payload, missing entity…): the job dies immediately.
    #[error("{0}")]
    Permanent(String),
}

impl From<sqlx::Error> for JobError {
    fn from(err: sqlx::Error) -> Self {
        Self::Transient(format!("database error: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorities_follow_agents_md() {
        use JobKind::*;
        assert!(SecondOpinion.default_priority() > ReviewReports.default_priority());
        assert!(ReviewReports.default_priority() > AnalyzeIdea.default_priority());
        assert!(AnalyzeIdea.default_priority() > ScreenContribution.default_priority());
        assert!(ScreenContribution.default_priority() > priority::ADMIN);
        assert!(priority::ADMIN > Purge.default_priority());
    }

    #[test]
    fn mistral_kinds() {
        let mistral: Vec<_> = JobKind::ALL
            .iter()
            .filter(|k| k.requires_mistral())
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            mistral,
            [
                "second_opinion",
                "dedup_idea",
                "analyze_idea",
                "review_reports"
            ]
        );
    }

    #[test]
    fn kinds_match_the_sql_pattern() {
        for kind in JobKind::ALL {
            let s = kind.as_str();
            assert!(s.chars().next().unwrap().is_ascii_lowercase());
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            );
        }
    }

    #[test]
    fn new_job_builder() {
        let job = NewJob::new(JobKind::AnalyzeIdea)
            .payload(serde_json::json!({ "idea_id": "x" }))
            .priority(priority::ADMIN)
            .max_attempts(0)
            .dedupe_key("analyze:x");
        assert_eq!(job.priority, priority::ADMIN);
        assert_eq!(job.max_attempts, 1);
        assert_eq!(job.dedupe_key.as_deref(), Some("analyze:x"));
        assert_eq!(
            NewJob::new(JobKind::Purge).max_attempts,
            DEFAULT_MAX_ATTEMPTS
        );
    }

    #[test]
    fn payload_errors_are_permanent() {
        #[derive(Debug, serde::Deserialize)]
        struct Payload {
            #[allow(dead_code)]
            idea_id: uuid::Uuid,
        }
        let job = Job {
            id: 1,
            kind: JobKind::AnalyzeIdea,
            payload: serde_json::json!({ "idea_id": "not-a-uuid" }),
            priority: 0,
            attempts: 1,
            max_attempts: 5,
            dedupe_key: None,
            created_at: Utc::now(),
        };
        assert!(matches!(
            job.payload_as::<Payload>(),
            Err(JobError::Permanent(_))
        ));
    }
}
