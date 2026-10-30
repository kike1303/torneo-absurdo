//! Handshake checks, in order (RF-B13, 008 RF-A7, RF-B14): `Origin` → valid upgrade → session
//! (first validation) → upgrade rate (per user and per `IpKey` with a session, per `IpKey`
//! without) → sockets (5 per user and 200 per `IpKey` with a session, 50 per `IpKey` without)
//! → global cap. A cookie that does not validate is an anonymous socket: anonymous quotas, no
//! registration and no `self` (008 plan-b question 3).
//!
//! With a session, once upgraded: register the socket → the validation gate → second validation
//! by session id. A revoke at any point is seen either by the NOTIFY (the socket is already
//! registered) or by the second validation.

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::extract::State;
use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{AuthErrorCode, WsErrorCode};

use super::ip_slots::Slot;
use super::{MAX_MESSAGE_BYTES, session};
use crate::app::AppState;
use crate::http::errors::{error_response, too_many_requests};
use crate::net::{ClientContext, IpKey};
use crate::sessions::{self, SessionInfo};

/// The slots a socket holds; all released when it ends (RAII).
enum Admission {
    Anonymous {
        _slot: Slot<IpKey>,
    },
    Session {
        info: SessionInfo,
        _user: Slot<uuid::Uuid>,
        _ip: Slot<IpKey>,
    },
}

pub async fn handler(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let ws = state.ws.clone();
    let rate = state.rate.clone();
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
    // 3. The session, read from the cookie only (never a token in the query, D6).
    // A database error is no session: the socket is anonymous (no `self`, anonymous quotas),
    // which grants nothing.
    let session = crate::http::session::current(&state, &headers)
        .await
        .unwrap_or(None);
    // 4. The upgrade rate and 5. the sockets, by kind (an invalid upgrade consumed nothing).
    let admission = match session {
        Some(info) => {
            let limited = rate
                .ws_session_user
                .check(&info.user_id)
                .retry_after_secs()
                .or_else(|| rate.ws_session_ip.check(&key).retry_after_secs());
            if let Some(secs) = limited {
                return too_many_requests(WsErrorCode::RateLimited, secs);
            }
            let Some(user) = ws.user_slots.try_acquire(info.user_id) else {
                return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::UserCapacity);
            };
            let Some(ip) = ws.session_ip_slots.try_acquire(key) else {
                return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::IpCapacity);
            };
            Admission::Session {
                info,
                _user: user,
                _ip: ip,
            }
        }
        None => {
            if let Some(secs) = rate.ws_upgrade_ip.check(&key).retry_after_secs() {
                return too_many_requests(WsErrorCode::RateLimited, secs);
            }
            let Some(slot) = ws.ip_slots.try_acquire(key) else {
                return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::IpCapacity);
            };
            Admission::Anonymous { _slot: slot }
        }
    };
    // 6. The global cap, without waiting: a queue would pile requests up in memory.
    let Ok(permit) = ws.permits.clone().try_acquire_owned() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, WsErrorCode::Capacity);
    };
    // Taken before the upgrade and moved into the callback with the permit, so a socket
    // upgraded during shutdown is still tracked; if the upgrade fails, all of them (the slots
    // included) are dropped.
    let tracked = ws.tracker.token();
    let shutdown = ws.shutdown.clone();
    let config = ws.config;
    let echo = ws.echo;
    let pool = state.db.clone();
    upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |mut socket| async move {
            let _permit = permit;
            let _tracked = tracked;
            let auth = match &admission {
                Admission::Anonymous { .. } => None,
                Admission::Session { info, .. } => {
                    ws.gate.before_register().await;
                    let registration = ws.registry.register(info.id);
                    ws.gate.before_validate().await;
                    // Bounded like the periodic check: the client already has its 101, and a
                    // hung query here would keep the socket out of its loop (no frames, no
                    // shutdown, no revocation). Fails closed: an error or a timeout closes the
                    // socket with `auth.try_again`, so a client can reconnect (round 3 of #54).
                    let second = tokio::time::timeout(
                        config.handshake_validate_timeout,
                        sessions::validate_by_id(&pool, info.id),
                    )
                    .await;
                    match second {
                        Ok(Ok(Some(_))) => Some(session::SocketAuth {
                            registration,
                            user_id: info.user_id,
                            pool,
                            hub: ws.self_hub.clone(),
                        }),
                        Ok(Ok(None)) => {
                            drop(registration);
                            session::reject(&mut socket, config, AuthErrorCode::Unauthenticated)
                                .await;
                            return;
                        }
                        Ok(Err(_)) | Err(_) => {
                            drop(registration);
                            session::reject(&mut socket, config, AuthErrorCode::TryAgain).await;
                            return;
                        }
                    }
                }
            };
            let context = session::Context { echo, config };
            session::run(socket, context, shutdown, auth).await;
            drop(admission);
        })
}
