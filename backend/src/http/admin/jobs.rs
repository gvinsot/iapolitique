//! Job queue of the admin (AGENTS.md §8): list by status and kind, retry of a dead job.

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};

use super::ideas::status_error;
use super::{conflict, not_found};
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::status::{self, Change};
use crate::domain::status::IdeaStatus;
use crate::domain::types::Actor;
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::{ApiPath, ApiQuery};
use crate::http::pagination::{PageRequest, page_type};
use crate::jobs::pipeline::AnalyzePayload;
use crate::jobs::queue::{self, JobRow, RetryError};
use crate::jobs::{JobKind, JobStatus};

/// A job of the queue. Payloads and errors hold internal identifiers only (§7.1).
#[derive(Debug, Clone, PartialEq, serde::Serialize, ToSchema)]
pub struct AdminJob {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub priority: i16,
    pub attempts: i32,
    pub max_attempts: i32,
    pub payload: Value,
    #[schema(required = true)]
    pub dedupe_key: Option<String>,
    pub run_after: DateTime<Utc>,
    #[schema(required = true)]
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[schema(required = true)]
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<JobRow> for AdminJob {
    fn from(row: JobRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            status: row.status,
            priority: row.priority,
            attempts: row.attempts,
            max_attempts: row.max_attempts,
            payload: row.payload,
            dedupe_key: row.dedupe_key,
            run_after: row.run_after,
            last_error: row.last_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
            finished_at: row.finished_at,
        }
    }
}

page_type!(
    /// A page of jobs, most recent first.
    AdminJobPage,
    AdminJob
);

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminJobQuery {
    #[serde(default)]
    pub status: Option<JobStatus>,
    #[serde(default)]
    pub kind: Option<JobKind>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

/// Jobs of the queue, filtered by status and kind.
#[utoipa::path(
    get,
    path = "/api/admin/jobs",
    tag = "admin",
    operation_id = "listAdminJobs",
    security(("admin_token" = [])),
    params(AdminJobQuery),
    responses(
        (status = 200, description = "Une page de jobs.", body = AdminJobPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminJobQuery>,
) -> Result<Json<AdminJobPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let (rows, total) = queue::list(
        &state.pool,
        query.status,
        query.kind,
        pages.limit(),
        pages.offset(),
    )
    .await?;
    Ok(Json(AdminJobPage::new(
        rows.into_iter().map(AdminJob::from).collect(),
        pages,
        total,
    )))
}

/// Queues a dead job again (attempts reset, due now). A dead screening or second opinion
/// leaves its text waiting: retrying it resumes the pipeline. A dead analysis left its idea in
/// `analysis_failed`, which the job ignores: retrying it also moves the idea back to
/// `queued_analysis`, as « Relancer l'analyse » of the idea does.
#[utoipa::path(
    post,
    path = "/api/admin/jobs/{id}/retry",
    tag = "admin",
    operation_id = "retryAdminJob",
    security(("admin_token" = [])),
    params(("id" = i64, Path, description = "Identifiant du job")),
    responses(
        (status = 200, description = "Job remis en file.", body = AdminJob),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Job inconnu (`job_not_found`).", body = ErrorBody),
        (status = 409, description = "Job non mort (`job_not_dead`) ou job équivalent déjà en \
                                      file (`job_already_queued`).", body = ErrorBody)
    )
)]
pub async fn retry(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<i64>,
) -> Result<Json<AdminJob>, ApiError> {
    let mut tx = state.pool.begin().await?;
    let row = match queue::retry_dead(&mut tx, id).await {
        Ok(row) => row,
        Err(RetryError::NotFound) => {
            return Err(not_found("job_not_found", "Ce job n'existe pas."));
        }
        Err(RetryError::NotDead) => {
            return Err(conflict(
                "job_not_dead",
                "Seuls les jobs abandonnés (dead) peuvent être relancés.",
            ));
        }
        Err(RetryError::Duplicate) => {
            return Err(conflict(
                "job_already_queued",
                "Un job équivalent est déjà en file ou en cours.",
            ));
        }
        Err(RetryError::Database(err)) => return Err(err.into()),
    };
    let mut details = json!({ "kind": row.kind });
    if row.kind == JobKind::AnalyzeIdea.as_str() {
        let idea_id = serde_json::from_value::<AnalyzePayload>(row.payload.clone())
            .map_err(|err| {
                tracing::error!(job_id = row.id, error = %err, "analysis job without idea");
                ApiError::internal()
            })?
            .idea_id;
        let current = status::lock::<IdeaStatus>(&mut tx, idea_id)
            .await
            .map_err(status_error)?;
        let requeued = current == Some(IdeaStatus::AnalysisFailed);
        if requeued {
            let change = Change::by(Actor::Admin).reason("admin_reanalysis");
            status::transition(&mut tx, idea_id, IdeaStatus::QueuedAnalysis, &change)
                .await
                .map_err(status_error)?;
        }
        details["idea_id"] = json!(idea_id);
        details["idea_requeued"] = json!(requeued);
    }
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("job.retry")
            .target("job", row.id)
            .details(details),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row.into()))
}
