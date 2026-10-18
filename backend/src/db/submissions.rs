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
use crate::domain::status::IdeaStatus;
use crate::domain::types::{Actor, AdminLevel, TargetKind};
use crate::jobs::{pipeline, queue};
use crate::moderation::SCREENING_RETRY_DELAY;

/// A new idea, already validated and masked.
pub struct NewIdea {
    pub text: String,
    pub admin_level: Option<AdminLevel>,
    pub territory: Option<String>,
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
    let token = TrackingToken::generate();
    tracking::insert(&mut *tx, &token, TargetKind::Idea, id).await?;
    queue::enqueue(
        &mut *tx,
        &pipeline::screen_job(TargetKind::Idea, id).run_after(Utc::now() + SCREENING_RETRY_DELAY),
    )
    .await?;
    tx.commit().await?;
    Ok((id, token))
}
