//! Jobs of the submission pipeline (AGENTS.md §5): payload shapes, dedupe keys and
//! constructors, shared by the api (which enqueues them) and the worker (which handles them).
//!
//! | Kind | Payload | Dedupe key |
//! |---|---|---|
//! | `screen_idea`, `screen_contribution`, `screen_report` | [`TargetPayload`] | `screen:<kind>:<id>` |
//! | `second_opinion` | [`TargetPayload`] | `second_opinion:<kind>:<id>` |
//! | `dedup_idea` | [`IdeaPayload`] | `dedup_idea:<idea id>` |
//! | `analyze_idea` | [`AnalyzePayload`] | `analyze_idea:<idea id>` |
//! | `review_reports` | [`IdeaPayload`] | `review_reports:<idea id>` |
//! | `generate_exports` | `{}` | `generate_exports` (debounced, [`EXPORTS_DEBOUNCE`]); none when urgent ([`regenerate_exports_now`]) |
//!
//! The dedupe keys group the requests: a queued analysis of an idea absorbs the next
//! contributions, a queued review absorbs the next reports (§5.6).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{JobKind, NewJob, priority, queue};
use crate::domain::types::{AnalysisTrigger, TargetKind};

/// Attempts of a screening retry: the backoff is capped at one hour, so a decision service
/// outage of about half a day is absorbed before the job dies (admin).
pub const SCREEN_MAX_ATTEMPTS: i32 = 20;

/// Payload of the screening retries and of the second opinions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetPayload {
    pub target_kind: TargetKind,
    pub target_id: Uuid,
}

/// Payload of the jobs about an idea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeaPayload {
    pub idea_id: Uuid,
}

/// Payload of an analysis: the idea and what triggered it (§5.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzePayload {
    pub idea_id: Uuid,
    pub trigger: AnalysisTrigger,
}

fn payload(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).expect("job payloads serialize")
}

/// Kind of the screening retry of a target.
pub const fn screen_kind(kind: TargetKind) -> JobKind {
    match kind {
        TargetKind::Idea => JobKind::ScreenIdea,
        TargetKind::Contribution => JobKind::ScreenContribution,
        TargetKind::Report => JobKind::ScreenReport,
    }
}

pub fn screen_key(kind: TargetKind, id: Uuid) -> String {
    format!("screen:{kind}:{id}")
}

/// Screening (retry) of a submitted target.
pub fn screen_job(kind: TargetKind, id: Uuid) -> NewJob {
    NewJob::new(screen_kind(kind))
        .payload(payload(TargetPayload {
            target_kind: kind,
            target_id: id,
        }))
        .max_attempts(SCREEN_MAX_ATTEMPTS)
        .dedupe_key(screen_key(kind, id))
}

pub fn second_opinion_key(kind: TargetKind, id: Uuid) -> String {
    format!("second_opinion:{kind}:{id}")
}

/// Mistral second opinion of a target in the gray zone.
pub fn second_opinion_job(kind: TargetKind, id: Uuid) -> NewJob {
    NewJob::new(JobKind::SecondOpinion)
        .payload(payload(TargetPayload {
            target_kind: kind,
            target_id: id,
        }))
        .dedupe_key(second_opinion_key(kind, id))
}

/// Duplicate detection of an accepted idea.
pub fn dedup_job(idea_id: Uuid) -> NewJob {
    NewJob::new(JobKind::DedupIdea)
        .payload(payload(IdeaPayload { idea_id }))
        .dedupe_key(format!("dedup_idea:{idea_id}"))
}

/// (Re-)analysis of an idea. Contributions use the contribution priority (§6.4).
pub fn analyze_job(idea_id: Uuid, trigger: AnalysisTrigger) -> NewJob {
    let job = NewJob::new(JobKind::AnalyzeIdea)
        .payload(payload(AnalyzePayload { idea_id, trigger }))
        .dedupe_key(format!("analyze_idea:{idea_id}"));
    match trigger {
        AnalysisTrigger::Contribution => job.priority(priority::CONTRIBUTION),
        AnalysisTrigger::Admin => job.priority(priority::ADMIN),
        AnalysisTrigger::Initial | AnalysisTrigger::Merge | AnalysisTrigger::Report => job,
    }
}

pub fn review_reports_key(idea_id: Uuid) -> String {
    format!("review_reports:{idea_id}")
}

/// Grouped review of the accepted reports of an idea, due at `run_after`.
pub fn review_reports_job(idea_id: Uuid, run_after: DateTime<Utc>) -> NewJob {
    NewJob::new(JobKind::ReviewReports)
        .payload(payload(IdeaPayload { idea_id }))
        .run_after(run_after)
        .dedupe_key(review_reports_key(idea_id))
}

/// Dedupe key of the regeneration of the open data exports after a change.
pub const EXPORTS_KEY: &str = "generate_exports";
/// Delay before the exports are regenerated after a change: the changes made meanwhile
/// (several publications, a deletion…) are absorbed by the same queued job.
pub const EXPORTS_DEBOUNCE: Duration = Duration::minutes(2);

/// Regeneration of the open data exports after a publication or a re-analysis (§7.4),
/// debounced: what is added to the site may wait two minutes. Content that leaves it goes
/// through [`regenerate_exports_now`] instead.
pub fn exports_job() -> NewJob {
    NewJob::new(JobKind::GenerateExports)
        .run_after(Utc::now() + EXPORTS_DEBOUNCE)
        .dedupe_key(EXPORTS_KEY)
}

/// Immediate regeneration of the exports, without dedupe key (see [`regenerate_exports_now`]).
pub fn urgent_exports_job() -> NewJob {
    NewJob::new(JobKind::GenerateExports).priority(priority::URGENT_EXPORTS)
}

/// Regenerates the exports right away, in the caller's transaction, after content left the
/// site: an idea masked (whatever the reason, escalation included) or withdrawn by its author,
/// a text deleted by its author (§7.4). A hidden or withdrawn idea must never stay
/// downloadable, so the two-minute grouping of [`exports_job`] does not apply:
///
/// - a queued regeneration (debounced, nightly or urgent) becomes due now with the top
///   priority ([`priority::URGENT_EXPORTS`]): it has not read the ideas yet;
/// - otherwise a new urgent job is enqueued, without dedupe key, so that it is not absorbed by
///   a generation already running, which may have read the idea before this change (that one
///   also checks, before storing its files, that every idea it exports is still published:
///   `db::exports::still_published`).
///
/// Returns the id of the enqueued job, `None` when a queued one was brought forward.
pub async fn regenerate_exports_now(conn: &mut PgConnection) -> sqlx::Result<Option<i64>> {
    let brought_forward = sqlx::query(
        "UPDATE jobs SET run_after = least(run_after, now()), priority = greatest(priority, $2)
         WHERE kind = $1 AND status = 'queued'",
    )
    .bind(JobKind::GenerateExports.as_str())
    .bind(priority::URGENT_EXPORTS)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if brought_forward > 0 {
        return Ok(None);
    }
    queue::enqueue(&mut *conn, &urgent_exports_job()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payloads_and_keys() {
        let id = Uuid::nil();
        let job = screen_job(TargetKind::Report, id);
        assert_eq!(job.kind, JobKind::ScreenReport);
        assert_eq!(
            job.payload,
            json!({ "target_kind": "report", "target_id": id.to_string() })
        );
        assert_eq!(job.max_attempts, SCREEN_MAX_ATTEMPTS);
        assert_eq!(
            job.dedupe_key.as_deref(),
            Some("screen:report:00000000-0000-0000-0000-000000000000")
        );
        let job = second_opinion_job(TargetKind::Idea, id);
        assert_eq!(job.priority, priority::SECOND_OPINION);
        assert!(job.dedupe_key.unwrap().starts_with("second_opinion:idea:"));

        let job = analyze_job(id, AnalysisTrigger::Contribution);
        assert_eq!(job.priority, priority::CONTRIBUTION);
        assert_eq!(
            job.payload,
            json!({ "idea_id": id.to_string(), "trigger": "contribution" })
        );
        assert_eq!(
            analyze_job(id, AnalysisTrigger::Initial).priority,
            priority::NEW_IDEA
        );
        assert_eq!(dedup_job(id).payload, json!({ "idea_id": id.to_string() }));
        let at = Utc::now();
        let job = review_reports_job(id, at);
        assert_eq!((job.priority, job.run_after), (priority::REPORT, Some(at)));
        let job = exports_job();
        assert_eq!(job.dedupe_key.as_deref(), Some(EXPORTS_KEY));
        assert_eq!(job.priority, priority::MAINTENANCE);
        assert!(job.run_after.unwrap() > Utc::now() + Duration::seconds(60));
    }
}
