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
    AppState::from_config(&config, settings, pool, probe).unwrap()
}

/// Like [`state`], with a WebSocket state below what the config allows.
pub fn state_with_ws(vars: &[(&str, &str)], settings: &Settings, ws: WsState) -> AppState {
    state(vars, settings).with_ws(ws)
}

/// The app on a real, migrated database; returns the pool too.
pub async fn start_db(
    db: &super::db::TestDb,
    vars: &[(&str, &str)],
    settings: &Settings,
) -> (RunningApp, sqlx::PgPool) {
    let url = db.url();
    let mut all = vec![("DATABASE_URL", url.as_str())];
    all.extend_from_slice(vars);
    let config = super::config::config(&all);
    let pool = db.migrated_pool().await;
    let state = AppState::from_config(&config, settings, pool.clone(), Arc::new(AlwaysUp)).unwrap();
    (start(state).await, pool)
}

/// Like [`start_db`], with the WebSocket state of the config changed by `ws`.
pub async fn start_db_ws(
    db: &super::db::TestDb,
    vars: &[(&str, &str)],
    settings: &Settings,
    ws: impl FnOnce(WsState) -> WsState,
) -> (RunningApp, sqlx::PgPool, Arc<WsState>) {
    let url = db.url();
    let mut all = vec![("DATABASE_URL", url.as_str())];
    all.extend_from_slice(vars);
    let config = super::config::config(&all);
    let pool = db.migrated_pool().await;
    let state = AppState::from_config(&config, settings, pool.clone(), Arc::new(AlwaysUp)).unwrap();
    let state = state.with_ws(ws(AppState::ws_from_config(&config, settings)));
    let handle = state.ws.clone();
    (start(state).await, pool, handle)
}

/// A user with a ledger account and a valid session; returns the user id and the token.
pub async fn user_with_session(pool: &sqlx::PgPool) -> (uuid::Uuid, String) {
    let user = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, display_name, display_name_key) VALUES ($1, $2, $3)")
        .bind(user)
        .bind(format!("Test{}", user.simple()))
        .bind(format!("test{}", user.simple()))
        .execute(pool)
        .await
        .unwrap();
    (user, session_for(pool, user).await)
}

/// Another valid session of `user`.
pub async fn session_for(pool: &sqlx::PgPool, user: uuid::Uuid) -> String {
    let mut tx = pool.begin().await.unwrap();
    let issued = server::sessions::issue(
        &mut tx,
        user,
        protocol::Role::User,
        &[0; 32],
        server::sessions::UaFamily::Other,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    issued.token.as_str().to_owned()
}
