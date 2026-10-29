//! One socket's life (RF-B13, 008 RF-B14, RF-B15): limits, ping/pong, idleness, echo,
//! subscriptions, the `self` channel, the session (revocation and periodic revalidation) and
//! shutdown. Nothing here is logged at `info`; at `debug` only `ws.closed` with the close code.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use protocol::{AuthErrorCode, ClientMessage, ErrorCode, ServerMessage, WsErrorCode};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::WsConfig;
use super::channels::Subscriptions;
use super::rate::RateWindow;
use super::registry::Registration;
use super::self_hub::{QUEUE, SelfHub, SelfSubscription};
use crate::sessions;

/// The session of a socket that passed its second validation.
pub struct SocketAuth {
    pub registration: Registration,
    pub user_id: Uuid,
    pub pool: PgPool,
    pub hub: Arc<SelfHub>,
}

pub const CLOSE_UNSUPPORTED_DATA: u16 = 1003;
pub const CLOSE_POLICY_VIOLATION: u16 = 1008;
pub const CLOSE_MESSAGE_TOO_BIG: u16 = 1009;
pub const CLOSE_SERVICE_RESTART: u16 = 1012;

#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub echo: bool,
    pub config: WsConfig,
}

/// How the session ends.
enum End {
    /// Close with this code and reason, then wait for the client's close.
    Close(u16, &'static str),
    /// The connection is gone or unusable: just drop it.
    Drop,
}

fn policy(code: impl Into<ErrorCode>) -> End {
    End::Close(CLOSE_POLICY_VIOLATION, code.into().as_str())
}

/// Closes a socket whose session failed the second validation.
pub async fn reject(socket: &mut WebSocket, config: WsConfig) {
    let reason = AuthErrorCode::Unauthenticated.as_str();
    tracing::debug!(event = "ws.closed", close_code = CLOSE_POLICY_VIOLATION);
    close(socket, CLOSE_POLICY_VIOLATION, reason, config.close_grace).await;
}

fn is_capacity_error(error: axum::Error) -> bool {
    error
        .into_inner()
        .downcast_ref::<tungstenite::Error>()
        .is_some_and(|e| matches!(e, tungstenite::Error::Capacity(_)))
}

pub async fn run(
    mut socket: WebSocket,
    context: Context,
    shutdown: CancellationToken,
    auth: Option<SocketAuth>,
) {
    let config = context.config;
    let end = serve(&mut socket, context, &shutdown, auth.as_ref()).await;
    if let End::Close(code, reason) = end {
        tracing::debug!(event = "ws.closed", close_code = code);
        close(&mut socket, code, reason, config.close_grace).await;
    }
    // Dropping the socket closes the TCP connection.
}

/// Sends a server message; `false` if the socket is gone.
async fn reply(socket: &mut WebSocket, message: &ServerMessage, grace: Duration) -> bool {
    match serde_json::to_string(message) {
        Ok(text) => send(socket, Message::Text(text.into()), grace).await,
        Err(_) => false,
    }
}

async fn serve(
    socket: &mut WebSocket,
    context: Context,
    shutdown: &CancellationToken,
    auth: Option<&SocketAuth>,
) -> End {
    let config = context.config;
    let mut rate = RateWindow::default();
    let mut subscriptions = Subscriptions::default();
    // One queue for the life of the socket; the hub holds its sender only while subscribed.
    let (self_tx, mut self_rx) = mpsc::channel(QUEUE);
    let mut self_subscription: Option<SelfSubscription> = None;
    // Cancelled when the session is revoked; never for an anonymous socket.
    let closed = auth.map_or_else(CancellationToken::new, |a| a.registration.closed());
    let mut revalidate = tokio::time::interval_at(
        Instant::now() + config.revalidate_interval,
        config.revalidate_interval,
    );
    revalidate.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_frame = Instant::now();
    let mut last_data = Instant::now();
    let mut pings =
        tokio::time::interval_at(Instant::now() + config.ping_interval, config.ping_interval);
    pings.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        let pong_deadline = last_frame + config.pong_timeout;
        let idle_deadline = last_data + config.idle_timeout;
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return End::Close(CLOSE_SERVICE_RESTART, ""),
            () = closed.cancelled() => return policy(AuthErrorCode::Unauthenticated),
            _ = revalidate.tick(), if auth.is_some() => {
                if let Some(auth) = auth {
                    // A database error keeps the socket: the NOTIFY is still the main path.
                    if let Ok(None) =
                        sessions::validate_by_id(&auth.pool, auth.registration.session()).await
                    {
                        return policy(AuthErrorCode::Unauthenticated);
                    }
                }
            }
            Some(event) = self_rx.recv() => {
                if !reply(socket, &ServerMessage::SelfEvent { event }, config.close_grace).await {
                    return End::Drop;
                }
            }
            () = tokio::time::sleep_until(pong_deadline) => {
                return policy(WsErrorCode::PongTimeout);
            }
            () = tokio::time::sleep_until(idle_deadline) => {
                return policy(WsErrorCode::IdleTimeout);
            }
            _ = pings.tick() => {
                if !send(socket, Message::Ping(Default::default()), config.close_grace).await {
                    return End::Drop;
                }
            }
            received = socket.recv() => {
                let message = match received {
                    None => return End::Drop,
                    Some(Err(error)) => {
                        return if is_capacity_error(error) {
                            End::Close(CLOSE_MESSAGE_TOO_BIG, "")
                        } else {
                            End::Drop
                        };
                    }
                    Some(Ok(message)) => message,
                };
                let now = Instant::now();
                last_frame = now;
                match message {
                    Message::Pong(_) => {}
                    // tungstenite answers the close itself.
                    Message::Close(_) => return End::Drop,
                    Message::Ping(_) | Message::Text(_) | Message::Binary(_) => {
                        // Client pings count too, so a client cannot make us answer
                        // pings without limit.
                        if !rate.allow(now) {
                            return policy(WsErrorCode::RateLimited);
                        }
                        match message {
                            Message::Binary(_) => return End::Close(CLOSE_UNSUPPORTED_DATA, ""),
                            Message::Text(text) => {
                                let Ok(message) =
                                    serde_json::from_str::<ClientMessage>(text.as_str())
                                else {
                                    return policy(WsErrorCode::InvalidMessage);
                                };
                                let answer = match message {
                                    ClientMessage::Echo { payload } => {
                                        if !context.echo {
                                            return policy(WsErrorCode::EchoDisabled);
                                        }
                                        ServerMessage::Echo { payload }
                                    }
                                    ClientMessage::Subscribe { channel } => {
                                        match subscriptions.subscribe(&channel, auth.is_some()) {
                                            Ok(channel) => {
                                                // `self` is the only channel: resolved from
                                                // the session, never from the client.
                                                if let (Some(auth), None) = (auth, &self_subscription) {
                                                    self_subscription = Some(
                                                        auth.hub.subscribe(auth.user_id, self_tx.clone()),
                                                    );
                                                }
                                                ServerMessage::Subscribed { channel }
                                            }
                                            Err(code) => ServerMessage::Error { code: code.into() },
                                        }
                                    }
                                    ClientMessage::Unsubscribe { channel } => {
                                        match subscriptions.unsubscribe(&channel) {
                                            Ok(channel) => {
                                                self_subscription = None;
                                                ServerMessage::Unsubscribed { channel }
                                            }
                                            Err(code) => ServerMessage::Error { code: code.into() },
                                        }
                                    }
                                };
                                if !reply(socket, &answer, config.close_grace).await {
                                    return End::Drop;
                                }
                                last_data = now;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

async fn send(socket: &mut WebSocket, message: Message, grace: Duration) -> bool {
    matches!(
        tokio::time::timeout(grace, socket.send(message)).await,
        Ok(Ok(()))
    )
}

/// Sends the close frame and waits (up to `grace` in total) for the client's close.
async fn close(socket: &mut WebSocket, code: u16, reason: &'static str, grace: Duration) {
    let deadline = Instant::now() + grace;
    let frame = Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }));
    if !matches!(
        tokio::time::timeout_at(deadline, socket.send(frame)).await,
        Ok(Ok(()))
    ) {
        return;
    }
    let _ = tokio::time::timeout_at(deadline, async {
        while let Some(Ok(message)) = socket.recv().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    })
    .await;
}
