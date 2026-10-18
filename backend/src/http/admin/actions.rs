//! Journal of the administrator's actions (`admin_actions`, AGENTS.md §8).

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::db::admin_actions::{self, AdminActionRow};
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::ApiQuery;
use crate::http::pagination::{PageRequest, page_type};

/// A recorded admin action.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminAction {
    pub id: i64,
    /// Dotted name: `idea.hide`, `job.retry`, `settings.screening_thresholds`…
    pub action: String,
    #[schema(required = true)]
    pub target_kind: Option<String>,
    #[schema(required = true)]
    pub target_id: Option<String>,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

impl From<AdminActionRow> for AdminAction {
    fn from(row: AdminActionRow) -> Self {
        Self {
            id: row.id,
            action: row.action,
            target_kind: row.target_kind,
            target_id: row.target_id,
            details: row.details,
            created_at: row.created_at,
        }
    }
}

page_type!(
    /// A page of admin actions, most recent first.
    AdminActionPage,
    AdminAction
);

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminActionQuery {
    /// Exact action name.
    #[serde(default)]
    pub action: Option<String>,
    /// Identifier of the target (idea, job, setting…).
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

fn bounded(value: Option<String>) -> Result<Option<String>, ApiError> {
    match value {
        Some(v) if v.chars().count() > 100 => Err(ApiError::bad_request(
            "invalid_query",
            "Filtre trop long (100 caractères au plus).",
        )),
        Some(v) if v.trim().is_empty() => Ok(None),
        other => Ok(other),
    }
}

/// Journal of the admin actions.
#[utoipa::path(
    get,
    path = "/api/admin/actions",
    tag = "admin",
    operation_id = "listAdminActions",
    security(("admin_token" = [])),
    params(AdminActionQuery),
    responses(
        (status = 200, description = "Une page du journal.", body = AdminActionPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminActionQuery>,
) -> Result<Json<AdminActionPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let action = bounded(query.action)?;
    let target_id = bounded(query.target_id)?;
    let (rows, total) = admin_actions::list(
        &state.pool,
        action.as_deref(),
        target_id.as_deref(),
        pages.limit(),
        pages.offset(),
    )
    .await?;
    Ok(Json(AdminActionPage::new(
        rows.into_iter().map(AdminAction::from).collect(),
        pages,
        total,
    )))
}
