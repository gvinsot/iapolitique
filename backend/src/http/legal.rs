//! `GET /api/legal` (AGENTS.md §7.5): identity and contacts of the publisher, read from the
//! `LEGAL_*` variables (never committed), version of the publication rules and dates of the
//! legal pages. A detail that is not configured is `null`: the pages then show
//! « coordonnées momentanément indisponibles ».

use axum::extract::State;
use axum::response::Response;
use chrono::NaiveDate;
use serde::Serialize;
use utoipa::ToSchema;

use super::AppState;
use super::cache;
use super::error::ApiError;
use crate::domain::rules::{CGU_VERSION, LEGAL_PAGES_UPDATED_ON};

/// The publisher (a natural person, LCEN art. 1-1); also the host of the site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LegalPublisher {
    #[schema(required = true)]
    pub name: Option<String>,
    #[schema(required = true)]
    pub address: Option<String>,
    #[schema(required = true)]
    pub phone: Option<String>,
}

/// Day of the last update of each legal page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LegalPages {
    pub mentions_legales: NaiveDate,
    pub confidentialite: NaiveDate,
    pub regles: NaiveDate,
    pub licence: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LegalInfo {
    pub publisher: LegalPublisher,
    #[schema(required = true)]
    pub contact_email: Option<String>,
    /// Address for personal data requests.
    #[schema(required = true)]
    pub privacy_email: Option<String>,
    /// Version of the publication rules accepted with each idea and contribution.
    #[schema(example = "1.0")]
    pub cgu_version: String,
    pub pages: LegalPages,
}

fn date(value: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| ApiError::internal())
}

/// Publisher details, version of the rules and dates of the legal pages.
#[utoipa::path(
    get,
    path = "/api/legal",
    tag = "reference",
    operation_id = "getLegal",
    responses((status = 200, description = "Mentions légales.", body = LegalInfo))
)]
pub async fn legal(State(state): State<AppState>) -> Result<Response, ApiError> {
    let config = &state.config.legal;
    let pages = LEGAL_PAGES_UPDATED_ON;
    Ok(cache::json(
        LegalInfo {
            publisher: LegalPublisher {
                name: config.publisher_name.clone(),
                address: config.publisher_address.clone(),
                phone: config.publisher_phone.clone(),
            },
            contact_email: config.contact_email.clone(),
            privacy_email: config.privacy_email.clone(),
            cgu_version: CGU_VERSION.to_owned(),
            pages: LegalPages {
                mentions_legales: date(pages.mentions_legales)?,
                confidentialite: date(pages.confidentialite)?,
                regles: date(pages.regles)?,
                licence: date(pages.licence)?,
            },
        },
        cache::MEDIUM,
    ))
}

#[cfg(test)]
mod tests {
    use crate::domain::rules::LEGAL_PAGES_UPDATED_ON;
    use crate::test_support::{TestApp, get_json};
    use axum::http::{StatusCode, header};
    use serde_json::{Value, json};
    use sqlx::PgPool;

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn serves_the_configured_publisher(pool: PgPool) {
        let app = TestApp::new(
            pool,
            false,
            &[
                ("LEGAL_PUBLISHER_NAME", "Camille Exemple"),
                ("LEGAL_PUBLISHER_ADDRESS", "1 rue de l'Exemple, 75000 Paris"),
                ("LEGAL_PUBLISHER_PHONE", "01 23 45 67 89"),
                ("LEGAL_CONTACT_EMAIL", "contact@example.org"),
                ("LEGAL_PRIVACY_EMAIL", "rgpd@example.org"),
            ],
        )
        .await;
        let (status, headers, body) = get_json(&app.router, "/api/legal", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
        assert_eq!(
            body,
            json!({
                "publisher": {
                    "name": "Camille Exemple",
                    "address": "1 rue de l'Exemple, 75000 Paris",
                    "phone": "01 23 45 67 89",
                },
                "contact_email": "contact@example.org",
                "privacy_email": "rgpd@example.org",
                "cgu_version": crate::domain::rules::CGU_VERSION,
                "pages": {
                    "mentions_legales": LEGAL_PAGES_UPDATED_ON.mentions_legales,
                    "confidentialite": LEGAL_PAGES_UPDATED_ON.confidentialite,
                    "regles": LEGAL_PAGES_UPDATED_ON.regles,
                    "licence": LEGAL_PAGES_UPDATED_ON.licence,
                },
            })
        );
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn missing_details_are_null(pool: PgPool) {
        let app = TestApp::new(pool, false, &[]).await;
        let (status, _, body) = get_json(&app.router, "/api/legal", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["publisher"]["name"], Value::Null);
        assert_eq!(body["contact_email"], Value::Null);
        assert!(body["pages"]["regles"].is_string());
    }
}
