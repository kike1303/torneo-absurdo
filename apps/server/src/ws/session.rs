//! One socket's life (RF-B13): limits, ping/pong, idleness, echo and shutdown.
//! Nothing here is logged at `info`; at `debug` only `ws.closed` with the close code.

use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use protocol::{ClientMessage, ServerMessage, WsErrorCode};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::WsConfig;
use super::rate::RateWindow;

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

fn policy(code: WsErrorCode) -> End {
    End::Close(CLOSE_POLICY_VIOLATION, code.as_str())
}

fn is_capacity_error(error: axum::Error) -> bool {
    error
        .into_inner()
        .downcast_ref::<tungstenite::Error>()
        .is_some_and(|e| matches!(e, tungstenite::Error::Capacity(_)))
}

pub async fn run(mut socket: WebSocket, context: Context, shutdown: CancellationToken) {
    let config = context.config;
    let end = serve(&mut socket, context, &shutdown).await;
    if let End::Close(code, reason) = end {
        tracing::debug!(event = "ws.closed", close_code = code);
        close(&mut socket, code, reason, config.close_grace).await;
    }
    // Dropping the socket closes the TCP connection.
}

async fn serve(socket: &mut WebSocket, context: Context, shutdown: &CancellationToken) -> End {
    let config = context.config;
    let mut rate = RateWindow::default();
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
                                let Ok(ClientMessage::Echo { payload }) =
                                    serde_json::from_str::<ClientMessage>(text.as_str())
                                else {
                                    return policy(WsErrorCode::InvalidMessage);
                                };
                                if !context.echo {
                                    return policy(WsErrorCode::EchoDisabled);
                                }
                                let Ok(reply) =
                                    serde_json::to_string(&ServerMessage::Echo { payload })
                                else {
                                    return End::Drop;
                                };
                                if !send(socket, Message::Text(reply.into()), config.close_grace).await {
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
