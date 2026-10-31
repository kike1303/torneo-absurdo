//! One socket's life (RF-B13, 008 RF-B14, RF-B15, 010 RF38 to RF43): limits, ping/pong, idleness,
//! echo, subscriptions, the `self` channel, the channels of the fights, the session (revocation and
//! periodic revalidation) and shutdown. Nothing here is logged at `info`; at `debug` only `ws.closed`
//! with the close code.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use protocol::{AuthErrorCode, Channel, ClientMessage, ErrorCode, ServerMessage, WsErrorCode};
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
use crate::fights::hub::{Cursor, FightHub};
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

/// Closes a socket whose session failed the second validation (`auth.unauthenticated`), or
/// whose second validation could not be done (`auth.try_again`).
pub async fn reject(socket: &mut WebSocket, config: WsConfig, code: AuthErrorCode) {
    let reason = code.as_str();
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
    hub: Arc<FightHub>,
) {
    let config = context.config;
    let end = serve(&mut socket, context, &shutdown, auth.as_ref(), &hub).await;
    if let End::Close(code, reason) = end {
        tracing::debug!(event = "ws.closed", close_code = code);
        close(&mut socket, code, reason, config.close_grace).await;
    }
    // Dropping the socket closes the TCP connection.
}

/// The outcome of a periodic revalidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Revalidation {
    /// The session is gone or expired: close with `auth.unauthenticated`.
    Invalid,
    /// Valid, or the database could not say in time: keep the socket.
    Keep,
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
    hub: &Arc<FightHub>,
) -> End {
    let config = context.config;
    let mut rate = RateWindow::default();
    let mut subscriptions = Subscriptions::default();
    // The `fight_sync` this socket can ask for by subscribing (010 amendment DP-B7).
    let (sync_limit, sync_window) = config.subscribe_sync_limit;
    let mut sync_requests = RateWindow::new(sync_limit, sync_window);
    // Where this socket is in each public channel: all its state, at most 8 entries (RF43).
    let mut hub_changes = hub.watch();
    let mut schedule: Option<u64> = None;
    let mut fights: BTreeMap<i32, Cursor> = BTreeMap::new();
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
    let mut check: Option<Pin<Box<dyn Future<Output = Revalidation> + Send>>> = None;
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
            // The periodic revalidation runs beside the socket, never inside an arm: while the
            // query is out, the loop keeps reading frames and answering the shutdown and the
            // revocation (deviation 17 of plan-b, review of #54).
            _ = revalidate.tick(), if auth.is_some() && check.is_none() => {
                if let Some(auth) = auth {
                    let pool = auth.pool.clone();
                    let id = auth.registration.session();
                    let limit = config.revalidate_timeout;
                    check = Some(Box::pin(async move {
                        // A database error, or no answer within the timeout, keeps the socket:
                        // the NOTIFY is still the main path.
                        match tokio::time::timeout(limit, sessions::validate_by_id(&pool, id)).await {
                            Ok(Ok(None)) => Revalidation::Invalid,
                            _ => Revalidation::Keep,
                        }
                    }));
                }
            }
            outcome = async {
                match check.as_mut() {
                    Some(pending) => pending.await,
                    None => std::future::pending().await,
                }
            }, if check.is_some() => {
                check = None;
                // The next check a full interval after this one ended.
                revalidate.reset();
                if outcome == Revalidation::Invalid {
                    return policy(AuthErrorCode::Unauthenticated);
                }
            }
            Some(event) = self_rx.recv() => {
                if !reply(socket, &ServerMessage::SelfEvent { event }, config.close_grace).await {
                    return End::Drop;
                }
            }
            // A slow socket skips ahead: changes join, and each channel says what it needs (RF43).
            _ = hub_changes.changed(), if schedule.is_some() || !fights.is_empty() => {
                if let Some(seen) = schedule.as_mut()
                    && let Some(text) = hub.next_schedule(seen)
                    && !send(socket, Message::Text(text), config.close_grace).await
                {
                    return End::Drop;
                }
                for (id, cursor) in &mut fights {
                    if let Some(text) = hub.next_for(*id, cursor)
                        && !send(socket, Message::Text(text), config.close_grace).await
                    {
                        return End::Drop;
                    }
                }
            }
            () = tokio::time::sleep_until(pong_deadline) => {
                return policy(WsErrorCode::PongTimeout);
            }
            // A viewer sends nothing for minutes (RF42). Pings and their timeout still apply.
            () = tokio::time::sleep_until(idle_deadline), if subscriptions.is_empty() => {
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
                                let mut initial = None;
                                let answer = match message {
                                    ClientMessage::Echo { payload } => {
                                        if !context.echo {
                                            return policy(WsErrorCode::EchoDisabled);
                                        }
                                        ServerMessage::Echo { payload }
                                    }
                                    ClientMessage::Subscribe { channel } => {
                                        let checked = subscriptions.check(
                                            &channel,
                                            auth.is_some(),
                                            |id| i32::try_from(id.get()).is_ok_and(|id| hub.knows(id)),
                                        );
                                        match checked {
                                            Ok((channel, already_open)) => {
                                                // Open: `subscribed` and nothing else, or a message
                                                // of 40 bytes would ask for one of 50 KB (RF38).
                                                if !already_open {
                                                    if matches!(channel, Channel::Fight(_))
                                                        && !sync_requests.allow(now)
                                                    {
                                                        return policy(WsErrorCode::RateLimited);
                                                    }
                                                    subscriptions.open(channel);
                                                    match channel {
                                                        // Resolved from the session, never from
                                                        // the client.
                                                        Channel::SelfChannel => {
                                                            if let (Some(auth), None) = (auth, &self_subscription) {
                                                                self_subscription = Some(
                                                                    auth.hub.subscribe(auth.user_id, self_tx.clone()),
                                                                );
                                                            }
                                                        }
                                                        Channel::Schedule => {
                                                            let (seen, text) = hub.subscribe_schedule();
                                                            schedule = Some(seen);
                                                            initial = text;
                                                        }
                                                        Channel::Fight(id) => {
                                                            let opened = i32::try_from(id.get()).ok().and_then(|id| {
                                                                hub.subscribe_fight(id).map(|sub| (id, sub))
                                                            });
                                                            if let Some((id, (cursor, text))) = opened {
                                                                fights.insert(id, cursor);
                                                                initial = text;
                                                            }
                                                        }
                                                    }
                                                }
                                                ServerMessage::Subscribed { channel }
                                            }
                                            Err(code) => ServerMessage::Error { code: code.into() },
                                        }
                                    }
                                    ClientMessage::Unsubscribe { channel } => {
                                        match subscriptions.unsubscribe(&channel) {
                                            Ok(channel) => {
                                                match channel {
                                                    Channel::SelfChannel => self_subscription = None,
                                                    Channel::Schedule => schedule = None,
                                                    Channel::Fight(id) => {
                                                        if let Ok(id) = i32::try_from(id.get()) {
                                                            fights.remove(&id);
                                                        }
                                                    }
                                                }
                                                ServerMessage::Unsubscribed { channel }
                                            }
                                            Err(code) => ServerMessage::Error { code: code.into() },
                                        }
                                    }
                                };
                                if !reply(socket, &answer, config.close_grace).await {
                                    return End::Drop;
                                }
                                if let Some(text) = initial
                                    && !send(socket, Message::Text(text), config.close_grace).await
                                {
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
