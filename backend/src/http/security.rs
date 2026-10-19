//! Security headers of every backend response (AGENTS.md §14.2).
//!
//! One layer, outermost on each server (public port, admin port, worker health port), so that
//! every response gets the same headers: handler answers, JSON errors of the middlewares (404,
//! 405, 413, 415, 504), panics, the ALTCHA challenge and the export downloads alike.
//!
//! - fixed headers: API `Content-Security-Policy`, HSTS, `nosniff`, `X-Frame-Options`,
//!   `Referrer-Policy`, `Permissions-Policy`, COOP / CORP, `X-Robots-Tag`;
//! - `Cache-Control: no-store` on the admin and worker ports, on `/api/track*` and on every
//!   non-`GET`/`HEAD` request (submissions); elsewhere the handler's value (short public
//!   `max-age`, `http/cache.rs`) is kept and a response without one gets `no-store`;
//! - removed: `Server`, `X-Powered-By` and any `Access-Control-*` header (no CORS, §14.3).

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::Response;

use super::cache;

/// Server a response comes from: decides the `Cache-Control` policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Public API (port 8080).
    Public,
    /// Admin API (port 8081): never stored.
    Admin,
    /// Internal health endpoint of the worker (port 8082): never stored.
    Worker,
}

pub const CONTENT_SECURITY_POLICY: &str =
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'";
pub const STRICT_TRANSPORT_SECURITY: &str = "max-age=63072000; includeSubDomains";
pub const PERMISSIONS_POLICY: &str = "accelerometer=(), autoplay=(), browsing-topics=(), \
     camera=(), display-capture=(), encrypted-media=(), fullscreen=(), geolocation=(), \
     gyroscope=(), hid=(), interest-cohort=(), magnetometer=(), microphone=(), midi=(), \
     payment=(), picture-in-picture=(), publickey-credentials-get=(), screen-wake-lock=(), \
     serial=(), usb=(), xr-spatial-tracking=()";
pub const X_ROBOTS_TAG: &str = "noindex, nofollow";

const PERMISSIONS_POLICY_HEADER: HeaderName = HeaderName::from_static("permissions-policy");
const COOP_HEADER: HeaderName = HeaderName::from_static("cross-origin-opener-policy");
const CORP_HEADER: HeaderName = HeaderName::from_static("cross-origin-resource-policy");
const X_ROBOTS_TAG_HEADER: HeaderName = HeaderName::from_static("x-robots-tag");
const X_POWERED_BY_HEADER: HeaderName = HeaderName::from_static("x-powered-by");

/// Headers set (overriding any handler value) on every response.
fn fixed_headers() -> [(HeaderName, HeaderValue); 9] {
    [
        (
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ),
        (
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static(STRICT_TRANSPORT_SECURITY),
        ),
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
        (
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ),
        (
            PERMISSIONS_POLICY_HEADER,
            HeaderValue::from_static(PERMISSIONS_POLICY),
        ),
        (COOP_HEADER, HeaderValue::from_static("same-origin")),
        (CORP_HEADER, HeaderValue::from_static("same-origin")),
        (X_ROBOTS_TAG_HEADER, HeaderValue::from_static(X_ROBOTS_TAG)),
    ]
}

/// `/api/track` and everything below it: answers about a tracked text, never stored.
pub fn is_tracking_path(path: &str) -> bool {
    path == "/api/track" || path.starts_with("/api/track/")
}

fn is_safe_method(method: &Method) -> bool {
    method == Method::GET || method == Method::HEAD
}

/// Whether the response to this request must be `no-store` whatever the handler said.
fn forces_no_store(surface: Surface, method: &Method, path: &str) -> bool {
    match surface {
        Surface::Admin | Surface::Worker => true,
        Surface::Public => !is_safe_method(method) || is_tracking_path(path),
    }
}

/// Applies the policy of this module to the headers of a response.
pub fn apply(headers: &mut HeaderMap, no_store: bool) {
    for (name, value) in fixed_headers() {
        headers.insert(name, value);
    }
    if no_store || !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, cache::NO_STORE);
    }
    headers.remove(header::SERVER);
    headers.remove(X_POWERED_BY_HEADER);
    let cors: Vec<HeaderName> = headers
        .keys()
        .filter(|name| name.as_str().starts_with("access-control-"))
        .cloned()
        .collect();
    for name in cors {
        headers.remove(name);
    }
}

/// Middleware setting the security headers (use with `from_fn_with_state(surface, …)`).
pub async fn security_headers(
    State(surface): State<Surface>,
    request: Request,
    next: Next,
) -> Response {
    let no_store = forces_no_store(surface, request.method(), request.uri().path());
    let mut response = next.run(request).await;
    apply(response.headers_mut(), no_store);
    response
}

/// Asserts the headers every backend response carries (tests of each response family).
#[cfg(test)]
pub fn assert_security_headers(headers: &HeaderMap, context: &str) {
    for (name, value) in fixed_headers() {
        assert_eq!(headers.get(&name), Some(&value), "{context}: {name}");
    }
    assert!(
        headers.contains_key(header::CACHE_CONTROL),
        "{context}: Cache-Control"
    );
    assert!(!headers.contains_key(header::SERVER), "{context}: Server");
    assert!(
        !headers.contains_key(X_POWERED_BY_HEADER),
        "{context}: X-Powered-By"
    );
    for name in headers.keys() {
        assert!(
            !name.as_str().starts_with("access-control-"),
            "{context}: {name}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_store_policy() {
        for surface in [Surface::Admin, Surface::Worker] {
            assert!(forces_no_store(surface, &Method::GET, "/api/health"));
        }
        assert!(!forces_no_store(
            Surface::Public,
            &Method::GET,
            "/api/ideas"
        ));
        assert!(!forces_no_store(
            Surface::Public,
            &Method::HEAD,
            "/api/stats"
        ));
        assert!(forces_no_store(
            Surface::Public,
            &Method::POST,
            "/api/ideas"
        ));
        assert!(forces_no_store(
            Surface::Public,
            &Method::POST,
            "/api/track"
        ));
        assert!(forces_no_store(Surface::Public, &Method::GET, "/api/track"));
        assert!(forces_no_store(
            Surface::Public,
            &Method::GET,
            "/api/track/delete"
        ));
        assert!(!forces_no_store(
            Surface::Public,
            &Method::GET,
            "/api/tracker"
        ));
    }

    #[test]
    fn apply_sets_removes_and_keeps() {
        let mut headers = HeaderMap::new();
        headers.insert(header::SERVER, HeaderValue::from_static("hyper"));
        headers.insert(X_POWERED_BY_HEADER, HeaderValue::from_static("axum"));
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("POST"),
        );
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("ALLOW"));
        headers.insert(header::CACHE_CONTROL, cache::SHORT);
        apply(&mut headers, false);
        assert_security_headers(&headers, "apply");
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=60");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");

        apply(&mut headers, true);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");

        let mut headers = HeaderMap::new();
        apply(&mut headers, false);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    }
}
