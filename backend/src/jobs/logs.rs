//! Logs of each job, readable from the admin (AGENTS.md §8).
//!
//! The worker runs every job inside a [`JOB_SPAN`] span carrying its `job_id`
//! ([`job_span`]). [`JobLogLayer`] copies each log event emitted inside such a span (the same
//! lines as stdout, after the `RUST_LOG` filter) to a bounded channel, never blocking: a full
//! channel drops the line. [`run_writer`] stores them in batches in `job_logs`, at most
//! [`MAX_LINES_PER_JOB`] per job and per process. The lines hold what the logs already hold:
//! internal identifiers, codes and counts, never a visitor text, an IP or a token (§14.6).

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Name of the span of a running job.
pub const JOB_SPAN: &str = "job";
/// Lines stored per job and per worker process; one more line says when the rest is dropped.
pub const MAX_LINES_PER_JOB: u32 = 2_000;
/// Lines waiting for the writer; beyond, new lines are dropped (logging never blocks a job).
const CHANNEL_CAPACITY: usize = 10_000;
/// Lines inserted at once.
const BATCH: usize = 200;
/// Fields kept per line, and characters per value or message.
const MAX_FIELDS: usize = 32;
const MAX_VALUE_CHARS: usize = 2_000;
/// Lines returned by one read of the admin.
pub const MAX_READ: i64 = 500;

/// The span of a running job: its events go to the job logs.
pub fn job_span(job_id: i64, kind: &str, attempt: i32) -> tracing::Span {
    tracing::info_span!(JOB_SPAN, job_id, kind, attempt)
}

/// A log line of a job, on its way to `job_logs`.
#[derive(Debug, Clone, PartialEq)]
pub struct JobLogLine {
    pub job_id: i64,
    pub logged_at: DateTime<Utc>,
    pub level: &'static str,
    pub target: String,
    pub message: String,
    pub fields: Map<String, Value>,
}

/// Receiving end of [`JobLogLayer`], handed to [`run_writer`].
pub type JobLogReceiver = mpsc::Receiver<JobLogLine>;

/// Tracing layer copying the events of the job spans to a channel.
pub struct JobLogLayer {
    sender: mpsc::Sender<JobLogLine>,
}

impl JobLogLayer {
    pub fn new() -> (Self, JobLogReceiver) {
        let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
        (Self { sender }, receiver)
    }
}

/// `job_id` of a job span, kept in its extensions.
struct JobId(i64);

fn level_name(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "error",
        Level::WARN => "warn",
        Level::INFO => "info",
        Level::DEBUG => "debug",
        Level::TRACE => "trace",
    }
}

fn truncate(mut text: String) -> String {
    if let Some((cut, _)) = text.char_indices().nth(MAX_VALUE_CHARS) {
        text.truncate(cut);
        text.push('…');
    }
    text
}

/// Collects the fields of an event (or the `job_id` of a span).
#[derive(Default)]
struct Fields {
    message: Option<String>,
    fields: Map<String, Value>,
    job_id: Option<i64>,
}

impl Fields {
    fn insert(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = Some(match value {
                Value::String(text) => text,
                other => other.to_string(),
            });
        } else if self.fields.len() < MAX_FIELDS {
            self.fields.insert(field.name().to_owned(), value);
        }
    }
}

impl Visit for Fields {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "job_id" {
            self.job_id = Some(value);
        }
        self.insert(field, json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "job_id" {
            self.job_id = i64::try_from(value).ok();
        }
        self.insert(field, json!(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::String(truncate(value.to_owned())));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, Value::String(truncate(format!("{value:?}"))));
    }
}

impl<S> Layer<S> for JobLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().name() != JOB_SPAN {
            return;
        }
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let (Some(job_id), Some(span)) = (fields.job_id, ctx.span(id)) {
            span.extensions_mut().insert(JobId(job_id));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let Some(scope) = ctx.event_scope(event) else {
            return;
        };
        let Some(job_id) = scope
            .from_root()
            .find_map(|span| span.extensions().get::<JobId>().map(|job| job.0))
        else {
            return;
        };
        let mut fields = Fields::default();
        event.record(&mut fields);
        let metadata = event.metadata();
        // Never blocks: a full channel drops the line.
        let _ = self.sender.try_send(JobLogLine {
            job_id,
            logged_at: Utc::now(),
            level: level_name(metadata.level()),
            target: metadata.target().to_owned(),
            message: truncate(fields.message.unwrap_or_default()),
            fields: fields.fields,
        });
    }
}

/// Stores the lines of `receiver` in `job_logs` until every sender is gone.
pub async fn run_writer(pool: PgPool, mut receiver: JobLogReceiver) {
    let mut counts: HashMap<i64, u32> = HashMap::new();
    let mut batch = Vec::with_capacity(BATCH);
    while receiver.recv_many(&mut batch, BATCH).await > 0 {
        let lines = keep_within_limit(&mut counts, std::mem::take(&mut batch));
        if let Err(err) = insert(&pool, &lines).await {
            // Outside any job span: not copied to the job logs.
            tracing::warn!(error = %err, lines = lines.len(), "cannot store job logs");
        }
        // Finished jobs are not counted forever.
        if counts.len() > 10_000 {
            counts.clear();
        }
    }
}

/// The lines under [`MAX_LINES_PER_JOB`], plus one line per job that reaches it.
fn keep_within_limit(counts: &mut HashMap<i64, u32>, lines: Vec<JobLogLine>) -> Vec<JobLogLine> {
    let mut kept = Vec::with_capacity(lines.len());
    for line in lines {
        let count = counts.entry(line.job_id).or_default();
        *count = count.saturating_add(1);
        if *count <= MAX_LINES_PER_JOB {
            kept.push(line);
        } else if *count == MAX_LINES_PER_JOB + 1 {
            kept.push(JobLogLine {
                level: "warn",
                target: module_path!().to_owned(),
                message: format!(
                    "log limit reached ({MAX_LINES_PER_JOB} lines): next lines dropped"
                ),
                fields: Map::new(),
                ..line
            });
        }
    }
    kept
}

/// Inserts `lines`; those of a job deleted meanwhile are skipped.
pub async fn insert(pool: &PgPool, lines: &[JobLogLine]) -> sqlx::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let job_ids: Vec<i64> = lines.iter().map(|l| l.job_id).collect();
    let logged_at: Vec<DateTime<Utc>> = lines.iter().map(|l| l.logged_at).collect();
    let levels: Vec<&str> = lines.iter().map(|l| l.level).collect();
    let targets: Vec<&str> = lines.iter().map(|l| l.target.as_str()).collect();
    let messages: Vec<&str> = lines.iter().map(|l| l.message.as_str()).collect();
    let fields: Vec<Value> = lines
        .iter()
        .map(|l| Value::Object(l.fields.clone()))
        .collect();
    sqlx::query(
        "INSERT INTO job_logs (job_id, logged_at, level, target, message, fields)
         SELECT l.job_id, l.logged_at, l.level, l.target, l.message, l.fields
         FROM UNNEST($1::bigint[], $2::timestamptz[], $3::text[], $4::text[], $5::text[],
                     $6::jsonb[]) WITH ORDINALITY
              AS l(job_id, logged_at, level, target, message, fields, n)
         WHERE EXISTS (SELECT 1 FROM jobs WHERE jobs.id = l.job_id)
         ORDER BY l.n",
    )
    .bind(job_ids)
    .bind(logged_at)
    .bind(levels)
    .bind(targets)
    .bind(messages)
    .bind(fields)
    .execute(pool)
    .await?;
    Ok(())
}

/// A stored line.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct JobLogRow {
    pub id: i64,
    pub logged_at: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: Value,
}

/// The lines of a job after the line `after` (all from the start without it), oldest first,
/// at most `limit`.
pub async fn read(
    pool: &PgPool,
    job_id: i64,
    after: Option<i64>,
    limit: i64,
) -> sqlx::Result<Vec<JobLogRow>> {
    sqlx::query_as(
        "SELECT id, logged_at, level, target, message, fields FROM job_logs
         WHERE job_id = $1 AND id > $2 ORDER BY id LIMIT $3",
    )
    .bind(job_id)
    .bind(after.unwrap_or(0))
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobKind, NewJob, queue};
    use tracing::Instrument;
    use tracing_subscriber::layer::SubscriberExt as _;

    /// Events inside a job span (directly, in a child span, or in a task spawned in it) are
    /// copied with their level, target, message and fields; events outside are not.
    #[tokio::test]
    async fn events_of_a_job_span_are_copied() {
        let (layer, mut receiver) = JobLogLayer::new();
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!("before any job");
        async {
            tracing::info!(
                purpose = "analysis_research",
                attempt = 2,
                "Mistral call started"
            );
            let inner = tracing::info_span!("inner");
            inner.in_scope(|| tracing::warn!(error = %"timeout", "Mistral call failed"));
            tokio::spawn(
                async { tracing::error!(long = %"x".repeat(5_000), "spawned") }.in_current_span(),
            )
            .await
            .unwrap();
        }
        .instrument(job_span(42, "analyze_idea", 1))
        .await;
        tracing::info!("after the job");
        drop(_guard);

        let mut lines = Vec::new();
        while let Ok(line) = receiver.try_recv() {
            lines.push(line);
        }
        let summary: Vec<(i64, &str, &str)> = lines
            .iter()
            .map(|l| (l.job_id, l.level, l.message.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (42, "info", "Mistral call started"),
                (42, "warn", "Mistral call failed"),
                (42, "error", "spawned"),
            ]
        );
        assert_eq!(lines[0].fields["purpose"], "analysis_research");
        assert_eq!(lines[0].fields["attempt"], 2);
        assert_eq!(lines[1].fields["error"], "timeout");
        assert_eq!(lines[0].target, module_path!());
        let long = lines[2].fields["long"].as_str().unwrap();
        assert_eq!(long.chars().count(), MAX_VALUE_CHARS + 1);
    }

    fn line(job_id: i64, n: u32) -> JobLogLine {
        JobLogLine {
            job_id,
            logged_at: Utc::now(),
            level: "info",
            target: "t".into(),
            message: format!("line {n}"),
            fields: Map::new(),
        }
    }

    #[test]
    fn lines_beyond_the_limit_are_dropped_with_one_notice() {
        let mut counts = HashMap::new();
        let lines = (0..MAX_LINES_PER_JOB + 5).map(|n| line(1, n)).collect();
        let kept = keep_within_limit(&mut counts, lines);
        assert_eq!(kept.len(), MAX_LINES_PER_JOB as usize + 1);
        assert_eq!(kept.last().unwrap().level, "warn");
        assert!(kept.last().unwrap().message.contains("log limit reached"));
        // Another job keeps its own count.
        assert_eq!(keep_within_limit(&mut counts, vec![line(2, 0)]).len(), 1);
        assert!(keep_within_limit(&mut counts, vec![line(1, 0)]).is_empty());
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn lines_are_stored_and_read_in_order(pool: PgPool) {
        let job = queue::enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap()
            .unwrap();
        let mut first = line(job, 0);
        first.fields.insert("attempt".into(), json!(1));
        // A line of a job that no longer exists is skipped, the others are kept.
        insert(
            &pool,
            &[first, line(999_999, 0), line(job, 1), line(job, 2)],
        )
        .await
        .unwrap();
        let all = read(&pool, job, None, MAX_READ).await.unwrap();
        let messages: Vec<&str> = all.iter().map(|r| r.message.as_str()).collect();
        assert_eq!(messages, ["line 0", "line 1", "line 2"]);
        assert_eq!(all[0].fields, json!({ "attempt": 1 }));
        let next = read(&pool, job, Some(all[0].id), 1).await.unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].message, "line 1");
        assert!(
            read(&pool, job, Some(all[2].id), 10)
                .await
                .unwrap()
                .is_empty()
        );

        // Deleted with their job.
        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(job)
            .execute(&pool)
            .await
            .unwrap();
        let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM job_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn the_writer_stores_what_the_layer_sends(pool: PgPool) {
        let job = queue::enqueue(&pool, &NewJob::new(JobKind::Purge))
            .await
            .unwrap()
            .unwrap();
        let (layer, receiver) = JobLogLayer::new();
        let writer = tokio::spawn(run_writer(pool.clone(), receiver));
        {
            let subscriber = tracing_subscriber::registry().with(layer);
            let _guard = tracing::subscriber::set_default(subscriber);
            async { tracing::info!(count = 3, "purge done") }
                .instrument(job_span(job, "purge", 1))
                .await;
        }
        // The layer (the only sender) is dropped with its subscriber: the writer ends.
        writer.await.unwrap();
        let rows = read(&pool, job, None, MAX_READ).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message, "purge done");
        assert_eq!(rows[0].fields, json!({ "count": 3 }));
    }
}
