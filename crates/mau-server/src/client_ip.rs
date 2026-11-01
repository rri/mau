//! The client's IP address. When the direct peer is one of the configured
//! trusted proxies, the address comes from `X-Forwarded-For` instead: the
//! right-most entry that isn't itself a trusted proxy.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;

use crate::app::AppState;

#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub Option<IpAddr>);

impl ClientIp {
    /// A key for rate limiting.
    pub fn key(&self) -> String {
        self.0.map_or_else(|| "unknown".into(), |ip| ip.to_string())
    }
}

pub fn resolve(
    peer: Option<IpAddr>,
    forwarded: Option<&str>,
    trusted: &[IpAddr],
) -> Option<IpAddr> {
    let peer = peer?;
    if !trusted.contains(&peer) {
        return Some(peer);
    }
    let Some(header) = forwarded else {
        return Some(peer);
    };
    header
        .split(',')
        .rev()
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .find(|ip| !trusted.contains(ip))
        .or(Some(peer))
}

/// Middleware storing the client's address in request extensions.
pub async fn layer(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let forwarded = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let ip = resolve(peer, forwarded, &state.config.trusted_proxies);
    req.extensions_mut().insert(ClientIp(ip));
    next.run(req).await
}

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<ClientIp>()
            .copied()
            .unwrap_or(ClientIp(None)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn untrusted_peer_is_the_client() {
        assert_eq!(
            resolve(Some(ip("203.0.113.5")), Some("1.2.3.4"), &[ip("127.0.0.1")]),
            Some(ip("203.0.113.5"))
        );
    }

    #[test]
    fn trusted_proxy_uses_forwarded_for() {
        let trusted = [ip("127.0.0.1"), ip("10.0.0.2")];
        assert_eq!(
            resolve(Some(ip("127.0.0.1")), Some("198.51.100.7"), &trusted),
            Some(ip("198.51.100.7"))
        );
        // A client can't spoof by prepending entries: the right-most untrusted wins.
        assert_eq!(
            resolve(
                Some(ip("127.0.0.1")),
                Some("6.6.6.6, 198.51.100.7, 10.0.0.2"),
                &trusted
            ),
            Some(ip("198.51.100.7"))
        );
        assert_eq!(
            resolve(Some(ip("127.0.0.1")), None, &trusted),
            Some(ip("127.0.0.1"))
        );
    }
}
