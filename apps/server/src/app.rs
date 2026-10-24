//! Application state and startup (RF-B9): database, migrations, then (and only then) the port.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::FromRef;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

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

/// HTTP connection settings.
#[derive(Debug, Clone, Copy)]
pub struct HttpConfig {
    /// A connection that does not finish sending its request headers in this time is closed
    /// (slow-header protection; `axum::serve` sets no timer, so hyper's default is inert).
    pub header_read_timeout: Duration,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            header_read_timeout: Duration::from_secs(10),
        }
    }
}

/// Settings that are not environment variables (the tests shorten them).
#[derive(Debug, Clone, Copy, Default)]
pub struct Settings {
    pub health: HealthConfig,
    pub db_retry: RetryConfig,
    pub http: HttpConfig,
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
    serve_listener(listener, state, settings.http, shutdown)
        .await
        .map_err(|_| StartupError::Serve)
}

/// Serves the router on an already bound listener until `shutdown` resolves, then stops
/// accepting, lets in-flight requests finish and returns (the in-process tests use it
/// directly).
pub async fn serve_listener(
    listener: TcpListener,
    state: AppState,
    http_config: HttpConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let router = http::router(state);
    let connections = TaskTracker::new();
    let closing = CancellationToken::new();
    let mut shutdown = pin!(shutdown);
    loop {
        let stream = tokio::select! {
            biased;
            () = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                // Usually out of file descriptors: back off instead of spinning.
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
        };
        let service = TowerToHyperService::new(router.clone());
        let closing = closing.clone();
        connections.spawn(async move {
            let mut builder = hyper::server::conn::http1::Builder::new();
            builder
                .timer(TokioTimer::new())
                .header_read_timeout(http_config.header_read_timeout);
            let mut connection = pin!(
                builder
                    .serve_connection(TokioIo::new(stream), service)
                    .with_upgrades()
            );
            tokio::select! {
                _ = connection.as_mut() => {}
                () = closing.cancelled() => {
                    connection.as_mut().graceful_shutdown();
                    let _ = connection.await;
                }
            }
        });
    }
    drop(listener);
    closing.cancel();
    connections.close();
    connections.wait().await;
    Ok(())
}
