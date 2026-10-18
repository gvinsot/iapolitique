//! Client IP address, for rate limiting only. It is **never** logged nor stored (§7.1, §7.2):
//! its `Debug` output is redacted and it has no `Display`.
//!
//! # Trusted hop logic
//!
//! Every reverse proxy appends the address of the peer it received the connection from to
//! `X-Forwarded-For`. Entries on the left are written by the client or by proxies we do not
//! control, so they can be forged; only the entries appended by **our** proxies are reliable.
//! With `TRUSTED_PROXY_HOPS = n` proxies in front of the api (Traefik: `n = 1`), the client is
//! the `n`-th entry **from the right**:
//!
//! ```text
//! X-Forwarded-For: <forged?>, <forged?>, <client seen by Traefik>
//!                                         └── n = 1: rightmost entry
//! ```
//!
//! With `n = 0`, or when the header has fewer than `n` usable entries, the TCP peer address is
//! used instead.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRef, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;

use super::error::ApiError;

/// Number of trusted proxies, extracted from the application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedProxyHops(pub usize);

/// The client address, resolved as described in the module documentation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(IpAddr);

impl ClientIp {
    /// The address. Use it only to compute rate limit fingerprints; never log or store it.
    pub fn ip(self) -> IpAddr {
        self.0
    }
}

impl std::fmt::Debug for ClientIp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientIp([REDACTED])")
    }
}

/// Parses one `X-Forwarded-For` entry: `1.2.3.4`, `1.2.3.4:5678`, `2001:db8::1` or
/// `[2001:db8::1]:443`.
fn parse_entry(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim();
    entry
        .parse::<IpAddr>()
        .ok()
        .or_else(|| entry.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
        .map(|ip| match ip {
            // An IPv4 client seen through an IPv6 socket.
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 => v4,
        })
}

/// Resolves the client address from the `X-Forwarded-For` values (in header order), the number
/// of trusted hops and the TCP peer.
pub fn resolve(forwarded_for: &[&str], hops: usize, peer: Option<IpAddr>) -> Option<IpAddr> {
    if hops == 0 {
        return peer;
    }
    let entries: Vec<&str> = forwarded_for
        .iter()
        .flat_map(|value| value.split(','))
        .collect();
    if entries.len() < hops {
        return peer;
    }
    parse_entry(entries[entries.len() - hops]).or(peer)
}

fn forwarded_for(headers: &HeaderMap) -> Vec<&str> {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect()
}

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
    TrustedProxyHops: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TrustedProxyHops(hops) = TrustedProxyHops::from_ref(state);
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());
        resolve(&forwarded_for(&parts.headers), hops, peer)
            .map(ClientIp)
            .ok_or_else(|| {
                ApiError::bad_request(
                    "client_address_unavailable",
                    "Impossible de déterminer l'origine de la requête.",
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn rightmost_entry_with_one_trusted_hop() {
        let peer = Some(ip("10.0.0.2"));
        assert_eq!(resolve(&["203.0.113.7"], 1, peer), Some(ip("203.0.113.7")));
        // The client forged the first entries: they are ignored.
        assert_eq!(
            resolve(&["1.1.1.1, 2.2.2.2, 203.0.113.7"], 1, peer),
            Some(ip("203.0.113.7"))
        );
    }

    #[test]
    fn several_headers_are_concatenated_in_order() {
        assert_eq!(
            resolve(&["1.1.1.1", "198.51.100.1, 203.0.113.7"], 2, None),
            Some(ip("198.51.100.1"))
        );
    }

    #[test]
    fn falls_back_to_the_peer() {
        let peer = Some(ip("192.0.2.10"));
        assert_eq!(resolve(&[], 1, peer), peer);
        assert_eq!(resolve(&["203.0.113.7"], 2, peer), peer);
        assert_eq!(resolve(&["203.0.113.7"], 0, peer), peer);
        assert_eq!(resolve(&["not-an-ip"], 1, peer), peer);
        assert_eq!(resolve(&[], 1, None), None);
    }

    #[test]
    fn parses_ports_ipv6_and_mapped_addresses() {
        assert_eq!(
            resolve(&["203.0.113.7:4711"], 1, None),
            Some(ip("203.0.113.7"))
        );
        assert_eq!(
            resolve(&[" 2001:db8::1 "], 1, None),
            Some(ip("2001:db8::1"))
        );
        assert_eq!(
            resolve(&["[2001:db8::1]:443"], 1, None),
            Some(ip("2001:db8::1"))
        );
        assert_eq!(
            resolve(&["::ffff:192.0.2.1"], 1, None),
            Some(ip("192.0.2.1"))
        );
    }

    #[test]
    fn debug_output_is_redacted() {
        assert_eq!(
            format!("{:?}", ClientIp(ip("203.0.113.7"))),
            "ClientIp([REDACTED])"
        );
    }

    #[tokio::test]
    async fn extractor_uses_headers_and_connect_info() {
        let request = Request::get("/")
            .header("x-forwarded-for", "9.9.9.9, 203.0.113.7")
            .extension(ConnectInfo(SocketAddr::from(([10, 0, 0, 2], 1234))))
            .body(())
            .unwrap();
        let (mut parts, ()) = request.into_parts();
        let client = ClientIp::from_request_parts(&mut parts, &TrustedProxyHops(1))
            .await
            .unwrap();
        assert_eq!(client.ip(), ip("203.0.113.7"));

        let client = ClientIp::from_request_parts(&mut parts, &TrustedProxyHops(0))
            .await
            .unwrap();
        assert_eq!(client.ip(), ip("10.0.0.2"));

        let (mut bare, ()) = Request::get("/").body(()).unwrap().into_parts();
        let err = ClientIp::from_request_parts(&mut bare, &TrustedProxyHops(1))
            .await
            .unwrap_err();
        assert_eq!(err.code, "client_address_unavailable");
    }
}
