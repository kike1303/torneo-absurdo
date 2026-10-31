//! The fight scheduler (feature 010, phase A): a fight every `cadence` on a fixed grid, carried by
//! a state machine in Postgres from `scheduled` to `settled`. See `runner` for the loop, `state`
//! for its pure decision, `sealed` for the only module that touches the seed and the log, and
//! `store` for the only SQL of `fights`.
//!
//! Once the commit is published the runner never ends a fight without a result: if it cannot go
//! on, it stops until someone fixes it (RF16).

pub mod bets_contract;
pub mod board;
pub mod clock;
pub mod hook;
pub mod matchup;
pub mod roster;
pub mod sealed;
pub mod snapshot;
pub mod state;
pub mod store;

use std::path::PathBuf;

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
