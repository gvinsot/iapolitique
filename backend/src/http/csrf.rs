//! No CORS and anti-CSRF (AGENTS.md §14.3), on the public and admin ports.
//!
//! - `OPTIONS` (CORS preflight or not) is refused with `405`: the API is same-origin only and
//!   never emits `Access-Control-*` (they are also stripped by `security.rs`).
//! - Every request other than `GET` / `HEAD`:
//!   1. `Content-Type` must be `application/json` (`415` otherwise): HTML forms, the only way to
//!      send a cross-site request without a preflight and with a body, cannot produce it. A
//!      request **without body** may omit it (`openapi-fetch` sends none for a bodyless
//!      `POST` / `DELETE`: job retry, label removal…).
//!   2. `Origin` must be one of the allowed origins of the port; without `Origin`,
//!      `Sec-Fetch-Site` must be `same-origin` or `none`. `Sec-Fetch-Site: cross-site` or
//!      `same-site` is refused whatever the `Origin`. Refusal: `403 origin_not_allowed`.
//!   3. A request with neither `Origin` nor `Sec-Fetch-Site` is accepted: it does not come from
//!      a browser (curl, scripts, the end-to-end scenario), and CSRF needs a browser, which
//!      always sends `Origin` on a cross-site `POST` (and `Sec-Fetch-Site` everywhere it is
//!      supported).
//!
//! Allowed origins: `https://TRAEFIK_HOST` and `https://ADMIN_HOST` on the public port (Traefik
//! also routes `ADMIN_HOST/api` to it), `https://ADMIN_HOST` only on the admin port, plus
//! `ALLOWED_DEV_ORIGINS` on both.

use std::sync::Arc;

use axum::body::HttpBody as _;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ApiError;
use super::security::security_event;
use crate::config::{Config, normalize_origin};

/// Origins whose pages may send state-changing requests to a port.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OriginPolicy {
    allowed: Arc<[String]>,
}

impl OriginPolicy {
    pub fn new(origins: impl IntoIterator<Item = String>) -> Self {
        let allowed: Vec<String> = origins
            .into_iter()
            .filter_map(|origin| normalize_origin(&origin))
            .collect();
        Self {
            allowed: allowed.into(),
        }
    }

    fn of_hosts(hosts: &[Option<&String>], config: &Config) -> Self {
        Self::new(
            hosts
                .iter()
                .flatten()
                .map(|host| format!("https://{host}"))
                .chain(config.http.allowed_dev_origins.iter().cloned()),
        )
    }

    /// Public port: the public host and the admin host (whose `/api` is routed to it).
    pub fn public(config: &Config) -> Self {
        Self::of_hosts(
            &[config.public_host.as_ref(), config.admin_host.as_ref()],
            config,
        )
    }

    /// Admin port: the admin host only.
    pub fn admin(config: &Config) -> Self {
        Self::of_hosts(&[config.admin_host.as_ref()], config)
    }

    pub fn allows(&self, origin: &str) -> bool {
        normalize_origin(origin).is_some_and(|origin| self.allowed.contains(&origin))
    }
}

/// Why a request is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Preflight,
    ContentType,
    Origin,
    CrossSite,
}

impl Refusal {
    fn reason(self) -> &'static str {
        match self {
            Self::Preflight => "options_request",
            Self::ContentType => "content_type",
            Self::Origin => "origin_not_allowed",
            Self::CrossSite => "cross_site",
        }
    }

    fn into_error(self) -> ApiError {
        match self {
            Self::Preflight => ApiError::from_status(StatusCode::METHOD_NOT_ALLOWED),
            Self::ContentType => ApiError::from_status(StatusCode::UNSUPPORTED_MEDIA_TYPE),
            Self::Origin | Self::CrossSite => ApiError::new(
                StatusCode::FORBIDDEN,
                "origin_not_allowed",
                "Requête refusée : elle ne provient pas du site iapolitique.",
            ),
        }
    }
}

fn is_json(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
}

/// Decision for a request (`has_body`: the request carries a non-empty body).
pub fn check(
    policy: &OriginPolicy,
    method: &Method,
    headers: &HeaderMap,
    has_body: bool,
) -> Result<(), Refusal> {
    if method == Method::OPTIONS {
        return Err(Refusal::Preflight);
    }
    if method == Method::GET || method == Method::HEAD {
        return Ok(());
    }
    match headers.get(header::CONTENT_TYPE) {
        Some(value) if !value.to_str().is_ok_and(is_json) => return Err(Refusal::ContentType),
        None if has_body => return Err(Refusal::ContentType),
        _ => {}
    }
    let fetch_site = headers
        .get("sec-fetch-site")
        .map(|value| value.to_str().unwrap_or("invalid"));
    if let Some(site) = fetch_site
        && !matches!(site, "same-origin" | "none")
    {
        return Err(Refusal::CrossSite);
    }
    match headers.get(header::ORIGIN) {
        Some(origin) if !origin.to_str().is_ok_and(|o| policy.allows(o)) => Err(Refusal::Origin),
        _ => Ok(()),
    }
}

/// Middleware applying [`check`] (use with `from_fn_with_state(policy, …)`).
pub async fn csrf(State(policy): State<OriginPolicy>, request: Request, next: Next) -> Response {
    let body = request.body().size_hint();
    let has_body = body.upper() != Some(0);
    match check(&policy, request.method(), request.headers(), has_body) {
        Ok(()) => next.run(request).await,
        Err(refusal) => {
            security_event("csrf_refused", refusal.reason());
            refusal.into_error().into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn policy() -> OriginPolicy {
        OriginPolicy::new([
            "https://iapolitique.fr".to_owned(),
            "http://localhost:5173".to_owned(),
        ])
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_static(value));
        }
        map
    }

    const JSON: (&str, &str) = ("content-type", "application/json");

    #[test]
    fn policies_of_the_ports() {
        let config = crate::test_support::config(
            crate::config::Role::Api,
            None,
            None,
            &[
                ("TRAEFIK_HOST", "qa.iapolitique.fr"),
                ("ADMIN_HOST", "qa.admin.iapolitique.fr"),
                ("ALLOWED_DEV_ORIGINS", "http://localhost:5173"),
            ],
        );
        let public = OriginPolicy::public(&config);
        let admin = OriginPolicy::admin(&config);
        assert!(public.allows("https://qa.iapolitique.fr"));
        assert!(public.allows("https://QA.iapolitique.fr/"));
        assert!(public.allows("https://qa.admin.iapolitique.fr"));
        assert!(public.allows("http://localhost:5173"));
        assert!(!public.allows("http://qa.iapolitique.fr"));
        assert!(!public.allows("https://iapolitique.fr"));
        assert!(!public.allows("https://qa.iapolitique.fr.evil.example"));
        assert!(!public.allows("null"));
        assert!(admin.allows("https://qa.admin.iapolitique.fr"));
        assert!(admin.allows("http://localhost:5173"));
        assert!(!admin.allows("https://qa.iapolitique.fr"));

        let bare = crate::test_support::config(crate::config::Role::Api, None, None, &[]);
        assert!(!OriginPolicy::public(&bare).allows("https://iapolitique.fr"));
    }

    #[test]
    fn safe_methods_pass_and_options_is_refused() {
        let evil = headers(&[
            ("origin", "https://evil.example"),
            ("sec-fetch-site", "cross-site"),
        ]);
        for method in [Method::GET, Method::HEAD] {
            assert_eq!(check(&policy(), &method, &evil, false), Ok(()));
        }
        assert_eq!(
            check(&policy(), &Method::OPTIONS, &HeaderMap::new(), false),
            Err(Refusal::Preflight)
        );
    }

    #[test]
    fn content_type_must_be_json() {
        let post = |pairs: &[(&'static str, &'static str)], body| {
            check(&policy(), &Method::POST, &headers(pairs), body)
        };
        assert_eq!(post(&[JSON], true), Ok(()));
        assert_eq!(
            post(&[("content-type", "Application/JSON; charset=utf-8")], true),
            Ok(())
        );
        for form in [
            "application/x-www-form-urlencoded",
            "multipart/form-data; boundary=x",
            "text/plain",
            "application/jsonx",
            "application/vnd.api+json",
        ] {
            assert_eq!(
                post(&[("content-type", form)], true),
                Err(Refusal::ContentType),
                "{form}"
            );
            assert_eq!(
                post(&[("content-type", form)], false),
                Err(Refusal::ContentType),
                "{form}"
            );
        }
        // Without Content-Type: only a bodyless request passes.
        assert_eq!(post(&[], true), Err(Refusal::ContentType));
        assert_eq!(post(&[], false), Ok(()));
    }

    #[test]
    fn origin_and_fetch_metadata_are_checked() {
        let post = |pairs: &[(&'static str, &'static str)]| {
            let mut all = vec![JSON];
            all.extend_from_slice(pairs);
            check(&policy(), &Method::POST, &headers(&all), true)
        };
        assert_eq!(post(&[("origin", "https://iapolitique.fr")]), Ok(()));
        assert_eq!(post(&[("origin", "http://localhost:5173")]), Ok(()));
        assert_eq!(
            post(&[
                ("origin", "https://iapolitique.fr"),
                ("sec-fetch-site", "same-origin")
            ]),
            Ok(())
        );
        assert_eq!(
            post(&[("origin", "https://evil.example")]),
            Err(Refusal::Origin)
        );
        assert_eq!(post(&[("origin", "null")]), Err(Refusal::Origin));
        assert_eq!(
            post(&[
                ("origin", "https://iapolitique.fr"),
                ("sec-fetch-site", "cross-site")
            ]),
            Err(Refusal::CrossSite)
        );
        // Without Origin: Sec-Fetch-Site decides.
        assert_eq!(post(&[("sec-fetch-site", "same-origin")]), Ok(()));
        assert_eq!(post(&[("sec-fetch-site", "none")]), Ok(()));
        assert_eq!(
            post(&[("sec-fetch-site", "cross-site")]),
            Err(Refusal::CrossSite)
        );
        assert_eq!(
            post(&[("sec-fetch-site", "same-site")]),
            Err(Refusal::CrossSite)
        );
        // Neither: not a browser.
        assert_eq!(post(&[]), Ok(()));
        for method in [Method::PUT, Method::DELETE, Method::PATCH] {
            assert_eq!(
                check(
                    &policy(),
                    &method,
                    &headers(&[("origin", "https://evil.example")]),
                    false
                ),
                Err(Refusal::Origin),
                "{method}"
            );
        }
    }
}
