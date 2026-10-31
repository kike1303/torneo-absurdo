//! The fight scheduler (feature 010, phase A): a fight every `cadence` on a fixed grid, carried by
//! a state machine in Postgres from `scheduled` to `settled`. See `runner` for the loop, `state`
//! for its pure decision, `sealed` for the only module that touches the seed and the log, and
//! `store` for the only SQL of `fights`.
//!
//! Once the commit is published the runner never ends a fight without a result: if it cannot go
//! on, it stops until someone fixes it (RF16).

pub mod api;
pub mod bets_contract;
pub mod board;
pub mod clock;
pub mod hook;
pub mod hub;
pub mod matchup;
pub mod release;
pub mod roster;
pub mod runner;
pub mod sealed;
pub mod snapshot;
pub mod state;
pub mod store;

use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

pub use board::FightsShared;
pub use clock::FightClock;

/// The ruleset of the new fights (RF22).
pub const RULESET_VERSION: u16 = 1;
/// How far behind the server the client plays (published in every summary).
pub const PLAYBACK_DELAY_MS: u32 = 3500;
/// From `end` to `Settled`: long enough to have seen the closing pose (spec, "Margen").
pub const SETTLE_DELAY_MS: u32 = 5000;
/// How long after `now` a late fight starts (RF15).
pub const LATE_START_WAIT_MS: i64 = 10_000;
/// Up to this long after `live_at`, a fight still starts at `live_at` (RF15).
pub const ON_TIME_MARGIN_MS: i64 = 1000;
/// The ceiling of the minimum window to open late (amendment DP-1).
pub const MAX_MIN_OPEN_WINDOW_MS: i64 = 60_000;

/// What the environment configures (RF11, RF22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FightsConfig {
    pub cadence_ms: i64,
    pub window_ms: i64,
    pub pre_live_ms: i64,
    pub content_dir: PathBuf,
}

impl FightsConfig {
    /// `(cadence, betting window, pre-live)` by default, in seconds.
    pub const DEFAULT_SECS: (u32, u32, u32) = (600, 450, 30);
    /// The default `CONTENT_DIR`, relative to the root of the repo.
    pub const DEFAULT_CONTENT_DIR: &'static str = "content/characters";
}

/// A pause, for the race tests (test code only, never an environment variable).
pub trait Gate: Send + Sync {
    fn wait(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// The gate of production: no pause.
pub struct NoGate;

impl Gate for NoGate {
    fn wait(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

/// Called at the start of every pass of the runner (a test makes it panic).
pub trait PassProbe: Send + Sync {
    fn before_pass(&self);
}

/// The probe of production: nothing.
pub struct NoProbe;

impl PassProbe for NoProbe {
    fn before_pass(&self) {}
}

/// The times of the runner. Constants in production; the tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct RunnerTimings {
    /// Without the lock, how often to ask for it (RF10).
    pub lock_retry_ms: u64,
    /// How often the lock connection is proven alive, and how long the proof may take.
    pub heartbeat_ms: u64,
    pub heartbeat_timeout_ms: u64,
    /// The server closes the lock session after this long without a heartbeat (R5).
    pub idle_session_ms: u64,
    /// How often the unsettled rows are read again, with or without a halt (RF15).
    pub reread_ms: i64,
    /// A settlement failing for this long halts the fights (RF18).
    pub settle_halt_ms: i64,
    pub settle_backoff_min_ms: i64,
    pub settle_backoff_max_ms: i64,
    pub attempt_timeout_ms: u64,
    /// The `lock_timeout` of every transition (RF8).
    pub lock_timeout_ms: u64,
    pub db_backoff_min_ms: u64,
    pub db_backoff_max_ms: u64,
    pub panic_backoff_ms: u64,
    /// The longest a pass of the loop may take.
    pub pass_timeout_ms: u64,
}

impl Default for RunnerTimings {
    fn default() -> Self {
        Self {
            lock_retry_ms: 5000,
            heartbeat_ms: 5000,
            heartbeat_timeout_ms: 3000,
            idle_session_ms: 30_000,
            reread_ms: 60_000,
            settle_halt_ms: 60_000,
            settle_backoff_min_ms: 1000,
            settle_backoff_max_ms: 30_000,
            attempt_timeout_ms: 30_000,
            lock_timeout_ms: 2000,
            db_backoff_min_ms: 1000,
            db_backoff_max_ms: 30_000,
            panic_backoff_ms: 1000,
            pass_timeout_ms: 30_000,
        }
    }
}

/// What is not an environment variable (the tests replace it).
#[derive(Clone)]
pub struct FightSettings {
    pub clock: FightClock,
    pub hook: Arc<dyn hook::SettlementHook>,
    pub compute_gate: Arc<dyn Gate>,
    pub sink: Arc<dyn release::EventSink>,
    pub probe: Arc<dyn PassProbe>,
    pub timings: RunnerTimings,
}

impl Default for FightSettings {
    fn default() -> Self {
        Self {
            clock: FightClock::default(),
            hook: Arc::new(hook::NoBets),
            compute_gate: Arc::new(NoGate),
            sink: Arc::new(release::DiscardEvents),
            probe: Arc::new(NoProbe),
            timings: RunnerTimings::default(),
        }
    }
}

impl fmt::Debug for FightSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FightSettings(..)")
    }
}

/// Lowercase hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_and_exact() {
        assert_eq!(hex(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
        assert_eq!(hex(&[]), "");
    }
}
