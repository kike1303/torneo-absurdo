//! The server in process, on `127.0.0.1:0`.

use std::net::SocketAddr;
use std::sync::Arc;

use server::app::{AppState, HttpConfig, Settings, serve_listener};
use server::db::{DbProbe, PingFuture};
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
pub const TRUST_LOOPBACK: (&str, &str) = ("TRUSTED_PROXIES", "127.0.0.1/32");

/// `INTERNAL_API_SECRET` set to [`SECRET`] (the default since phase B; kept explicit where a test
/// is about the secret).
pub const WITH_SECRET: (&str, &str) = ("INTERNAL_API_SECRET", SECRET);

/// Another valid secret: a deployment whose secret is not [`SECRET`].
pub const OTHER_SECRET: (&str, &str) = (
    "INTERNAL_API_SECRET",
    "b3RoZXItc2VjcmV0LW90aGVyLXNlY3JldC1vdGhlcis",
);

/// A valid `IP_HASH_KEY` for the tests (32 bytes, base64url). Not a real key.
pub const IP_HASH_KEY: &str = "dGVzdC1vbmx5LWlwLWhhc2gta2V5LXRlc3Qtb25seS1rZXk";

/// The state from the config of `vars`, a lazy pool and a database that is always up.
pub fn state(vars: &[(&str, &str)], settings: &Settings) -> AppState {
    state_with_probe(vars, settings, Arc::new(AlwaysUp))
}

/// Like [`state`], with another health probe.
pub fn state_with_probe(
    vars: &[(&str, &str)],
    settings: &Settings,
    probe: Arc<dyn DbProbe>,
) -> AppState {
    let config = super::config::config(vars);
    let pool = super::config::lazy_pool(&config);
    AppState::from_config(&config, settings, pool, probe)
}

/// Like [`state`], with a WebSocket state below what the config allows.
pub fn state_with_ws(vars: &[(&str, &str)], settings: &Settings, ws: WsState) -> AppState {
    state(vars, settings).with_ws(ws)
}
