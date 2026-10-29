//! The rate limiter clock (008 plan-a decision 6): monotonic in production, manual in tests.
//!
//! The tests do not use `tokio::time::pause()`: it would also advance hyper's and the
//! WebSocket timers in the tests with real sockets.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use governor::clock::Clock;
use governor::nanos::Nanos;

#[derive(Debug, Clone)]
pub enum ServerClock {
    /// Time elapsed since `start`.
    Monotonic { start: Instant },
    /// A counter in nanoseconds that only [`ServerClock::advance`] moves.
    Manual(Arc<AtomicU64>),
}

impl Default for ServerClock {
    fn default() -> Self {
        Self::Monotonic {
            start: Instant::now(),
        }
    }
}

impl ServerClock {
    /// A manual clock at zero. Clones share the counter.
    pub fn manual() -> Self {
        Self::Manual(Arc::new(AtomicU64::new(0)))
    }

    /// Moves a manual clock forward. A monotonic clock ignores it.
    pub fn advance(&self, by: Duration) {
        if let Self::Manual(nanos) = self {
            let by = u64::try_from(by.as_nanos()).unwrap_or(u64::MAX);
            nanos.fetch_add(by, Ordering::SeqCst);
        }
    }
}

impl Clock for ServerClock {
    type Instant = Nanos;

    fn now(&self) -> Nanos {
        match self {
            Self::Monotonic { start } => {
                Nanos::new(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX))
            }
            Self::Manual(nanos) => Nanos::new(nanos.load(Ordering::SeqCst)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_advanced_and_clones_share_it() {
        let clock = ServerClock::manual();
        let other = clock.clone();
        assert_eq!(clock.now().as_u64(), 0);
        other.advance(Duration::from_secs(6));
        assert_eq!(clock.now().as_u64(), 6_000_000_000);
    }

    #[test]
    fn a_monotonic_clock_never_goes_back() {
        let clock = ServerClock::default();
        let first = clock.now();
        clock.advance(Duration::from_secs(60));
        assert!(clock.now() >= first);
        assert!(clock.now().as_u64() < 60_000_000_000);
    }
}
