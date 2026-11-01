//! WebSocket `/ws` (RF-B13, plan-b decision 10).

pub mod channels;
mod handshake;
pub mod ip_slots;
pub mod rate;
pub mod registry;
pub mod self_hub;
mod session;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use std::future::Future;
use std::pin::Pin;

pub use handshake::handler;
pub use ip_slots::WS_ANON_PER_IP;
use ip_slots::{IpSlots, Slots, WS_PER_USER, WS_SESSION_PER_IP};
use registry::Registry;
use self_hub::SelfHub;

use crate::net::IpKey;

/// Pauses around the registration of a socket with a session (RF-B14): after the first
/// validation and before the registration, and between the registration and the second
/// validation. Production uses [`NoGate`]; the race tests hold the socket at each point to
/// revoke its session there.
pub trait ValidationGate: Send + Sync {
    fn before_register(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }

    fn before_validate(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

pub struct NoGate;

impl ValidationGate for NoGate {
    fn before_validate(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

/// Largest message and frame the server reads, fixed in the protocol (RF-B13).
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// The largest message the server sends: a socket keeps that capacity until it closes (010 DP-B9).
pub const MAX_SERVER_MESSAGE_BYTES: usize = 8 * 1024;

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
    /// A socket with a session revalidates it this often (RF-B14, D6: 5 min).
    pub revalidate_interval: Duration,
    /// Bound of that revalidation query; past it the socket stays, as on a database error.
    pub revalidate_timeout: Duration,
    /// Bound of the second validation of the handshake; past it, or on a database error, the
    /// socket closes with `auth.try_again`. Its own setting, so a test that shortens the
    /// periodic check never closes sockets in their handshake (round 4 of #54).
    pub handshake_validate_timeout: Duration,
    /// The read buffer of a socket. It is written whole on every read, so it stays resident: the
    /// default of 128 KiB would be 250 MiB with 2000 sockets (010 amendment DP-B1).
    pub read_buffer: usize,
    /// From this size on, a socket writes to the network instead of buffering (010 RF43).
    pub write_buffer: usize,
    /// The most a socket buffers. It has to exceed `write_buffer` (tungstenite panics otherwise). No
    /// message of the server passes [`MAX_SERVER_MESSAGE_BYTES`] (010 amendment DP-B9).
    pub max_write_buffer: usize,
    /// How many `fight_sync` a socket can ask for by subscribing, and in what window (DP-B7).
    pub subscribe_sync_limit: (usize, Duration),
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(120),
            close_grace: Duration::from_secs(5),
            revalidate_interval: Duration::from_secs(300),
            revalidate_timeout: Duration::from_secs(5),
            handshake_validate_timeout: Duration::from_secs(5),
            read_buffer: 8 * 1024,
            write_buffer: 8 * 1024,
            max_write_buffer: 16 * 1024,
            subscribe_sync_limit: (rate::SUBSCRIBE_SYNC_LIMIT, rate::SUBSCRIBE_SYNC_WINDOW),
        }
    }
}

pub struct WsState {
    public_origin: String,
    echo: bool,
    permits: Arc<Semaphore>,
    ip_slots: Arc<IpSlots>,
    session_ip_slots: Arc<Slots<IpKey>>,
    user_slots: Arc<Slots<uuid::Uuid>>,
    registry: Arc<Registry>,
    self_hub: Arc<SelfHub>,
    gate: Arc<dyn ValidationGate>,
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
            session_ip_slots: Arc::new(Slots::new(WS_SESSION_PER_IP)),
            user_slots: Arc::new(Slots::new(WS_PER_USER)),
            registry: Arc::new(Registry::default()),
            self_hub: Arc::new(SelfHub::default()),
            gate: Arc::new(NoGate),
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

    /// Replaces the gate between registration and validation (the race test).
    pub fn with_validation_gate(mut self, gate: Arc<dyn ValidationGate>) -> Self {
        self.gate = gate;
        self
    }

    /// Sockets with a session.
    pub fn session_sockets(&self) -> usize {
        self.registry.len()
    }

    /// The registry of sockets with a session (the revocation listener closes them).
    pub fn registry(&self) -> Arc<Registry> {
        self.registry.clone()
    }

    /// The `self` channel (TA-27 and the rename of phase C publish here).
    pub fn self_hub(&self) -> Arc<SelfHub> {
        self.self_hub.clone()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// tungstenite panics when it is built with a maximum that does not exceed the write buffer.
    #[test]
    fn the_default_buffers_are_valid() {
        let config = WsConfig::default();
        assert!(config.max_write_buffer > config.write_buffer);
        assert!(config.read_buffer > MAX_MESSAGE_BYTES);
        assert!(config.max_write_buffer >= 2 * MAX_SERVER_MESSAGE_BYTES);
    }
}
