//! Ideas in the admin (AGENTS.md §8): list by status with search, masking and republication
//! with a reason, re-analysis on demand (§5.7), of one idea or of every failed analysis.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::{admin_text, conflict, not_found};
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::ideas::{self, AdminIdeaRow};
use crate::db::status::{self, Change};
use crate::domain::status::IdeaStatus;
use crate::domain::types::{Actor, AnalysisTrigger, HiddenReason};
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::ideas::{SEARCH_MAX_CHARS, optional_text};
use crate::http::pagination::{PageRequest, page_type};
use crate::jobs::{pipeline, queue};

/// Longest reason of a masking or a republication.
pub const REASON_MAX_CHARS: usize = 1_000;

fn idea_not_found() -> ApiError {
    not_found("idea_not_found", "Cette idée n'existe pas.")
}

/// An idea, whatever its status.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminIdea {
    pub id: Uuid,
    pub status: IdeaStatus,
    #[schema(required = true)]
    pub slug: Option<String>,
    #[schema(required = true)]
    pub title: Option<String>,
    /// Beginning of the masked original text (`null` once purged).
    #[schema(required = true)]
    pub excerpt: Option<String>,
    #[schema(required = true)]
    pub submitted_admin_level: Option<String>,
    pub created_at: DateTime<Utc>,
    pub status_changed_at: DateTime<Utc>,
    #[schema(required = true)]
    pub rejection_reason_code: Option<String>,
    /// `report_review`, `escalation` or `admin`.
    #[schema(required = true)]
    pub hidden_reason: Option<String>,
    #[schema(required = true)]
    pub hidden_at: Option<DateTime<Utc>>,
    /// The idea a merged duplicate was merged into.
    #[schema(required = true)]
    pub parent_idea_id: Option<Uuid>,
    pub proposal_count: i32,
    pub contribution_count: i32,
    #[schema(required = true)]
    pub score_eur: Option<i64>,
    /// Accepted reports waiting for their review.
    pub reports_pending: i64,
    pub review_request_pending: bool,
    pub escalation_pending: bool,
    /// When its author withdrew it (status `withdrawn`, « retirée par son auteur »; kept after
    /// the purge). Always sent; declared optional so that clients built before it keep
    /// compiling.
    #[schema(required = false)]
    pub withdrawn_at: Option<DateTime<Utc>>,
}

impl TryFrom<AdminIdeaRow> for AdminIdea {
    type Error = ApiError;

    fn try_from(row: AdminIdeaRow) -> Result<Self, ApiError> {
        Ok(Self {
            status: row.status.parse().map_err(|_| ApiError::internal())?,
            id: row.id,
            slug: row.slug,
            title: row.title,
            excerpt: row.excerpt,
            submitted_admin_level: row.submitted_admin_level,
            created_at: row.created_at,
            status_changed_at: row.status_changed_at,
            rejection_reason_code: row.rejection_reason_code,
            hidden_reason: row.hidden_reason,
            hidden_at: row.hidden_at,
            parent_idea_id: row.parent_idea_id,
            proposal_count: row.proposal_count,
            contribution_count: row.contribution_count,
            score_eur: row.score_eur,
            reports_pending: row.reports_pending,
            review_request_pending: row.review_request_pending,
            escalation_pending: row.escalation_pending,
            withdrawn_at: row.withdrawn_at,
        })
    }
}

page_type!(
    /// A page of ideas, most recently changed first.
    AdminIdeaPage,
    AdminIdea
);

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminIdeaQuery {
    #[serde(default)]
    pub status: Option<IdeaStatus>,
    /// Identifier, or part of the slug, title, summary or text (accents and case ignored),
    /// 200 characters.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

/// Ideas of a status, with a search.
#[utoipa::path(
    get,
    path = "/api/admin/ideas",
    tag = "admin",
    operation_id = "listAdminIdeas",
    security(("admin_token" = [])),
    params(AdminIdeaQuery),
    responses(
        (status = 200, description = "Une page d'idées.", body = AdminIdeaPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminIdeaQuery>,
) -> Result<Json<AdminIdeaPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let q = optional_text(
        query.q,
        SEARCH_MAX_CHARS,
        "La recherche doit contenir au plus 200 caractères.",
    )?;
    let (rows, total) = ideas::admin_list(
        &state.pool,
        query.status,
        q.as_deref(),
        pages.limit(),
        pages.offset(),
    )
    .await?;
    let items = rows
        .into_iter()
        .map(AdminIdea::try_from)
        .collect::<Result<_, _>>()?;
    Ok(Json(AdminIdeaPage::new(items, pages, total)))
}

/// Reason of a masking or a republication, kept in the admin journal.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminReason {
    /// 1 to 1 000 characters.
    pub reason: String,
}

async fn idea_after(state: &AppState, id: Uuid) -> Result<Json<AdminIdea>, ApiError> {
    let row = ideas::admin_get(&state.pool, id)
        .await?
        .ok_or_else(idea_not_found)?;
    Ok(Json(row.try_into()?))
}

async fn exists(conn: &mut sqlx::PgConnection, id: Uuid) -> Result<(), ApiError> {
    match status::lock::<IdeaStatus>(conn, id).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(idea_not_found()),
        Err(status::StatusError::Database(err)) => Err(err.into()),
        Err(_) => Err(ApiError::internal()),
    }
}

/// Hides a published idea (`hidden_reason = admin`): it leaves the portal and the next exports;
/// its author sees the masking on the tracking page and may ask for a human review.
#[utoipa::path(
    post,
    path = "/api/admin/ideas/{id}/hide",
    tag = "admin",
    operation_id = "hideAdminIdea",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de l'idée")),
    request_body = AdminReason,
    responses(
        (status = 200, description = "Idée masquée.", body = AdminIdea),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Idée inconnue (`idea_not_found`).", body = ErrorBody),
        (status = 409, description = "Idée non publiée (`idea_not_published`).", body = ErrorBody),
        (status = 422, description = "Motif absent ou trop long (`invalid_field`).", body = ErrorBody)
    )
)]
pub async fn hide(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AdminReason>,
) -> Result<Json<AdminIdea>, ApiError> {
    let reason = admin_text(Some(&body.reason), REASON_MAX_CHARS, true, "Le motif")?;
    let mut tx = state.pool.begin().await?;
    exists(&mut tx, id).await?;
    let change = Change::by(Actor::Admin).reason(HiddenReason::Admin.as_str());
    let hidden = ideas::hide(&mut tx, id, HiddenReason::Admin, &change)
        .await
        .map_err(status_error)?;
    if !hidden {
        return Err(conflict(
            "idea_not_published",
            "Seule une idée publiée peut être masquée.",
        ));
    }
    queue::enqueue(&mut *tx, &pipeline::exports_job()).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("idea.hide")
            .target("idea", id)
            .details(json!({ "reason": reason })),
    )
    .await?;
    tx.commit().await?;
    idea_after(&state, id).await
}

/// Republishes a hidden idea, whatever hid it.
#[utoipa::path(
    post,
    path = "/api/admin/ideas/{id}/republish",
    tag = "admin",
    operation_id = "republishAdminIdea",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de l'idée")),
    request_body = AdminReason,
    responses(
        (status = 200, description = "Idée republiée.", body = AdminIdea),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Idée inconnue (`idea_not_found`).", body = ErrorBody),
        (status = 409, description = "Idée non masquée (`idea_not_hidden`), ou retirée par son \
            auteur (`idea_withdrawn` : jamais republiée).", body = ErrorBody),
        (status = 422, description = "Motif absent ou trop long (`invalid_field`).", body = ErrorBody)
    )
)]
pub async fn republish(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AdminReason>,
) -> Result<Json<AdminIdea>, ApiError> {
    let reason = admin_text(Some(&body.reason), REASON_MAX_CHARS, true, "Le motif")?;
    let mut tx = state.pool.begin().await?;
    exists(&mut tx, id).await?;
    if status::lock::<IdeaStatus>(&mut tx, id)
        .await
        .map_err(status_error)?
        == Some(IdeaStatus::Withdrawn)
    {
        // The author's choice is respected (owner decision D2): never republished.
        return Err(conflict(
            "idea_withdrawn",
            "Cette idée a été retirée par son auteur : elle ne peut pas être republiée.",
        ));
    }
    let change = Change::by(Actor::Admin).reason("admin_republish");
    if !ideas::republish(&mut tx, id, &change)
        .await
        .map_err(status_error)?
    {
        return Err(conflict(
            "idea_not_hidden",
            "Seule une idée masquée peut être republiée.",
        ));
    }
    queue::enqueue(&mut *tx, &pipeline::exports_job()).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("idea.republish")
            .target("idea", id)
            .details(json!({ "reason": reason })),
    )
    .await?;
    tx.commit().await?;
    idea_after(&state, id).await
}

pub(crate) fn status_error(err: status::StatusError) -> ApiError {
    match err {
        status::StatusError::Database(err) => err.into(),
        other => {
            tracing::error!(error = %other, "status change refused");
            ApiError::internal()
        }
    }
}

/// Result of a re-analysis request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminReanalysis {
    /// `false` when an analysis of the idea was already queued (it will run anyway).
    pub queued: bool,
    pub status: IdeaStatus,
}

/// Most ideas put back in the queue by one [`reanalyze_failed`] request.
pub const BULK_REANALYSIS_MAX: i64 = 500;

/// Result of a bulk re-analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminBulkReanalysis {
    /// Ideas moved from `analysis_failed` to `queued_analysis`.
    pub requeued: i64,
    /// Ideas still in `analysis_failed` (beyond the 500 of one request): call again.
    pub remaining: i64,
}

/// Puts every idea whose analysis failed back in the queue (`analysis_failed →
/// queued_analysis`, trigger `admin`), oldest first, 500 per request; one admin action
/// `idea.reanalyze_failed` with their identifiers.
#[utoipa::path(
    post,
    path = "/api/admin/ideas/reanalyze-failed",
    tag = "admin",
    operation_id = "reanalyzeFailedAdminIdeas",
    security(("admin_token" = [])),
    responses(
        (status = 202, description = "Analyses mises en file.", body = AdminBulkReanalysis),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn reanalyze_failed(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<AdminBulkReanalysis>), ApiError> {
    let mut tx = state.pool.begin().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM ideas WHERE status = 'analysis_failed'
         ORDER BY status_changed_at, id LIMIT $1 FOR UPDATE SKIP LOCKED",
    )
    .bind(BULK_REANALYSIS_MAX)
    .fetch_all(&mut *tx)
    .await?;
    let change = Change::by(Actor::Admin).reason("admin_reanalysis");
    for id in &ids {
        status::transition(&mut tx, *id, IdeaStatus::QueuedAnalysis, &change)
            .await
            .map_err(status_error)?;
        queue::enqueue(
            &mut *tx,
            &pipeline::analyze_job(*id, AnalysisTrigger::Admin),
        )
        .await?;
    }
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ideas WHERE status = 'analysis_failed'")
            .fetch_one(&mut *tx)
            .await?;
    let requeued = ids.len() as i64;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("idea.reanalyze_failed").details(json!({
            "count": requeued,
            "remaining": remaining,
            "idea_ids": ids,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AdminBulkReanalysis {
            requeued,
            remaining,
        }),
    ))
}

/// Queues a new analysis (trigger `admin`, §5.7): of a published or hidden idea (it stays
/// visible with its current analysis until the new one replaces it), or of an idea whose
/// analysis failed (`analysis_failed → queued_analysis`).
#[utoipa::path(
    post,
    path = "/api/admin/ideas/{id}/reanalyze",
    tag = "admin",
    operation_id = "reanalyzeAdminIdea",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de l'idée")),
    responses(
        (status = 202, description = "Analyse mise en file.", body = AdminReanalysis),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Idée inconnue (`idea_not_found`).", body = ErrorBody),
        (status = 409, description = "Statut sans analyse possible (`reanalysis_not_available`).",
         body = ErrorBody)
    )
)]
pub async fn reanalyze(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<(StatusCode, Json<AdminReanalysis>), ApiError> {
    let mut tx = state.pool.begin().await?;
    let current = status::lock::<IdeaStatus>(&mut tx, id)
        .await
        .map_err(status_error)?
        .ok_or_else(idea_not_found)?;
    let change = Change::by(Actor::Admin).reason("admin_reanalysis");
    let status = match current {
        IdeaStatus::Published | IdeaStatus::Hidden => current,
        IdeaStatus::AnalysisFailed => {
            status::transition(&mut tx, id, IdeaStatus::QueuedAnalysis, &change)
                .await
                .map_err(status_error)?;
            IdeaStatus::QueuedAnalysis
        }
        _ => {
            return Err(conflict(
                "reanalysis_not_available",
                "Seule une idée publiée, masquée ou dont l'analyse a échoué peut être \
                 ré-analysée.",
            ));
        }
    };
    let job = queue::enqueue(&mut *tx, &pipeline::analyze_job(id, AnalysisTrigger::Admin)).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("idea.reanalyze")
            .target("idea", id)
            .details(json!({ "from_status": current.as_str(), "job_id": job })),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AdminReanalysis {
            queued: job.is_some(),
            status,
        }),
    ))
}
