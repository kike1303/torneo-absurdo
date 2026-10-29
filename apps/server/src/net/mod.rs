//! The client of each request (008 RF-A1, RF-A2, RF-A6): its IP, its `IpKey` and whether it
//! is an internal request from `web`. Pure functions plus one layer; no global state.

pub mod client_ip;
pub mod internal;
pub mod ip_key;
pub mod trusted;

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;

pub use client_ip::client_ip;
pub use internal::{InternalSecret, is_internal};
pub use ip_key::IpKey;
pub use trusted::{InvalidTrustedProxies, TrustedProxies};

/// Computed once per request by [`layer`] and inserted as an extension.
#[derive(Clone, Copy)]
pub struct ClientContext {
    pub ip: IpAddr,
    pub key: IpKey,
    pub internal: bool,
}

// Hand-written: the IP never reaches a `Debug` (ADR-0012 A4.1).
impl fmt::Debug for ClientContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientContext")
            .field("internal", &self.internal)
            .finish_non_exhaustive()
    }
}

/// What the layer needs from the configuration.
#[derive(Debug, Clone, Default)]
pub struct NetState {
    pub trusted: TrustedProxies,
    pub secret: Option<InternalSecret>,
}

impl NetState {
    pub fn new(trusted: TrustedProxies, secret: Option<InternalSecret>) -> Self {
        Self { trusted, secret }
    }

    /// The context of a request from `peer` with `headers`.
    pub fn context(&self, peer: IpAddr, headers: &axum::http::HeaderMap) -> ClientContext {
        let internal = is_internal(headers, self.secret.as_ref());
        let ip = client_ip(peer, headers, internal, &self.trusted);
        ClientContext {
            ip,
            key: IpKey::from(ip),
            internal,
        }
    }
}

/// Inserts the [`ClientContext`] of the request. The peer comes from `ConnectInfo`, which
/// `serve_listener` inserts on every request; without it (only a test that bypasses it),
/// everything falls into the `0.0.0.0` bucket: more restrictive, never less.
pub async fn layer(State(net): State<Arc<NetState>>, mut request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());
    debug_assert!(peer.is_some(), "no ConnectInfo on the request");
    let peer = peer.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let context = net.context(peer, request.headers());
    request.extensions_mut().insert(context);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_only_whether_it_is_internal() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let context = ClientContext {
            ip,
            key: IpKey::from(ip),
            internal: true,
        };
        let shown = format!("{context:?}");
        assert!(!shown.contains("203") && !shown.contains("113"), "{shown}");
        assert!(shown.contains("internal: true"), "{shown}");
    }
}
