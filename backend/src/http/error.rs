//! Unified JSON error type (AGENTS.md §4.1):
//! `{ "error": { "code": "snake_case_code", "message": "…" } }`, message in French.

use std::borrow::Cow;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::validation::TextError;

/// Body of every error response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable machine-readable code, in snake_case.
    #[schema(example = "not_found")]
    pub code: String,
    /// Human-readable message, in French when shown to visitors.
    #[schema(example = "Ressource introuvable.")]
    pub message: String,
}

/// An error returned by a handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: Cow<'static, str>,
    pub message: Cow<'static, str>,
    /// Seconds before retrying (429, 503).
    pub retry_after: Option<u64>,
}

impl ApiError {
    pub fn new(
        status: StatusCode,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }

    pub fn bad_request(
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    /// 422: syntactically valid request whose content is refused (validation).
    pub fn validation(
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    pub fn not_found() -> Self {
        Self::from_status(StatusCode::NOT_FOUND)
    }

    pub fn unauthorized() -> Self {
        Self::from_status(StatusCode::UNAUTHORIZED)
    }

    pub fn too_many_requests(retry_after: u64) -> Self {
        Self::from_status(StatusCode::TOO_MANY_REQUESTS).with_retry_after(retry_after)
    }

    pub fn internal() -> Self {
        Self::from_status(StatusCode::INTERNAL_SERVER_ERROR)
    }

    pub fn service_unavailable(
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }

    /// Generic error for a status code (used for errors produced by middlewares).
    pub fn from_status(status: StatusCode) -> Self {
        let (code, message) = match status {
            StatusCode::BAD_REQUEST => ("bad_request", "Requête invalide."),
            StatusCode::UNAUTHORIZED => ("unauthorized", "Authentification requise."),
            StatusCode::FORBIDDEN => ("forbidden", "Accès refusé."),
            StatusCode::NOT_FOUND => ("not_found", "Ressource introuvable."),
            StatusCode::METHOD_NOT_ALLOWED => ("method_not_allowed", "Méthode non autorisée."),
            StatusCode::NOT_ACCEPTABLE => ("not_acceptable", "Format de réponse non disponible."),
            StatusCode::REQUEST_TIMEOUT => ("request_timeout", "La requête a expiré."),
            StatusCode::CONFLICT => ("conflict", "Conflit avec l'état actuel de la ressource."),
            StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "Requête trop volumineuse."),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => (
                "unsupported_media_type",
                "Type de contenu non pris en charge (JSON attendu).",
            ),
            StatusCode::UNPROCESSABLE_ENTITY => ("invalid_request", "Données invalides."),
            StatusCode::TOO_MANY_REQUESTS => (
                "too_many_requests",
                "Trop de requêtes : veuillez réessayer plus tard.",
            ),
            StatusCode::SERVICE_UNAVAILABLE => (
                "service_unavailable",
                "Service momentanément indisponible, veuillez réessayer plus tard.",
            ),
            StatusCode::GATEWAY_TIMEOUT => ("timeout", "La requête a pris trop de temps."),
            s if s.is_server_error() => (
                "internal_error",
                "Erreur interne, veuillez réessayer plus tard.",
            ),
            _ => ("http_error", "La requête n'a pas pu aboutir."),
        };
        Self::new(status, code, message)
    }

    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            error: ErrorDetail {
                code: self.code.to_string(),
                message: self.message.to_string(),
            },
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.status.as_u16(), self.code)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, axum::Json(self.body())).into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        if self.status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        // Database messages never contain user data at this level, only identifiers.
        tracing::error!(error = %err, "database error");
        match err {
            sqlx::Error::PoolTimedOut => Self::service_unavailable(
                "database_unavailable",
                "Service momentanément indisponible, veuillez réessayer plus tard.",
            ),
            _ => Self::internal(),
        }
    }
}

impl From<TextError> for ApiError {
    fn from(err: TextError) -> Self {
        Self::validation(err.code(), err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_of(response: Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn serializes_the_unified_shape() {
        let response = ApiError::bad_request("bad_thing", "Mauvaise chose.").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(
            body_of(response).await,
            serde_json::json!({ "error": { "code": "bad_thing", "message": "Mauvaise chose." } })
        );
    }

    #[tokio::test]
    async fn adds_retry_after_and_www_authenticate() {
        let response = ApiError::too_many_requests(120).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "120");
        let response = ApiError::unauthorized().into_response();
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
    }

    #[test]
    fn every_status_has_a_snake_case_code() {
        for code in 400u16..600 {
            let Ok(status) = StatusCode::from_u16(code) else {
                continue;
            };
            let err = ApiError::from_status(status);
            assert!(
                err.code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{status}: {}",
                err.code
            );
            assert!(!err.message.is_empty());
        }
        assert_eq!(
            ApiError::from_status(StatusCode::IM_A_TEAPOT).code,
            "http_error"
        );
        assert_eq!(
            ApiError::from_status(StatusCode::BAD_GATEWAY).code,
            "internal_error"
        );
    }

    #[test]
    fn text_errors_become_422() {
        let err = ApiError::from(TextError::TooShort { min: 50, actual: 3 });
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code, "text_too_short");
        assert!(err.message.contains("50"));
    }

    #[test]
    fn pool_timeouts_are_503() {
        assert_eq!(
            ApiError::from(sqlx::Error::PoolTimedOut).status,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            ApiError::from(sqlx::Error::RowNotFound).status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
