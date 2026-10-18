//! Storage of the texts submitted by visitors (AGENTS.md §5.1, §5.5, §5.6).
//!
//! Each insertion runs in one transaction with the creation event of the history, the hash of
//! the tracking token and the screening job that takes over if the synchronous screening cannot
//! complete (decision service down, api stopped). Only the **masked** text is ever stored
//! (§7.5): the caller masks it before.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use super::status::{self, Change};
use super::tracking::{self, TrackingToken};
use crate::domain::rules::CGU_VERSION;
use crate::domain::status::{ContributionStatus, IdeaStatus, ReportStatus};
use crate::domain::types::{Actor, AdminLevel, ContributionKind, ReportReason, TargetKind};
use crate::jobs::{pipeline, queue};
use crate::moderation::SCREENING_RETRY_DELAY;

/// A new idea, already validated and masked.
pub struct NewIdea {
    pub text: String,
    pub admin_level: Option<AdminLevel>,
    pub territory: Option<String>,
}

/// A new contribution to a published idea, already validated and masked.
pub struct NewContribution {
    pub idea_id: Uuid,
    pub kind: ContributionKind,
    pub text: String,
}

/// A new report about a published idea, already validated (details masked).
pub struct NewReport {
    pub idea_id: Uuid,
    pub reason: ReportReason,
    pub details: Option<String>,
    /// Optional contact of the reporter, erased after 6 months (§7.5).
    pub reporter_name: Option<String>,
    pub reporter_email: Option<String>,
}

/// Id of the published idea with this slug.
pub async fn published_idea_id(pool: &PgPool, slug: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("SELECT id FROM ideas WHERE slug = $1 AND status = 'published'")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

/// Stores the tracking token and the delayed screening job of a new text, then commits.
async fn finish(
    mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    kind: TargetKind,
    id: Uuid,
) -> sqlx::Result<TrackingToken> {
    let token = TrackingToken::generate();
    tracking::insert(&mut *tx, &token, kind, id).await?;
    queue::enqueue(
        &mut *tx,
        &pipeline::screen_job(kind, id).run_after(Utc::now() + SCREENING_RETRY_DELAY),
    )
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// Stores a submitted idea with the consent to the current rules (day precision, Paris time)
/// and returns its id and its tracking token.
pub async fn insert_idea(pool: &PgPool, idea: &NewIdea) -> sqlx::Result<(Uuid, TrackingToken)> {
    let mut tx = pool.begin().await?;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO ideas (original_text, submitted_admin_level, submitted_territory,
                            cgu_version, cgu_accepted_at)
         VALUES ($1, $2, $3, $4, (now() AT TIME ZONE 'Europe/Paris')::date)
         RETURNING id",
    )
    .bind(&idea.text)
    .bind(idea.admin_level.map(AdminLevel::as_str))
    .bind(idea.territory.as_deref())
    .bind(CGU_VERSION)
    .fetch_one(&mut *tx)
    .await?;
    status::record_creation::<IdeaStatus>(&mut tx, id, &Change::by(Actor::Author)).await?;
    Ok((id, finish(tx, TargetKind::Idea, id).await?))
}

/// Stores a contribution with the consent to the current rules.
pub async fn insert_contribution(
    pool: &PgPool,
    contribution: &NewContribution,
) -> sqlx::Result<(Uuid, TrackingToken)> {
    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO contributions (idea_id, kind, text, cgu_version, cgu_accepted_at)
         VALUES ($1, $2, $3, $4, (now() AT TIME ZONE 'Europe/Paris')::date)
         RETURNING id",
    )
    .bind(contribution.idea_id)
    .bind(contribution.kind.as_str())
    .bind(&contribution.text)
    .bind(CGU_VERSION)
    .fetch_one(&mut *tx)
    .await?;
    status::record_creation::<ContributionStatus>(&mut tx, id, &Change::by(Actor::Author)).await?;
    Ok((id, finish(tx, TargetKind::Contribution, id).await?))
}

/// Stores a report (with the declaration of good faith, required by the form).
pub async fn insert_report(
    pool: &PgPool,
    report: &NewReport,
) -> sqlx::Result<(Uuid, TrackingToken)> {
    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO reports (idea_id, reason, details, reporter_name, reporter_email, good_faith)
         VALUES ($1, $2, $3, $4, $5, true)
         RETURNING id",
    )
    .bind(report.idea_id)
    .bind(report.reason.as_str())
    .bind(report.details.as_deref())
    .bind(report.reporter_name.as_deref())
    .bind(report.reporter_email.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    status::record_creation::<ReportStatus>(&mut tx, id, &Change::by(Actor::Author)).await?;
    Ok((id, finish(tx, TargetKind::Report, id).await?))
}
