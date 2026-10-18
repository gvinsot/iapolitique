//! Rights of reply (LCEN art. 1-1 III, AGENTS.md §7.5): the administrator publishes, edits or
//! removes a reply attached to an idea; it appears as plain text on the idea page
//! (`GET /api/ideas/{slug}`). The law requires the insertion within 3 days of the request.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use super::ideas::status_error;
use super::{admin_text, conflict, not_found};
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::replies::{self, ReplyContent, ReplyRow};
use crate::db::status;
use crate::domain::status::IdeaStatus;
use crate::domain::validation::validate_line;
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::{ApiJson, ApiPath};

/// Longest designation of the person replying.
pub const AUTHOR_LABEL_MAX_CHARS: usize = 200;
/// Longest reply.
pub const BODY_MAX_CHARS: usize = 8_000;
/// Days within which the reply must be inserted after the request.
pub const PUBLICATION_DELAY_DAYS: u64 = 3;

/// A right of reply as edited by the administrator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminReply {
    pub id: Uuid,
    pub idea_id: Uuid,
    pub author_label: String,
    pub body: String,
    /// Day the request was received.
    pub requested_on: NaiveDate,
    /// Last day of the legal 3-day delay.
    pub due_on: NaiveDate,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ReplyRow> for AdminReply {
    fn from(row: ReplyRow) -> Self {
        Self {
            due_on: row
                .requested_on
                .checked_add_days(Days::new(PUBLICATION_DELAY_DAYS))
                .unwrap_or(row.requested_on),
            id: row.id,
            idea_id: row.idea_id,
            author_label: row.author_label,
            body: row.body,
            requested_on: row.requested_on,
            published_at: row.published_at,
            updated_at: row.updated_at,
        }
    }
}

/// Content of a right of reply, published as plain text.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminReplyContent {
    /// How the person replying is designated, as they asked (one line, 200 characters).
    pub author_label: String,
    /// Text of the reply (8 000 characters).
    pub body: String,
    /// Day the request was received (not in the future).
    pub requested_on: NaiveDate,
}

struct Validated {
    author_label: String,
    body: String,
    requested_on: NaiveDate,
}

impl Validated {
    fn content(&self) -> ReplyContent<'_> {
        ReplyContent {
            author_label: &self.author_label,
            body: &self.body,
            requested_on: self.requested_on,
        }
    }
}

async fn validate(state: &AppState, content: AdminReplyContent) -> Result<Validated, ApiError> {
    let author_label = validate_line(
        Some(&content.author_label),
        "La désignation de l'auteur de la réponse",
        AUTHOR_LABEL_MAX_CHARS,
    )
    .map_err(|err| ApiError::validation("invalid_field", err.to_string()))?
    .ok_or_else(|| {
        ApiError::validation(
            "invalid_field",
            "La désignation de l'auteur de la réponse est obligatoire.",
        )
    })?;
    let body =
        admin_text(Some(&content.body), BODY_MAX_CHARS, true, "La réponse")?.unwrap_or_default();
    let today: NaiveDate = sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Paris')::date")
        .fetch_one(&state.pool)
        .await?;
    if content.requested_on > today {
        return Err(ApiError::validation(
            "invalid_field",
            "La date de la demande ne peut pas être dans le futur.",
        ));
    }
    Ok(Validated {
        author_label,
        body,
        requested_on: content.requested_on,
    })
}

fn idea_not_found() -> ApiError {
    not_found("idea_not_found", "Cette idée n'existe pas.")
}

fn reply_not_found() -> ApiError {
    not_found("reply_not_found", "Ce droit de réponse n'existe pas.")
}

/// Rights of reply of an idea.
#[utoipa::path(
    get,
    path = "/api/admin/ideas/{id}/replies",
    tag = "admin",
    operation_id = "listAdminReplies",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de l'idée")),
    responses(
        (status = 200, description = "Droits de réponse de l'idée.", body = Vec<AdminReply>),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Idée inconnue (`idea_not_found`).", body = ErrorBody)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ApiPath(idea_id): ApiPath<Uuid>,
) -> Result<Json<Vec<AdminReply>>, ApiError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM ideas WHERE id = $1)")
        .bind(idea_id)
        .fetch_one(&state.pool)
        .await?;
    if !exists {
        return Err(idea_not_found());
    }
    let rows = replies::of_idea(&state.pool, idea_id).await?;
    Ok(Json(rows.into_iter().map(AdminReply::from).collect()))
}

/// Publishes a right of reply under a published (or hidden) idea.
#[utoipa::path(
    post,
    path = "/api/admin/ideas/{id}/replies",
    tag = "admin",
    operation_id = "createAdminReply",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de l'idée")),
    request_body = AdminReplyContent,
    responses(
        (status = 201, description = "Réponse publiée.", body = AdminReply),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Idée inconnue (`idea_not_found`).", body = ErrorBody),
        (status = 409, description = "Idée ni publiée ni masquée (`idea_not_published`).",
         body = ErrorBody),
        (status = 422, description = "Champ invalide (`invalid_field`), corps invalide.",
         body = ErrorBody)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    ApiPath(idea_id): ApiPath<Uuid>,
    ApiJson(content): ApiJson<AdminReplyContent>,
) -> Result<(StatusCode, Json<AdminReply>), ApiError> {
    let content = validate(&state, content).await?;
    let mut tx = state.pool.begin().await?;
    let current = status::lock::<IdeaStatus>(&mut tx, idea_id)
        .await
        .map_err(status_error)?
        .ok_or_else(idea_not_found)?;
    if !current.has_publication() {
        return Err(conflict(
            "idea_not_published",
            "Un droit de réponse ne peut être publié que sous une idée publiée ou masquée.",
        ));
    }
    let row = replies::insert(&mut tx, idea_id, &content.content()).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("reply.create")
            .target("reply", row.id)
            .details(json!({ "idea_id": idea_id, "requested_on": row.requested_on })),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// Replaces the content of a right of reply.
#[utoipa::path(
    put,
    path = "/api/admin/replies/{id}",
    tag = "admin",
    operation_id = "updateAdminReply",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant du droit de réponse")),
    request_body = AdminReplyContent,
    responses(
        (status = 200, description = "Réponse modifiée.", body = AdminReply),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Réponse inconnue (`reply_not_found`).", body = ErrorBody),
        (status = 422, description = "Champ invalide (`invalid_field`), corps invalide.",
         body = ErrorBody)
    )
)]
pub async fn update(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(content): ApiJson<AdminReplyContent>,
) -> Result<Json<AdminReply>, ApiError> {
    let content = validate(&state, content).await?;
    let mut tx = state.pool.begin().await?;
    let row = replies::update(&mut tx, id, &content.content())
        .await?
        .ok_or_else(reply_not_found)?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("reply.update")
            .target("reply", id)
            .details(json!({ "idea_id": row.idea_id })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row.into()))
}

/// Removes a right of reply.
#[utoipa::path(
    delete,
    path = "/api/admin/replies/{id}",
    tag = "admin",
    operation_id = "deleteAdminReply",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant du droit de réponse")),
    responses(
        (status = 204, description = "Réponse retirée."),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Réponse inconnue (`reply_not_found`).", body = ErrorBody)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.pool.begin().await?;
    let idea_id = replies::delete(&mut tx, id)
        .await?
        .ok_or_else(reply_not_found)?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("reply.delete")
            .target("reply", id)
            .details(json!({ "idea_id": idea_id })),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
