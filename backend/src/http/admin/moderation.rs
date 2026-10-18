//! Human moderation queues (AGENTS.md §5.6, §7.1, §7.5, §8): reports, escalations of serious
//! content (forwarded to PHAROS by hand, 24-hour target) and human review requests of
//! automated decisions.

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::ideas::status_error;
use super::{AdminIdeaRef, admin_text, conflict, not_found};
use crate::db::admin_actions::{self, NewAdminAction};
use crate::db::admin_moderation::{
    self, CloseError, EscalationRow, ReportFilter, ReportRow, ReviewRequestRow,
};
use crate::db::ideas;
use crate::db::status::Change;
use crate::domain::status::ReportStatus;
use crate::domain::types::{
    Actor, EscalationOutcome, ReportReason, ReviewRequestStatus, ReviewResolution, TargetKind,
};
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::pagination::{PageRequest, page_type};
use crate::jobs::{pipeline, queue};
use crate::moderation::{self, Target};

/// Longest note of the administrator.
pub const NOTE_MAX_CHARS: usize = 2_000;
/// Target to handle an escalation (§7.5).
pub const ESCALATION_TARGET_HOURS: i64 = 24;

// ---------------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------------

/// The AI review that handled a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminReportReview {
    pub id: Uuid,
    /// `maintain`, `update` or `hide`.
    pub outcome: String,
    pub escalate: bool,
    pub explanation: String,
    pub corrections: String,
    pub reviewed_at: DateTime<Utc>,
}

/// A report. The reporter's name and e-mail (optional, erased after 6 months) are shown only
/// here, never published nor exported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminReport {
    pub id: Uuid,
    pub idea: AdminIdeaRef,
    pub reason: ReportReason,
    /// Masked details (`null` when none were given or once erased).
    #[schema(required = true)]
    pub details: Option<String>,
    pub status: ReportStatus,
    pub created_at: DateTime<Utc>,
    #[schema(required = true)]
    pub rejection_reason_code: Option<String>,
    #[schema(required = true)]
    pub reporter_name: Option<String>,
    #[schema(required = true)]
    pub reporter_email: Option<String>,
    pub good_faith: bool,
    #[schema(required = true)]
    pub review: Option<AdminReportReview>,
}

fn known<T: std::str::FromStr>(value: &str) -> Result<T, ApiError> {
    value.parse().map_err(|_| {
        tracing::error!("unknown value in the database");
        ApiError::internal()
    })
}

impl TryFrom<ReportRow> for AdminReport {
    type Error = ApiError;

    fn try_from(row: ReportRow) -> Result<Self, ApiError> {
        let review = match (row.review_id, row.review_outcome, row.reviewed_at) {
            (Some(id), Some(outcome), Some(reviewed_at)) => Some(AdminReportReview {
                id,
                outcome,
                escalate: row.review_escalate.unwrap_or(false),
                explanation: row.review_explanation.unwrap_or_default(),
                corrections: row.review_corrections.unwrap_or_default(),
                reviewed_at,
            }),
            _ => None,
        };
        Ok(Self {
            id: row.id,
            idea: AdminIdeaRef {
                id: row.idea_id,
                slug: row.idea_slug,
                title: row.idea_title,
                status: row.idea_status,
            },
            reason: known(&row.reason)?,
            details: row.details,
            status: known(&row.status)?,
            created_at: row.created_at,
            rejection_reason_code: row.rejection_reason_code,
            reporter_name: row.reporter_name,
            reporter_email: row.reporter_email,
            good_faith: row.good_faith,
            review,
        })
    }
}

page_type!(
    /// A page of reports, most recent first.
    AdminReportPage,
    AdminReport
);

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminReportQuery {
    #[serde(default)]
    pub status: Option<ReportStatus>,
    #[serde(default)]
    pub reason: Option<ReportReason>,
    /// Reports of one idea.
    #[serde(default)]
    pub idea_id: Option<Uuid>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

/// Reports, with their idea and their review.
#[utoipa::path(
    get,
    path = "/api/admin/reports",
    tag = "admin",
    operation_id = "listAdminReports",
    security(("admin_token" = [])),
    params(AdminReportQuery),
    responses(
        (status = 200, description = "Une page de signalements.", body = AdminReportPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn reports(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminReportQuery>,
) -> Result<Json<AdminReportPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let filter = ReportFilter {
        status: query.status,
        reason: query.reason,
        idea_id: query.idea_id,
    };
    let (rows, total) =
        admin_moderation::reports(&state.pool, filter, pages.limit(), pages.offset()).await?;
    let items = rows
        .into_iter()
        .map(AdminReport::try_from)
        .collect::<Result<_, _>>()?;
    Ok(Json(AdminReportPage::new(items, pages, total)))
}

// ---------------------------------------------------------------------------------------------
// Escalations
// ---------------------------------------------------------------------------------------------

/// An idea hidden at once because a report review found serious content (§7.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminEscalation {
    pub id: Uuid,
    pub idea: AdminIdeaRef,
    /// `pending` or `done`.
    pub status: ReviewRequestStatus,
    #[schema(required = true)]
    pub outcome: Option<EscalationOutcome>,
    #[schema(required = true)]
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    /// End of the 24-hour target.
    pub due_at: DateTime<Utc>,
    #[schema(required = true)]
    pub handled_at: Option<DateTime<Utc>>,
    /// Explanation of the review that raised it.
    #[schema(required = true)]
    pub review_explanation: Option<String>,
}

impl TryFrom<EscalationRow> for AdminEscalation {
    type Error = ApiError;

    fn try_from(row: EscalationRow) -> Result<Self, ApiError> {
        Ok(Self {
            id: row.id,
            idea: AdminIdeaRef {
                id: row.idea_id,
                slug: row.idea_slug,
                title: row.idea_title,
                status: row.idea_status,
            },
            status: known(&row.status)?,
            outcome: row.outcome.as_deref().map(known).transpose()?,
            note: row.note,
            due_at: row.created_at + Duration::hours(ESCALATION_TARGET_HOURS),
            created_at: row.created_at,
            handled_at: row.handled_at,
            review_explanation: row.review_explanation,
        })
    }
}

page_type!(
    /// A page of escalations: pending ones first, oldest first.
    AdminEscalationPage,
    AdminEscalation
);

/// Filter and pagination of the human queues.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct AdminQueueQuery {
    /// `pending` or `done`; both when absent (pending first).
    #[serde(default)]
    pub status: Option<ReviewRequestStatus>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 10000)]
    pub page: Option<u32>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 50)]
    pub per_page: Option<u32>,
}

/// The escalation queue.
#[utoipa::path(
    get,
    path = "/api/admin/escalations",
    tag = "admin",
    operation_id = "listAdminEscalations",
    security(("admin_token" = [])),
    params(AdminQueueQuery),
    responses(
        (status = 200, description = "Une page d'escalades.", body = AdminEscalationPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn escalations(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminQueueQuery>,
) -> Result<Json<AdminEscalationPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let (rows, total) =
        admin_moderation::escalations(&state.pool, query.status, pages.limit(), pages.offset())
            .await?;
    let items = rows
        .into_iter()
        .map(AdminEscalation::try_from)
        .collect::<Result<_, _>>()?;
    Ok(Json(AdminEscalationPage::new(items, pages, total)))
}

/// How an escalation was handled.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminEscalationHandling {
    pub outcome: EscalationOutcome,
    /// Date of the referral, reference… (2 000 characters).
    #[serde(default)]
    pub note: Option<String>,
}

/// Closes an escalation (the idea stays hidden; republish it separately if needed).
#[utoipa::path(
    post,
    path = "/api/admin/escalations/{id}/handle",
    tag = "admin",
    operation_id = "handleAdminEscalation",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de l'escalade")),
    request_body = AdminEscalationHandling,
    responses(
        (status = 200, description = "Escalade traitée.", body = AdminEscalation),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Escalade inconnue (`escalation_not_found`).", body = ErrorBody),
        (status = 409, description = "Déjà traitée (`escalation_already_handled`).", body = ErrorBody),
        (status = 422, description = "Corps invalide.", body = ErrorBody)
    )
)]
pub async fn handle_escalation(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AdminEscalationHandling>,
) -> Result<Json<AdminEscalation>, ApiError> {
    let note = admin_text(body.note.as_deref(), NOTE_MAX_CHARS, false, "La note")?;
    let mut tx = state.pool.begin().await?;
    match admin_moderation::close_escalation(&mut tx, id, body.outcome, note.as_deref()).await? {
        Ok(()) => {}
        Err(CloseError::NotFound) => {
            return Err(not_found(
                "escalation_not_found",
                "Cette escalade n'existe pas.",
            ));
        }
        Err(CloseError::AlreadyDone) => {
            return Err(conflict(
                "escalation_already_handled",
                "Cette escalade a déjà été traitée.",
            ));
        }
    }
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("escalation.handle")
            .target("escalation", id)
            .details(json!({ "outcome": body.outcome.as_str(), "note": note })),
    )
    .await?;
    let row = admin_moderation::escalation(&mut tx, id)
        .await?
        .ok_or_else(ApiError::internal)?;
    tx.commit().await?;
    Ok(Json(row.try_into()?))
}

// ---------------------------------------------------------------------------------------------
// Human review requests
// ---------------------------------------------------------------------------------------------

/// The automated decision under review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminReviewedDecision {
    /// `rejected` (refused by the screening) or `hidden` (masked idea).
    pub measure: String,
    /// Refusal reason code, or `hidden_reason` of a masking.
    #[schema(required = true)]
    pub reason_code: Option<String>,
}

/// A human review requested by an author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminReviewRequest {
    pub id: Uuid,
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    /// The idea concerned (the text itself, or the idea of a contribution or a report).
    #[schema(required = true)]
    pub idea: Option<AdminIdeaRef>,
    /// Current status of the text.
    #[schema(required = true)]
    pub target_status: Option<String>,
    /// Masked text under review.
    #[schema(required = true)]
    pub text: Option<String>,
    /// Masked message of the author.
    #[schema(required = true)]
    pub message: Option<String>,
    /// The decision still in force (`null` once overturned or when the text changed since).
    #[schema(required = true)]
    pub decision: Option<AdminReviewedDecision>,
    pub status: ReviewRequestStatus,
    #[schema(required = true)]
    pub resolution: Option<ReviewResolution>,
    #[schema(required = true)]
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    #[schema(required = true)]
    pub handled_at: Option<DateTime<Utc>>,
}

impl TryFrom<ReviewRequestRow> for AdminReviewRequest {
    type Error = ApiError;

    fn try_from(row: ReviewRequestRow) -> Result<Self, ApiError> {
        let decision = match row.target_status.as_deref() {
            Some("rejected") => Some(AdminReviewedDecision {
                measure: "rejected".to_owned(),
                reason_code: row.rejection_reason_code,
            }),
            Some("hidden") => Some(AdminReviewedDecision {
                measure: "hidden".to_owned(),
                reason_code: row.hidden_reason,
            }),
            _ => None,
        };
        let idea = match (row.idea_id, row.idea_status) {
            (Some(id), Some(status)) => Some(AdminIdeaRef {
                id,
                slug: row.idea_slug,
                title: row.idea_title,
                status,
            }),
            _ => None,
        };
        Ok(Self {
            id: row.id,
            target_kind: known(&row.target_kind)?,
            target_id: row.target_id,
            idea,
            target_status: row.target_status,
            text: row.text,
            message: row.message,
            decision,
            status: known(&row.status)?,
            resolution: row.resolution.as_deref().map(known).transpose()?,
            note: row.note,
            created_at: row.created_at,
            handled_at: row.handled_at,
        })
    }
}

page_type!(
    /// A page of review requests: pending ones first, oldest first.
    AdminReviewRequestPage,
    AdminReviewRequest
);

/// The human review requests.
#[utoipa::path(
    get,
    path = "/api/admin/review-requests",
    tag = "admin",
    operation_id = "listAdminReviewRequests",
    security(("admin_token" = [])),
    params(AdminQueueQuery),
    responses(
        (status = 200, description = "Une page de demandes de réexamen.",
         body = AdminReviewRequestPage),
        (status = 400, description = "Paramètre invalide.", body = ErrorBody),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn review_requests(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AdminQueueQuery>,
) -> Result<Json<AdminReviewRequestPage>, ApiError> {
    let pages = PageRequest::parse(query.page, query.per_page)?;
    let (rows, total) =
        admin_moderation::review_requests(&state.pool, query.status, pages.limit(), pages.offset())
            .await?;
    let items = rows
        .into_iter()
        .map(AdminReviewRequest::try_from)
        .collect::<Result<_, _>>()?;
    Ok(Json(AdminReviewRequestPage::new(items, pages, total)))
}

/// Decision of the administrator on a review request.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminReviewResolution {
    pub resolution: ReviewResolution,
    /// Note of the administrator (2 000 characters), never shown to the author.
    #[serde(default)]
    pub note: Option<String>,
}

/// Closes a review request. `overturned`: a refused text becomes `accepted` and resumes the
/// pipeline (duplicate detection, re-analysis or report review), a masked idea is republished;
/// `confirmed`: the decision stands and the suspended purge resumes.
#[utoipa::path(
    post,
    path = "/api/admin/review-requests/{id}/resolve",
    tag = "admin",
    operation_id = "resolveAdminReviewRequest",
    security(("admin_token" = [])),
    params(("id" = Uuid, Path, description = "Identifiant de la demande")),
    request_body = AdminReviewResolution,
    responses(
        (status = 200, description = "Demande traitée.", body = AdminReviewRequest),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody),
        (status = 404, description = "Demande inconnue (`review_request_not_found`).",
         body = ErrorBody),
        (status = 409, description = "Déjà traitée (`review_request_already_resolved`) ou décision \
                                      qui ne peut plus être infirmée (`decision_not_overturnable`).",
         body = ErrorBody),
        (status = 422, description = "Corps invalide.", body = ErrorBody)
    )
)]
pub async fn resolve_review_request(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AdminReviewResolution>,
) -> Result<Json<AdminReviewRequest>, ApiError> {
    let note = admin_text(body.note.as_deref(), NOTE_MAX_CHARS, false, "La note")?;
    let mut tx = state.pool.begin().await?;
    let (kind, target_id, pending) = admin_moderation::lock_review_request(&mut tx, id)
        .await?
        .ok_or_else(|| {
            not_found(
                "review_request_not_found",
                "Cette demande de réexamen n'existe pas.",
            )
        })?;
    if !pending {
        return Err(conflict(
            "review_request_already_resolved",
            "Cette demande de réexamen a déjà été traitée.",
        ));
    }
    let target = Target::new(known(&kind)?, target_id);
    let mut effect = "none";
    if body.resolution == ReviewResolution::Overturned {
        let change = Change::by(Actor::Admin)
            .reason("review_overturned")
            .details(json!({ "review_request_id": id }));
        if moderation::overturn_rejection(&mut tx, target, &change)
            .await
            .map_err(|err| match err {
                moderation::ModerationError::Status(err) => status_error(err),
                moderation::ModerationError::Database(err) => err.into(),
            })?
        {
            effect = "accepted";
        } else if target.kind == TargetKind::Idea
            && ideas::republish(&mut tx, target.id, &change)
                .await
                .map_err(status_error)?
        {
            queue::enqueue(&mut *tx, &pipeline::exports_job()).await?;
            effect = "republished";
        } else {
            return Err(conflict(
                "decision_not_overturnable",
                "La décision ne peut plus être infirmée : le texte n'est plus refusé ni masqué.",
            ));
        }
    }
    admin_moderation::resolve_review_request(&mut tx, id, body.resolution, note.as_deref()).await?;
    admin_actions::record(
        &mut tx,
        &NewAdminAction::new("review_request.resolve")
            .target("review_request", id)
            .details(json!({
                "resolution": body.resolution.as_str(),
                "target_kind": target.kind.as_str(),
                "target_id": target.id,
                "effect": effect,
                "note": note,
            })),
    )
    .await?;
    let row = admin_moderation::review_request(&mut tx, id)
        .await?
        .ok_or_else(ApiError::internal)?;
    tx.commit().await?;
    Ok(Json(row.try_into()?))
}
