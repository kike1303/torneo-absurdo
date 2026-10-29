//! The quotas of ADR-0008 D7 that phase A mounts (008 RF-A4, RF-A7).

use std::num::NonZeroU32;

use governor::Quota;

const fn nonzero(n: u32) -> NonZeroU32 {
    match NonZeroU32::new(n) {
        Some(n) => n,
        None => panic!("a quota needs a positive number"),
    }
}

/// `start`, `link` and `callback` per `IpKey`: 10 per minute, burst 5 (mounted in phase B).
pub const AUTH_IP: Quota = Quota::per_minute(nonzero(10)).allow_burst(nonzero(5));

/// Anonymous `/ws` upgrades per `IpKey`: 30 per minute, burst 10.
pub const WS_ANON_UPGRADE_IP: Quota = Quota::per_minute(nonzero(30)).allow_burst(nonzero(10));

/// Keys per limiter. A constant, never an environment variable (RF-A4).
pub const MAX_KEYS: usize = 100_000;

/// `Retry-After` when the key table is full: the sweep interval.
pub const FULL_RETRY_AFTER_SECS: u64 = 60;
