//! The server in process, on `127.0.0.1:0`.

use std::net::SocketAddr;
use std::sync::Arc;

use server::app::{AppState, HttpConfig, Settings, serve_listener};
use server::db::{DbProbe, PingFuture};
use server::net::{InternalSecret, NetState, TrustedProxies};
use server::ws::{WsConfig, WsState};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub struct RunningApp {
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

pub async fn start(state: AppState) -> RunningApp {
    start_with(state, HttpConfig::default()).await
}

pub async fn start_with(state: AppState, http: HttpConfig) -> RunningApp {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        serve_listener(listener, state, http, async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    RunningApp {
        addr,
        stop: Some(stop),
        task,
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
    }
}

/// Origin the in-process tests use.
pub const ORIGIN: &str = "http://localhost:5173";

/// WebSocket state with the echo on, a cap of 2000 and the default lifetimes.
pub fn ws_state() -> WsState {
    WsState::new(ORIGIN.to_owned(), true, 2000, WsConfig::default())
}

/// A database that is always up.
pub struct AlwaysUp;

impl DbProbe for AlwaysUp {
    fn ping(&self) -> PingFuture<'_> {
        Box::pin(async { true })
    }
}

/// A valid `INTERNAL_API_SECRET` for the tests (32 bytes, base64url). Not a real secret.
pub const SECRET: &str = "dGVzdC1vbmx5LXNlY3JldC10ZXN0LW9ubHktc2VjcmV0";

/// `TRUSTED_PROXIES` with the loopback, so `X-Forwarded-For` simulates other clients.
pub fn trusting_loopback() -> TrustedProxies {
    TrustedProxies::parse("127.0.0.1/32", true).unwrap()
}

/// The client context settings: trusted proxies and, optionally, the test secret.
pub fn net(trusted: TrustedProxies, secret: bool) -> NetState {
    NetState::new(trusted, secret.then(|| InternalSecret::new(SECRET)))
}

/// The state with `net`, `settings` and `ws`.
pub fn state_with_ws(net: NetState, settings: &Settings, ws: WsState) -> AppState {
    AppState::new(Arc::new(AlwaysUp), settings, ws).with_net(net)
}

/// The state with `net` and `settings`, and the default WebSocket state.
pub fn state_with(net: NetState, settings: &Settings) -> AppState {
    state_with_ws(net, settings, ws_state())
}
