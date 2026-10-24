//! Handshake checks, in order (RF-B13): `Origin` → valid upgrade → global cap.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::WsErrorCode;

use super::{MAX_MESSAGE_BYTES, WsState, session};
use crate::http::errors::error_response;

pub async fn handler(
    State(ws): State<Arc<WsState>>,
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
    // 3. The global cap, without waiting: a queue would pile requests up in memory.
    let Ok(permit) = ws.permits.clone().try_acquire_owned() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::Capacity);
    };
    // Taken before the upgrade and moved into the callback with the permit, so a socket
    // upgraded during shutdown is still tracked; if the upgrade fails, all three are dropped.
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
            let _tracked = tracked;
            session::run(socket, context, shutdown).await;
        })
}
