//! WebSocket `/ws` (RF-B13, plan-b decision 10).

mod handshake;
pub mod ip_slots;
pub mod rate;
mod session;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use handshake::handler;
use ip_slots::IpSlots;
pub use ip_slots::WS_ANON_PER_IP;

/// Largest message and frame the server reads, fixed in the protocol (RF-B13).
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// Socket lifetimes. Not environment variables; the tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct WsConfig {
    /// The server pings this often.
    pub ping_interval: Duration,
    /// Closed with `ws.pong_timeout` if no frame at all arrives in this time.
    pub pong_timeout: Duration,
    /// Closed with `ws.idle_timeout` if no data message is accepted in this time.
    pub idle_timeout: Duration,
    /// Bound for every send and for the close handshake; then the TCP connection is dropped.
    pub close_grace: Duration,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(120),
            close_grace: Duration::from_secs(5),
        }
    }
}

pub struct WsState {
    public_origin: String,
    echo: bool,
    permits: Arc<Semaphore>,
    ip_slots: Arc<IpSlots>,
    config: WsConfig,
    tracker: TaskTracker,
    shutdown: CancellationToken,
}

impl WsState {
    pub fn new(public_origin: String, echo: bool, max_connections: u32, config: WsConfig) -> Self {
        let permits = usize::try_from(max_connections).unwrap_or(usize::MAX);
        Self {
            public_origin,
            echo,
            permits: Arc::new(Semaphore::new(permits)),
            ip_slots: Arc::new(IpSlots::new(WS_ANON_PER_IP)),
            config,
            tracker: TaskTracker::new(),
            shutdown: CancellationToken::new(),
        }
    }

    /// Replaces the anonymous sockets per `IpKey` limit ([`WS_ANON_PER_IP`]); the tests
    /// shorten it.
    pub fn with_ip_limit(mut self, limit: u32) -> Self {
        self.ip_slots = Arc::new(IpSlots::new(limit));
        self
    }

    /// The exact `Origin` the handshake accepts (`PUBLIC_ORIGIN`).
    pub fn public_origin(&self) -> &str {
        &self.public_origin
    }

    /// `IpKey`s with at least one anonymous socket.
    pub fn ip_keys_in_use(&self) -> usize {
        self.ip_slots.keys()
    }

    /// Cancelled at shutdown: every socket closes with 1012.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Tracks every upgraded socket (`close()` then `wait()` at shutdown).
    pub fn tracker(&self) -> TaskTracker {
        self.tracker.clone()
    }

    /// Free permits of the global socket cap.
    pub fn available_permits(&self) -> usize {
        self.permits.available_permits()
    }
}
