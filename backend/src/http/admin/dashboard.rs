//! `GET /api/admin/dashboard` (AGENTS.md §8): job queue by kind and status, screening rates,
//! AI costs of the day and of the month by purpose, remaining daily budget, pending queues and
//! configuration alerts (Mistral API key refused, a model that refuses `reasoning_effort`).

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use super::euros;
use crate::ai::budget;
use crate::config::MistralConfig;
use crate::db::{dashboard, provider_alerts};
use crate::domain::types::ReasoningEffort;
use crate::http::AppState;
use crate::http::error::{ApiError, ErrorBody};
use crate::jobs::queue;

/// Days covered by the screening rates.
pub const SCREENING_WINDOW_DAYS: i32 = 30;

/// Jobs of one kind in one status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminJobCount {
    pub kind: String,
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminJobSummary {
    pub queued: i64,
    pub running: i64,
    pub succeeded: i64,
    pub dead: i64,
    pub by_kind: Vec<AdminJobCount>,
}

/// Screening outcomes of one kind of text over the window. Rates are shares of the
/// screenings that reached the decision model (`null` without any).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminScreeningRates {
    /// `idea`, `contribution` or `report`.
    pub kind: String,
    pub total: i64,
    pub accepted: i64,
    pub rejected: i64,
    pub gray_zone: i64,
    /// Decision service failures (retried by a job).
    pub errors: i64,
    #[schema(required = true)]
    pub acceptance_rate: Option<f64>,
    #[schema(required = true)]
    pub rejection_rate: Option<f64>,
    #[schema(required = true)]
    pub gray_zone_rate: Option<f64>,
    pub second_opinion_accept: i64,
    pub second_opinion_reject: i64,
    pub labeled: i64,
}

/// AI calls and estimated costs of one purpose.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminPurposeCost {
    /// `screening`, `second_opinion`, `analysis_research`, `analysis_structuring`,
    /// `embedding` or `report_review`.
    pub purpose: String,
    /// `decision_service` or `mistral`.
    pub provider: String,
    /// `reasoning_effort` sent to Mistral (§6.1): `high`, `none`, or `null` when omitted or not
    /// applicable. A purpose whose effort changed during the month has one row per effort.
    #[schema(required = true)]
    pub reasoning_effort: Option<ReasoningEffort>,
    pub calls_today: i64,
    pub calls_month: i64,
    pub today_eur: f64,
    pub month_eur: f64,
}

/// Estimated AI costs (euros) of the Paris day and month, and the daily budget.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminAiCosts {
    pub today_eur: f64,
    pub month_eur: f64,
    pub daily_budget_eur: f64,
    pub remaining_today_eur: f64,
    /// Mistral jobs wait for the next reset.
    pub budget_reached: bool,
    /// Next Paris midnight.
    pub next_reset: DateTime<Utc>,
    pub by_purpose: Vec<AdminPurposeCost>,
}

/// What waits for the administrator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminQueues {
    pub escalations_pending: i64,
    pub review_requests_pending: i64,
    pub ideas_analysis_failed: i64,
    pub texts_second_opinion: i64,
    /// Texts still waiting for the decision service after an hour.
    pub texts_submitted_stale: i64,
    pub ideas_hidden: i64,
    pub reports_awaiting_review: i64,
    pub jobs_dead: i64,
}

/// A configuration alert in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdminAlert {
    /// `mistral_auth`: Mistral refuses the API key (HTTP 401 or 403); the jobs concerned wait
    /// (no attempt consumed) until the first accepted call ends it.
    /// `mistral_reasoning_unsupported`: a configured model refuses `reasoning_effort` (§6.1);
    /// its calls go without it, nothing waits; it ends when the model changes or takes it.
    pub code: String,
    /// First refusal of the alert.
    pub since: DateTime<Utc>,
    /// Last refusal.
    pub last_seen_at: DateTime<Utc>,
    #[schema(required = true)]
    pub http_status: Option<i32>,
    /// Refused calls since the beginning of the alert.
    pub failures: i64,
    /// French message to display as is.
    pub message: String,
}

/// The alert of a model that refuses `reasoning_effort`, if a configured tier sends it an
/// effort (otherwise the refusal is stale: the model is not used with the parameter any more).
pub fn reasoning_alert(
    refusal: provider_alerts::ReasoningRefusal,
    config: &MistralConfig,
) -> Option<AdminAlert> {
    let effort =
        |model: &str, effort: Option<ReasoningEffort>| effort.filter(|_| model == refusal.model);
    let analysis = effort(&config.analysis_model, config.analysis_reasoning_effort);
    let review = effort(&config.review_model, config.review_reasoning_effort);
    let (variable, configured) = match (analysis, review) {
        (Some(analysis), _) => ("MISTRAL_ANALYSIS_MODEL", analysis),
        (None, Some(review)) => ("MISTRAL_REVIEW_MODEL", review),
        (None, None) => return None,
    };
    Some(AdminAlert {
        code: provider_alerts::MISTRAL_REASONING_UNSUPPORTED.to_owned(),
        message: format!(
            "Le modèle {model} refuse le paramètre reasoning_effort depuis le {since}              (HTTP {status}) : ses appels partent sans lui, avec l'effort de raisonnement par              défaut de Mistral au lieu de « {configured} ». Pour appliquer l'effort voulu,              définir {variable} sur un modèle qui le prend en charge, par exemple              mistral-large-4-0.",
            model = refusal.model,
            since = refusal.since_paris,
            status = refusal.last_http_status,
            configured = configured.as_str(),
        ),
        since: refusal.since,
        last_seen_at: refusal.last_seen_at,
        http_status: Some(refusal.last_http_status),
        failures: refusal.refusals,
    })
}

impl From<provider_alerts::ProviderAlert> for AdminAlert {
    fn from(alert: provider_alerts::ProviderAlert) -> Self {
        let status = alert
            .last_http_status
            .map(|s| format!(" (HTTP {s})"))
            .unwrap_or_default();
        Self {
            message: format!(
                "Clé Mistral refusée depuis le {}{status} : les analyses, dédoublonnages, \
                 seconds avis et revues de signalement attendent sans échouer. Vérifier \
                 MISTRAL_API_KEY ; tout reprend au premier appel accepté.",
                alert.since_paris
            ),
            code: alert.alert,
            since: alert.since,
            last_seen_at: alert.last_seen_at,
            http_status: alert.last_http_status,
            failures: alert.failures,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminDashboard {
    /// Configuration alerts in progress, oldest first (empty when all is well). Always sent;
    /// declared optional so that clients built before it keep compiling.
    #[schema(required = false)]
    pub alerts: Vec<AdminAlert>,
    pub jobs: AdminJobSummary,
    pub screening_window_days: i32,
    pub screening: Vec<AdminScreeningRates>,
    pub ai_costs: AdminAiCosts,
    pub queues: AdminQueues,
}

fn rate(part: i64, total: i64) -> Option<f64> {
    (total > 0).then(|| part as f64 / total as f64)
}

/// Dashboard of the administrator.
#[utoipa::path(
    get,
    path = "/api/admin/dashboard",
    tag = "admin",
    operation_id = "getAdminDashboard",
    security(("admin_token" = [])),
    responses(
        (status = 200, description = "Tableau de bord.", body = AdminDashboard),
        (status = 401, description = "Jeton absent ou invalide.", body = ErrorBody)
    )
)]
pub async fn dashboard(State(state): State<AppState>) -> Result<Json<AdminDashboard>, ApiError> {
    let pool = &state.pool;
    let counts = queue::counts(pool).await?;
    let total_of = |status: &str| {
        counts
            .iter()
            .filter(|c| c.status == status)
            .map(|c| c.count)
            .sum()
    };
    let jobs = AdminJobSummary {
        queued: total_of("queued"),
        running: total_of("running"),
        succeeded: total_of("succeeded"),
        dead: total_of("dead"),
        by_kind: counts
            .iter()
            .map(|c| AdminJobCount {
                kind: c.kind.clone(),
                status: c.status.clone(),
                count: c.count,
            })
            .collect(),
    };

    let screening = dashboard::screening_counts(pool, SCREENING_WINDOW_DAYS)
        .await?
        .into_iter()
        .map(|c| {
            let decided = c.accepted + c.rejected + c.gray_zone;
            AdminScreeningRates {
                acceptance_rate: rate(c.accepted, decided),
                rejection_rate: rate(c.rejected, decided),
                gray_zone_rate: rate(c.gray_zone, decided),
                kind: c.target_kind,
                total: c.total,
                accepted: c.accepted,
                rejected: c.rejected,
                gray_zone: c.gray_zone,
                errors: c.errors,
                second_opinion_accept: c.second_opinion_accept,
                second_opinion_reject: c.second_opinion_reject,
                labeled: c.labeled,
            }
        })
        .collect();

    let budget = budget::status(pool, state.config.mistral.daily_budget).await?;
    let costs = dashboard::costs_by_purpose(pool).await?;
    let month_micro: i64 = costs.iter().map(|c| c.cost_month_micro_eur).sum();
    let by_purpose: Vec<AdminPurposeCost> = costs
        .into_iter()
        .map(|c| AdminPurposeCost {
            purpose: c.purpose,
            provider: c.provider,
            reasoning_effort: c.reasoning_effort.and_then(|e| e.parse().ok()),
            calls_today: c.calls_today,
            calls_month: c.calls_month,
            today_eur: euros(c.cost_today_micro_eur),
            month_eur: euros(c.cost_month_micro_eur),
        })
        .collect();
    let ai_costs = AdminAiCosts {
        today_eur: euros(budget.spent.0),
        month_eur: euros(month_micro),
        daily_budget_eur: euros(budget.limit.0),
        remaining_today_eur: euros(budget.remaining().0),
        budget_reached: budget.exceeded(),
        next_reset: budget.next_reset,
        by_purpose,
    };

    let q = dashboard::queues(pool).await?;
    let queues = AdminQueues {
        escalations_pending: q.escalations_pending,
        review_requests_pending: q.review_requests_pending,
        ideas_analysis_failed: q.ideas_analysis_failed,
        texts_second_opinion: q.texts_second_opinion,
        texts_submitted_stale: q.texts_submitted_stale,
        ideas_hidden: q.ideas_hidden,
        reports_awaiting_review: q.reports_awaiting_review,
        jobs_dead: q.jobs_dead,
    };

    let mut alerts: Vec<AdminAlert> = provider_alerts::all(pool)
        .await?
        .into_iter()
        .map(AdminAlert::from)
        .collect();
    alerts.extend(
        provider_alerts::reasoning_refusals(pool)
            .await?
            .into_iter()
            .filter_map(|refusal| reasoning_alert(refusal, &state.config.mistral)),
    );

    Ok(Json(AdminDashboard {
        alerts,
        jobs,
        screening_window_days: SCREENING_WINDOW_DAYS,
        screening,
        ai_costs,
        queues,
    }))
}
