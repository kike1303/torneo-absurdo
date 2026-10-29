//! In-memory rate limiter per `IpKey` (008 RF-A4, ADR-0008 D7), on `governor` (GCRA).
//!
//! Each limiter has an exact cap of keys. A check of an existing key holds the admission
//! lock shared, so the sweep (exclusive) cannot drop the key between the lookup and the
//! measurement; a new key only enters with the lock held exclusively, after the cap check.
//! So the table never goes over [`MAX_KEYS`] and no live key is ever evicted (evicting it
//! would reset the bucket of whoever drained it).

pub mod clock;
pub mod http;
pub mod quotas;
pub mod store;

#[cfg(test)]
use std::sync::Mutex;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use governor::clock::Clock;
use governor::middleware::NoOpMiddleware;
use governor::nanos::Nanos;
use governor::{Quota, RateLimiter};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

pub use clock::ServerClock;
pub use quotas::{AUTH_IP, FULL_RETRY_AFTER_SECS, MAX_KEYS, WS_ANON_UPGRADE_IP};
use store::CappedStore;

use crate::net::IpKey;

/// How often the sweeper drops keys that are indistinguishable from fresh ones.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// The outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Over the quota: retry in this many seconds (at least 1).
    Limited {
        retry_after_secs: u64,
    },
    /// A new key with the table full: not admitted (`Retry-After` is the sweep interval).
    Full,
}

impl Decision {
    /// `Retry-After` for a rejection, `None` for [`Decision::Allow`].
    pub fn retry_after_secs(self) -> Option<u64> {
        match self {
            Self::Allow => None,
            Self::Limited { retry_after_secs } => Some(retry_after_secs),
            Self::Full => Some(FULL_RETRY_AFTER_SECS),
        }
    }
}

/// Parameters that are not environment variables (the tests shorten them).
#[derive(Debug, Clone)]
pub struct RateSettings {
    pub max_keys: usize,
    pub clock: ServerClock,
}

impl Default for RateSettings {
    fn default() -> Self {
        Self {
            max_keys: MAX_KEYS,
            clock: ServerClock::default(),
        }
    }
}

pub struct KeyedLimiter {
    limiter: RateLimiter<IpKey, CappedStore, ServerClock, NoOpMiddleware<Nanos>>,
    store: CappedStore,
    clock: ServerClock,
    max_keys: usize,
    admission: RwLock<()>,
    /// Runs once between the existence check and the measurement (race tests only).
    #[cfg(test)]
    hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl KeyedLimiter {
    pub fn new(quota: Quota, settings: &RateSettings) -> Self {
        let store = CappedStore::default();
        Self {
            limiter: RateLimiter::new(quota, store.clone(), settings.clock.clone()),
            store,
            clock: settings.clock.clone(),
            max_keys: settings.max_keys,
            admission: RwLock::new(()),
            #[cfg(test)]
            hook: Mutex::new(None),
        }
    }

    pub fn check(&self, key: &IpKey) -> Decision {
        {
            let _shared = self
                .admission
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            if self.store.contains(key) {
                #[cfg(test)]
                self.run_hook();
                // No sweep can run while the shared lock is held: the key is still there.
                return self.measure(key);
            }
        }
        let _exclusive = self
            .admission
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if !self.store.contains(key) && self.limiter.len() >= self.max_keys {
            return Decision::Full;
        }
        self.measure(key)
    }

    /// Whether a sweep or the admission of a new key would have to wait right now.
    #[cfg(test)]
    fn exclusive_would_block(&self) -> bool {
        matches!(
            self.admission.try_write(),
            Err(std::sync::TryLockError::WouldBlock)
        )
    }

    #[cfg(test)]
    fn run_hook(&self) {
        let hook = self
            .hook
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }

    fn measure(&self, key: &IpKey) -> Decision {
        match self.limiter.check_key(key) {
            Ok(()) => Decision::Allow,
            Err(not_until) => {
                let wait = not_until.wait_time_from(self.clock.now());
                let secs = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
                Decision::Limited {
                    retry_after_secs: secs.max(1),
                }
            }
        }
    }

    /// Drops the keys whose bucket is full again, and gives the memory back.
    pub fn sweep(&self) {
        let _exclusive = self
            .admission
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        self.limiter.retain_recent();
        self.limiter.shrink_to_fit();
    }

    pub fn len(&self) -> usize {
        self.limiter.len()
    }

    pub fn is_empty(&self) -> bool {
        self.limiter.is_empty()
    }

    /// Buckets allocated by the key table.
    pub fn capacity(&self) -> usize {
        self.store.capacity()
    }
}

/// Every limiter of the server.
pub struct RateLimits {
    /// `AUTH_IP` (mounted on the auth routes in phase B).
    pub auth_ip: Arc<KeyedLimiter>,
    /// `WS_ANON_UPGRADE_IP`, checked in the `/ws` handshake (RF-A7).
    pub ws_upgrade_ip: Arc<KeyedLimiter>,
}

impl RateLimits {
    pub fn new(settings: &RateSettings) -> Self {
        Self {
            auth_ip: Arc::new(KeyedLimiter::new(AUTH_IP, settings)),
            ws_upgrade_ip: Arc::new(KeyedLimiter::new(WS_ANON_UPGRADE_IP, settings)),
        }
    }

    pub fn sweep(&self) {
        self.auth_ip.sweep();
        self.ws_upgrade_ip.sweep();
    }
}

/// Sweeps every [`SWEEP_INTERVAL`] until `stop` is cancelled. Logs nothing (RF-A9).
pub async fn sweeper(limits: Arc<RateLimits>, stop: CancellationToken) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // The first tick is immediate: nothing to sweep yet.
    interval.tick().await;
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            _ = interval.tick() => limits.sweep(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use governor::state::InMemoryState;

    use super::*;

    fn key(n: u32) -> IpKey {
        IpKey::from(IpAddr::V4(Ipv4Addr::from(n)))
    }

    fn manual(max_keys: usize) -> (RateSettings, ServerClock) {
        let clock = ServerClock::manual();
        (
            RateSettings {
                max_keys,
                clock: clock.clone(),
            },
            clock,
        )
    }

    #[test]
    fn auth_ip_burst_five_then_one_every_six_seconds() {
        let (settings, clock) = manual(MAX_KEYS);
        let limiter = KeyedLimiter::new(AUTH_IP, &settings);
        for i in 0..5 {
            assert_eq!(limiter.check(&key(1)), Decision::Allow, "{i}");
        }
        assert_eq!(
            limiter.check(&key(1)),
            Decision::Limited {
                retry_after_secs: 6
            }
        );
        clock.advance(Duration::from_secs(6));
        assert_eq!(limiter.check(&key(1)), Decision::Allow);
        assert!(matches!(limiter.check(&key(1)), Decision::Limited { .. }));
        // Another key has its own bucket.
        assert_eq!(limiter.check(&key(2)), Decision::Allow);
    }

    #[test]
    fn retry_after_rounds_up() {
        let (settings, clock) = manual(MAX_KEYS);
        let limiter = KeyedLimiter::new(AUTH_IP, &settings);
        for _ in 0..5 {
            limiter.check(&key(1));
        }
        clock.advance(Duration::from_millis(500));
        assert_eq!(
            limiter.check(&key(1)),
            Decision::Limited {
                retry_after_secs: 6
            }
        );
        clock.advance(Duration::from_millis(5_400));
        assert_eq!(
            limiter.check(&key(1)),
            Decision::Limited {
                retry_after_secs: 1
            }
        );
    }

    #[test]
    fn ws_upgrade_burst_ten() {
        let (settings, clock) = manual(MAX_KEYS);
        let limiter = KeyedLimiter::new(WS_ANON_UPGRADE_IP, &settings);
        for i in 0..10 {
            assert_eq!(limiter.check(&key(1)), Decision::Allow, "{i}");
        }
        assert_eq!(
            limiter.check(&key(1)),
            Decision::Limited {
                retry_after_secs: 2
            }
        );
        clock.advance(Duration::from_secs(2));
        assert_eq!(limiter.check(&key(1)), Decision::Allow);
    }

    #[test]
    fn a_full_table_refuses_new_keys_and_never_evicts() {
        let (settings, _clock) = manual(3);
        let limiter = KeyedLimiter::new(AUTH_IP, &settings);
        // Key 1 drains its bucket; keys 2 and 3 still have tokens.
        for _ in 0..5 {
            assert_eq!(limiter.check(&key(1)), Decision::Allow);
        }
        assert_eq!(limiter.check(&key(2)), Decision::Allow);
        assert_eq!(limiter.check(&key(3)), Decision::Allow);
        assert_eq!(limiter.check(&key(4)), Decision::Full);
        assert_eq!(Decision::Full.retry_after_secs(), Some(60));
        assert_eq!(limiter.len(), 3);
        // The drained key keeps its bucket (not reset), the others keep theirs.
        assert!(matches!(limiter.check(&key(1)), Decision::Limited { .. }));
        assert_eq!(limiter.check(&key(2)), Decision::Allow);
        assert_eq!(limiter.check(&key(4)), Decision::Full);
    }

    #[test]
    fn the_sweep_frees_refilled_keys_only() {
        let (settings, clock) = manual(2);
        let limiter = KeyedLimiter::new(AUTH_IP, &settings);
        for _ in 0..5 {
            limiter.check(&key(1));
        }
        limiter.check(&key(2));
        assert_eq!(limiter.check(&key(3)), Decision::Full);
        // governor drops a key one interval after its bucket is full again: after 12 s key 2
        // (one token used) goes; key 1 (five used, full at 30 s) stays.
        clock.advance(Duration::from_secs(12));
        limiter.sweep();
        assert_eq!(limiter.len(), 1);
        assert_eq!(limiter.check(&key(3)), Decision::Allow);
        assert_eq!(limiter.check(&key(4)), Decision::Full);
        // Once everything refills, the sweep empties the table.
        clock.advance(Duration::from_secs(60));
        limiter.sweep();
        assert!(limiter.is_empty());
    }

    /// QA race: a sweep drops K and a new key L takes the free slot while K's check is in
    /// flight; re-inserting K must not take the table over the cap.
    #[test]
    fn a_swept_key_in_flight_never_takes_the_table_over_the_cap() {
        let (settings, clock) = manual(1);
        let limiter = Arc::new(KeyedLimiter::new(AUTH_IP, &settings));
        assert_eq!(limiter.check(&key(1)), Decision::Allow);
        // K is indistinguishable from fresh: the next sweep would drop it.
        clock.advance(Duration::from_secs(12));
        // Inside K's in-flight check (between the lookup and the measurement), neither a sweep
        // nor the admission of a new key can take the lock: checked without threads.
        let blocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = blocked.clone();
        let racer = limiter.clone();
        *limiter.hook.lock().unwrap() = Some(Box::new(move || {
            seen.store(
                racer.exclusive_would_block(),
                std::sync::atomic::Ordering::SeqCst,
            );
        }));
        assert_eq!(limiter.check(&key(1)), Decision::Allow);
        assert!(blocked.load(std::sync::atomic::Ordering::SeqCst));
        // What the racer would do once K's check ends: K was measured again, so it stays,
        // and the new key finds the table full.
        limiter.sweep();
        assert_eq!(limiter.check(&key(2)), Decision::Full);
        assert_eq!(limiter.len(), 1);
        assert!(!limiter.exclusive_would_block());
    }

    #[test]
    fn a_hundred_thousand_keys_stay_under_twenty_megabytes() {
        let (settings, _clock) = manual(MAX_KEYS);
        let limiter = KeyedLimiter::new(WS_ANON_UPGRADE_IP, &settings);
        let v6 = |n: u64| {
            let high = u128::from(0x2001_0db8_0000_0000u64 | n) << 64;
            IpKey::from(IpAddr::V6(Ipv6Addr::from(high | 1)))
        };
        for n in 0..u64::try_from(MAX_KEYS).unwrap() {
            assert_eq!(limiter.check(&v6(n)), Decision::Allow);
        }
        assert_eq!(limiter.len(), MAX_KEYS);
        assert_eq!(limiter.check(&v6(1_000_000)), Decision::Full);
        assert_eq!(limiter.check(&v6(0)), Decision::Allow);
        // hashbrown's layout: buckets × (entry + 1 control byte) + a group of control bytes.
        // `capacity()` is 7/8 of the buckets, a power of two.
        let buckets = (limiter.capacity() * 8 / 7).next_power_of_two();
        let entry = std::mem::size_of::<(IpKey, InMemoryState)>();
        let bytes = buckets * (entry + 1) + 16;
        assert!(bytes < 20 * 1024 * 1024, "{bytes} bytes");
    }

    #[tokio::test]
    async fn the_sweeper_stops_when_cancelled() {
        let limits = Arc::new(RateLimits::new(&RateSettings::default()));
        let stop = CancellationToken::new();
        let task = tokio::spawn(sweeper(limits, stop.clone()));
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }
}
