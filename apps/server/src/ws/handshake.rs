//! Handshake checks, in order (RF-B13, 008 RF-A7): `Origin` → valid upgrade → upgrade rate
//! per `IpKey` → anonymous sockets per `IpKey` → global cap.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use axum::Extension;
use axum::extract::State;
use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::WsErrorCode;

use super::{MAX_MESSAGE_BYTES, WsState, session};
use crate::http::errors::{error_response, too_many_requests};
use crate::net::{ClientContext, IpKey};
use crate::rate::RateLimits;

pub async fn handler(
    State(ws): State<Arc<WsState>>,
    State(rate): State<Arc<RateLimits>>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    // 1. Exactly one `Origin`, equal to `PUBLIC_ORIGIN` (ADR-0008 D6). With several, the
    //    request is ambiguous (a proxy or the browser could read a different one): reject it.
    //    The value is never logged.
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let origin_ok = match (origins.next(), origins.next()) {
        (Some(origin), None) => origin.as_bytes() == ws.public_origin.as_bytes(),
        _ => false,
    };
    if !origin_ok {
        return error_response(StatusCode::FORBIDDEN, WsErrorCode::OriginForbidden);
    }
    // 2. A valid upgrade. axum's status, with an empty body and no `Content-Type`: the
    //    rejection's English text never reaches the client (ADR-0007).
    let upgrade = match upgrade {
        Ok(upgrade) => upgrade,
        Err(rejection) => return rejection.status().into_response(),
    };
    // `net::layer` always runs first; without it, one shared key (more restrictive).
    let key = client.map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.0.key);
    // 3. The upgrade rate per `IpKey` (an invalid upgrade above consumed nothing).
    if let Some(secs) = rate.ws_upgrade_ip.check(&key).retry_after_secs() {
        return too_many_requests(WsErrorCode::RateLimited, secs);
    }
    // 4. Anonymous sockets per `IpKey`.
    let Some(ip_slot) = ws.ip_slots.try_acquire(key) else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::IpCapacity);
    };
    // 5. The global cap, without waiting: a queue would pile requests up in memory.
    let Ok(permit) = ws.permits.clone().try_acquire_owned() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::Capacity);
    };
    // Taken before the upgrade and moved into the callback with the permit, so a socket
    // upgraded during shutdown is still tracked; if the upgrade fails, all of them (the IP
    // slot included) are dropped.
    let tracked = ws.tracker.token();
    let shutdown = ws.shutdown.clone();
    let context = session::Context {
        echo: ws.echo,
        config: ws.config,
    };
    upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            let _ip_slot = ip_slot;
            let _tracked = tracked;
            session::run(socket, context, shutdown).await;
        })
}
