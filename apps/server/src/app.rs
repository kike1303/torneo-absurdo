//! Application state and startup (RF-B9): database, migrations, then (and only then) the port.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, FromRef};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::db::{self, DbProbe, PgProbe, RetryConfig};
use crate::http::origin::OriginPolicy;
use crate::http::{self, health::HealthConfig, health::HealthState};
use crate::net::NetState;
use crate::rate::{self, RateLimits, RateSettings};
use crate::ws::{WsConfig, WsState};

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
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub health: HealthConfig,
    pub db_retry: RetryConfig,
    pub http: HttpConfig,
    pub ws: WsConfig,
    pub rate: RateSettings,
}

#[derive(Clone)]
pub struct AppState {
    pub health: Arc<HealthState>,
    pub ws: Arc<WsState>,
    pub net: Arc<NetState>,
    pub origin: Arc<OriginPolicy>,
    pub rate: Arc<RateLimits>,
}

impl AppState {
    /// The state with no trusted proxy and no internal secret (see [`AppState::with_net`]).
    /// The `Origin` policy uses the WebSocket's `PUBLIC_ORIGIN`.
    pub fn new(probe: Arc<dyn DbProbe>, settings: &Settings, ws: WsState) -> Self {
        Self {
            health: Arc::new(HealthState::new(probe, settings.health)),
            origin: Arc::new(OriginPolicy::new(ws.public_origin().to_owned())),
            ws: Arc::new(ws),
            net: Arc::new(NetState::default()),
            rate: Arc::new(RateLimits::new(&settings.rate)),
        }
    }

    /// Replaces the trusted proxies and the internal secret.
    pub fn with_net(mut self, net: NetState) -> Self {
        self.net = Arc::new(net);
        self
    }

    /// The client context settings from the configuration.
    pub fn net_from_config(config: &Config) -> NetState {
        NetState::new(
            config.trusted_proxies.clone(),
            config.internal_secret.clone(),
        )
    }

    /// The WebSocket state from the configuration.
    pub fn ws_from_config(config: &Config, settings: &Settings) -> WsState {
        WsState::new(
            config.public_origin.clone(),
            config.echo.is_enabled(),
            config.ws_max_connections,
            settings.ws,
        )
    }
}

impl FromRef<AppState> for Arc<HealthState> {
    fn from_ref(state: &AppState) -> Self {
        state.health.clone()
    }
}

impl FromRef<AppState> for Arc<WsState> {
    fn from_ref(state: &AppState) -> Self {
        state.ws.clone()
    }
}

impl FromRef<AppState> for Arc<RateLimits> {
    fn from_ref(state: &AppState) -> Self {
        state.rate.clone()
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
    let ws = AppState::ws_from_config(config, &settings);
    let state = AppState::new(Arc::new(PgProbe::new(pool)), &settings, ws)
        .with_net(AppState::net_from_config(config));
    run_until_shutdown(listener, state, settings.http, shutdown).await
}

/// Serves until `shutdown` resolves, then stops accepting, closes every WebSocket with 1012
/// and waits for the HTTP connections and the sockets, bounded by [`SHUTDOWN_DEADLINE`]. The
/// rate limiter sweeper runs until the signal.
pub async fn run_until_shutdown(
    listener: TcpListener,
    state: AppState,
    http_config: HttpConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), StartupError> {
    let sockets = state.ws.tracker();
    let close_sockets = state.ws.shutdown_token();

    // On the signal: stop accepting and close every WebSocket with 1012 at the same time.
    let stop = CancellationToken::new();
    tokio::spawn(rate::sweeper(state.rate.clone(), stop.clone()));
    let on_signal = stop.clone();
    tokio::spawn(async move {
        shutdown.await;
        on_signal.cancel();
    });
    let stopped = stop.clone();
    let mut server = tokio::spawn(serve_listener(listener, state, http_config, async move {
        stopped.cancelled().await
    }));
    // `biased`: once the signal arrives, `serve_listener` also finishes right away, and an
    // unbiased select could pick that branch and skip closing the sockets.
    let ended_early = tokio::select! {
        biased;
        () = stop.cancelled() => None,
        result = &mut server => Some(result),
    };
    tracing::info!(event = "server.stopping");
    close_sockets.cancel();
    sockets.close();
    // hyper does not track upgraded connections: wait for them through the tracker. The
    // whole drain is bounded so the process ends within Docker's 10 s (ADR-0006 D10).
    let drained = tokio::time::timeout(SHUTDOWN_DEADLINE, async {
        if ended_early.is_none() {
            let _ = (&mut server).await;
        }
        sockets.wait().await;
    })
    .await;
    if drained.is_err() {
        server.abort();
        tracing::warn!(event = "server.shutdown_timeout");
    }
    match ended_early {
        // The server stopped without a signal: report it.
        Some(result) => result
            .map_err(|_| StartupError::Serve)?
            .map_err(|_| StartupError::Serve),
        None => Ok(()),
    }
}

/// Upper bound for the drain after the signal (Docker sends SIGKILL at 10 s).
pub const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(9);

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
        let (stream, peer) = tokio::select! {
            biased;
            () = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                // Usually out of file descriptors: back off instead of spinning.
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
        };
        // The peer of the connection, for `net::layer` (008 plan-a decision 3).
        let with_peer = tower::util::MapRequest::new(
            router.clone(),
            move |mut request: axum::http::Request<hyper::body::Incoming>| {
                request
                    .extensions_mut()
                    .insert(ConnectInfo::<SocketAddr>(peer));
                request
            },
        );
        let service = TowerToHyperService::new(with_peer);
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
