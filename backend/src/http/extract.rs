//! Extractors whose rejections use the unified JSON error format (axum's defaults answer in
//! plain text). Rejection details are returned to the caller but never logged: they may quote
//! the submitted content.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use super::error::ApiError;

/// JSON body.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_rejection(&rejection)),
        }
    }
}

fn json_rejection(rejection: &JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::JsonDataError(err) => ApiError::validation(
            "invalid_body",
            format!("Contenu de la requête invalide : {}", err.body_text()),
        ),
        JsonRejection::JsonSyntaxError(_) => ApiError::bad_request(
            "malformed_json",
            "Le corps de la requête n'est pas du JSON valide.",
        ),
        JsonRejection::MissingJsonContentType(_) => {
            ApiError::from_status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        }
        other => ApiError::from_status(other.status()),
    }
}

/// Query string.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Self(value)),
            Err(QueryRejection::FailedToDeserializeQueryString(err)) => Err(ApiError::bad_request(
                "invalid_query",
                format!("Paramètres invalides : {}", err.body_text()),
            )),
            Err(other) => Err(ApiError::from_status(other.status())),
        }
    }
}

/// Path parameters. A malformed parameter is answered with 404, like a missing resource.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiPath<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Self(value)),
            Err(PathRejection::FailedToDeserializePathParams(_)) => Err(ApiError::not_found()),
            Err(other) => Err(ApiError::from_status(other.status())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::{get, post};
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Debug, Deserialize)]
    struct Payload {
        count: u32,
    }

    fn app() -> Router {
        Router::new()
            .route(
                "/json",
                post(|ApiJson(p): ApiJson<Payload>| async move { p.count.to_string() }),
            )
            .route(
                "/query",
                get(|ApiQuery(p): ApiQuery<Payload>| async move { p.count.to_string() }),
            )
            .route(
                "/path/{count}",
                get(|ApiPath(count): ApiPath<u32>| async move { count.to_string() }),
            )
    }

    async fn call(request: axum::http::Request<Body>) -> (StatusCode, String) {
        let response = app().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    fn json(body: &'static str) -> axum::http::Request<Body> {
        axum::http::Request::post("/json")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    fn code(body: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        value["error"]["code"].as_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn json_extractor() {
        assert_eq!(
            call(json(r#"{"count": 3}"#)).await,
            (StatusCode::OK, "3".into())
        );

        let (status, body) = call(json("{")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(code(&body), "malformed_json");

        let (status, body) = call(json(r#"{"count": -1}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&body), "invalid_body");

        let request = axum::http::Request::post("/json")
            .body(Body::from(r#"{"count": 3}"#))
            .unwrap();
        let (status, body) = call(request).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(code(&body), "unsupported_media_type");
    }

    #[tokio::test]
    async fn query_and_path_extractors() {
        let get = |uri: &str| axum::http::Request::get(uri).body(Body::empty()).unwrap();
        assert_eq!(
            call(get("/query?count=4")).await,
            (StatusCode::OK, "4".into())
        );
        let (status, body) = call(get("/query?count=x")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(code(&body), "invalid_query");

        assert_eq!(call(get("/path/5")).await, (StatusCode::OK, "5".into()));
        let (status, body) = call(get("/path/abc")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code(&body), "not_found");
    }
}
