//! Application state and startup (RF-B9): database, migrations, then (and only then) the port.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::FromRef;
use tokio::net::TcpListener;

use crate::config::Config;
use crate::db::{self, DbProbe, PgProbe, RetryConfig};
use crate::http::{self, health::HealthConfig, health::HealthState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StartupError {
    #[error("the database was unavailable until the deadline")]
    DbUnavailable,
    #[error("the migrations failed")]
    Migrate,
    #[error("the listener could not bind")]
    Bind,
    #[error("the server stopped with an I/O error")]
    Serve,
}

/// Settings that are not environment variables (the tests shorten them).
#[derive(Debug, Clone, Copy, Default)]
pub struct Settings {
    pub health: HealthConfig,
    pub db_retry: RetryConfig,
}

#[derive(Clone)]
pub struct AppState {
    pub health: Arc<HealthState>,
}

impl AppState {
    pub fn new(probe: Arc<dyn DbProbe>, settings: &Settings) -> Self {
        Self {
            health: Arc::new(HealthState::new(probe, settings.health)),
        }
    }
}

impl FromRef<AppState> for Arc<HealthState> {
    fn from_ref(state: &AppState) -> Self {
        state.health.clone()
    }
}

/// The kind of a bind error, as a code (never the address or the port).
pub fn bind_error_kind(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::AddrInUse => "addr_in_use",
        io::ErrorKind::AddrNotAvailable => "addr_not_available",
        io::ErrorKind::PermissionDenied => "permission_denied",
        _ => "other",
    }
}

/// Connects to the database, migrates, binds and serves until `shutdown` resolves.
pub async fn serve(
    config: &Config,
    settings: Settings,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), StartupError> {
    let pool = db::connect_with_retry(config.database.clone(), settings.db_retry).await?;
    db::migrate(&pool).await?;
    let listener = TcpListener::bind(SocketAddr::new(config.bind_addr, config.port))
        .await
        .map_err(|error| {
            tracing::error!(
                event = "server.bind_failed",
                error_kind = bind_error_kind(&error)
            );
            StartupError::Bind
        })?;
    let port = listener
        .local_addr()
        .map_err(|_| StartupError::Bind)?
        .port();
    tracing::info!(event = "server.listening", port);
    let state = AppState::new(Arc::new(PgProbe::new(pool)), &settings);
    serve_listener(listener, state, shutdown)
        .await
        .map_err(|_| StartupError::Serve)
}

/// Serves the router on an already bound listener (the in-process tests use it directly).
pub async fn serve_listener(
    listener: TcpListener,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(shutdown)
        .await
}
