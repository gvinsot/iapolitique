//! Open data downloads (AGENTS.md §7.4, §13.2): the latest generation of each export file,
//! served from the `exports` table (never generated on demand, §14.5).
//!
//! Every header is built from fixed names and from the database (§14.1): the download name is
//! `iapolitique-<file>-<day of the generation>.<extension>`, the `ETag` is the SHA-256 of the
//! content (conditional requests answer `304`).

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::NaiveDate;

use super::AppState;
use super::cache;
use super::error::{ApiError, ErrorBody};
use crate::db::exports::{self, StoredExport};
use crate::domain::types::ExportFormat;

/// Seconds suggested before asking again when no export exists yet.
pub const RETRY_AFTER_SECS: u64 = 300;

fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "exports_unavailable",
        "Les exports ne sont pas encore disponibles : ils sont générés chaque nuit et après \
         chaque publication. Réessayez dans quelques minutes.",
    )
    .with_retry_after(RETRY_AFTER_SECS)
}

/// Download name: fixed prefix and stem, day of the generation, fixed extension.
pub fn download_name(format: ExportFormat, generated_on: NaiveDate) -> String {
    let (stem, extension) = match format {
        ExportFormat::Json => ("ideas", "json"),
        ExportFormat::Csv => ("ideas", "csv"),
        ExportFormat::License => ("LICENSE", "txt"),
        ExportFormat::Metadata => ("metadata", "json"),
    };
    format!(
        "iapolitique-{stem}-{}.{extension}",
        generated_on.format("%Y-%m-%d")
    )
}

fn etag(file: &StoredExport) -> String {
    let hex: String = file
        .content_sha256
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("\"{hex}\"")
}

/// Whether `If-None-Match` lists this entity tag (or `*`).
fn not_modified(request: &HeaderMap, etag: &str) -> bool {
    request
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|tag| tag.trim().trim_start_matches("W/"))
        .any(|tag| tag == "*" || tag == etag)
}

async fn serve(
    state: &AppState,
    request: &HeaderMap,
    format: ExportFormat,
) -> Result<Response, ApiError> {
    let file = exports::latest(&state.pool, format)
        .await?
        .ok_or_else(unavailable)?;
    let etag = etag(&file);
    let etag_value = HeaderValue::from_str(&etag).map_err(|_| ApiError::internal())?;
    let last_modified = HeaderValue::from_str(
        &file
            .generated_at
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string(),
    )
    .map_err(|_| ApiError::internal())?;
    if not_modified(request, &etag) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        let headers = response.headers_mut();
        headers.insert(header::ETAG, etag_value);
        headers.insert(header::LAST_MODIFIED, last_modified);
        return Ok(cache::with_cache_control(response, cache::MEDIUM));
    }
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        download_name(format, file.generated_on)
    ))
    .map_err(|_| ApiError::internal())?;
    let mut response = file.content.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(format.content_type()),
    );
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(header::ETAG, etag_value);
    headers.insert(header::LAST_MODIFIED, last_modified);
    Ok(cache::with_cache_control(response, cache::MEDIUM))
}

/// Published ideas with their analyses, metadata and licence (JSON).
#[utoipa::path(
    get,
    path = "/api/exports/ideas.json",
    tag = "exports",
    operation_id = "downloadIdeasJson",
    responses(
        (status = 200, description = "Dernier export JSON.", content_type = "application/json",
         body = serde_json::Value),
        (status = 304, description = "Inchangé depuis la version en cache (`If-None-Match`)."),
        (status = 503, description = "Aucun export généré pour l'instant (`exports_unavailable`).",
         body = ErrorBody)
    )
)]
pub async fn ideas_json(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve(&state, &headers, ExportFormat::Json).await
}

/// One row per published idea (CSV, UTF-8 with BOM).
#[utoipa::path(
    get,
    path = "/api/exports/ideas.csv",
    tag = "exports",
    operation_id = "downloadIdeasCsv",
    responses(
        (status = 200, description = "Dernier export CSV.", content_type = "text/csv",
         body = String),
        (status = 304, description = "Inchangé depuis la version en cache (`If-None-Match`)."),
        (status = 503, description = "Aucun export généré pour l'instant (`exports_unavailable`).",
         body = ErrorBody)
    )
)]
pub async fn ideas_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve(&state, &headers, ExportFormat::Csv).await
}

/// Text of the Licence Ouverte 2.0.
#[utoipa::path(
    get,
    path = "/api/exports/LICENSE",
    tag = "exports",
    operation_id = "downloadLicense",
    responses(
        (status = 200, description = "Texte de la licence.", content_type = "text/plain",
         body = String),
        (status = 304, description = "Inchangé depuis la version en cache (`If-None-Match`)."),
        (status = 503, description = "Aucun export généré pour l'instant (`exports_unavailable`).",
         body = ErrorBody)
    )
)]
pub async fn license(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve(&state, &headers, ExportFormat::License).await
}

/// Metadata of the exports (licence, producer, notices, files).
#[utoipa::path(
    get,
    path = "/api/exports/metadata.json",
    tag = "exports",
    operation_id = "downloadExportMetadata",
    responses(
        (status = 200, description = "Métadonnées du dernier export.",
         content_type = "application/json", body = serde_json::Value),
        (status = 304, description = "Inchangé depuis la version en cache (`If-None-Match`)."),
        (status = 503, description = "Aucun export généré pour l'instant (`exports_unavailable`).",
         body = ErrorBody)
    )
)]
pub async fn metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve(&state, &headers, ExportFormat::Metadata).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::exports::{ExportFile, insert};
    use crate::test_support::{TestApp, get_json};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use sqlx::PgPool;
    use tower::ServiceExt;

    #[test]
    fn download_names_are_fixed_and_dated() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        assert_eq!(
            download_name(ExportFormat::Json, day),
            "iapolitique-ideas-2026-10-19.json"
        );
        assert_eq!(
            download_name(ExportFormat::Csv, day),
            "iapolitique-ideas-2026-10-19.csv"
        );
        assert_eq!(
            download_name(ExportFormat::License, day),
            "iapolitique-LICENSE-2026-10-19.txt"
        );
        assert_eq!(
            download_name(ExportFormat::Metadata, day),
            "iapolitique-metadata-2026-10-19.json"
        );
    }

    async fn generation(pool: &PgPool, marker: &str) {
        let files: Vec<ExportFile> = ExportFormat::ALL
            .iter()
            .map(|&format| ExportFile {
                format,
                content: format!("{marker} {}", format.as_str()).into_bytes(),
            })
            .collect();
        let mut tx = pool.begin().await.unwrap();
        insert(&mut tx, &files, 1, "1", "1.0").await.unwrap();
        tx.commit().await.unwrap();
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn serves_the_latest_generation_of_each_file(pool: PgPool) {
        let app = TestApp::new(pool.clone(), false, &[]).await;
        for path in ["ideas.json", "ideas.csv", "LICENSE", "metadata.json"] {
            let (status, headers, body) =
                get_json(&app.router, &format!("/api/exports/{path}"), &[]).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
            assert_eq!(body["error"]["code"], "exports_unavailable");
            assert_eq!(headers[header::RETRY_AFTER], "300");
        }

        generation(&pool, "old").await;
        sqlx::query("UPDATE exports SET generated_at = now() - interval '1 day'")
            .execute(&pool)
            .await
            .unwrap();
        generation(&pool, "new").await;
        let today: NaiveDate =
            sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Paris')::date")
                .fetch_one(&pool)
                .await
                .unwrap();

        for (path, format) in [
            ("ideas.json", ExportFormat::Json),
            ("ideas.csv", ExportFormat::Csv),
            ("LICENSE", ExportFormat::License),
            ("metadata.json", ExportFormat::Metadata),
        ] {
            let request = Request::get(format!("/api/exports/{path}"))
                .body(Body::empty())
                .unwrap();
            let response = app.router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let headers = response.headers().clone();
            assert_eq!(headers[header::CONTENT_TYPE], format.content_type());
            assert_eq!(
                headers[header::CONTENT_DISPOSITION],
                format!("attachment; filename=\"{}\"", download_name(format, today)).as_str()
            );
            assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
            assert!(
                headers[header::LAST_MODIFIED]
                    .to_str()
                    .unwrap()
                    .ends_with("GMT")
            );
            let etag = headers[header::ETAG].to_str().unwrap().to_owned();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body, format!("new {}", format.as_str()).as_bytes());

            // Conditional request.
            let request = Request::get(format!("/api/exports/{path}"))
                .header(header::IF_NONE_MATCH, format!("\"other\", {etag}"))
                .body(Body::empty())
                .unwrap();
            let response = app.router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{path}");
            assert_eq!(response.headers()[header::ETAG], etag.as_str());
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert!(body.is_empty());
        }

        let (status, _, _) = get_json(&app.router, "/api/exports/other.json", &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
