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

/// "Other mutations" (D7): 60/min per user and 240/min per IP. D7 gives no burst, so the burst
/// is the per-minute count (`per_minute`).
pub const MUTATION_USER: Quota = Quota::per_minute(nonzero(60));
pub const MUTATION_IP: Quota = Quota::per_minute(nonzero(240));

/// Upgrades of a `/ws` with a session (D7): 10/min per user and 120/min per IP.
pub const WS_SESSION_UPGRADE_USER: Quota = Quota::per_minute(nonzero(10));
pub const WS_SESSION_UPGRADE_IP: Quota = Quota::per_minute(nonzero(120));

/// `GET /api/fights/{id}` per `IpKey`: 120 per minute, burst 30 (010 RF34).
pub const FIGHT_READ_IP: Quota = Quota::per_minute(nonzero(120)).allow_burst(nonzero(30));

/// `POST /api/bets` (D7, 012 RF25): 30/min per user with a burst of 10, and 120/min per IP.
pub const BET_USER: Quota = Quota::per_minute(nonzero(30)).allow_burst(nonzero(10));
pub const BET_IP: Quota = Quota::per_minute(nonzero(120));

/// `bonus` and `bailout`, one bucket for both (D7, 012 RF29, RF30, plan-b D6): 5/min per user and
/// 60/min per IP.
pub const CLAIM_USER: Quota = Quota::per_minute(nonzero(5));
pub const CLAIM_IP: Quota = Quota::per_minute(nonzero(60));

/// `wallet`, `bets/mine` and `bets/history`, one bucket for all (012 RF37, 019): 60/min per user and 600/min per IP.
pub const WALLET_READ_USER: Quota = Quota::per_minute(nonzero(60));
pub const WALLET_READ_IP: Quota = Quota::per_minute(nonzero(600));

/// `GET /api/botarga` (021): the numbers of the wallet reads, in its own buckets.
pub const BOTARGA_READ_USER: Quota = Quota::per_minute(nonzero(60));
pub const BOTARGA_READ_IP: Quota = Quota::per_minute(nonzero(600));

/// `quote` (012 RF37): 120/min per user and 600/min per IP.
pub const QUOTE_USER: Quota = Quota::per_minute(nonzero(120));
pub const QUOTE_IP: Quota = Quota::per_minute(nonzero(600));

/// Keys per limiter. A constant, never an environment variable (RF-A4).
pub const MAX_KEYS: usize = 100_000;

/// `Retry-After` when the key table is full: the sweep interval.
pub const FULL_RETRY_AFTER_SECS: u64 = 60;
