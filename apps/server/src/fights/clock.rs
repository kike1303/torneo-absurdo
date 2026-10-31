//! The clock of the fights (RF14, RF40): the wall clock in production, a manual one in the tests.
//!
//! Like `rate::ServerClock`, the tests do not use `tokio::time::pause()`: it would also advance
//! hyper's and the database timers. A manual clock wakes whoever waits on it when it moves.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::Notify;

#[derive(Debug, Default)]
pub struct ManualTime {
    now_ms: AtomicI64,
    moved: Notify,
}

#[derive(Debug, Clone, Default)]
pub enum FightClock {
    /// The wall clock, in milliseconds since the Unix epoch.
    #[default]
    Real,
    /// A counter that only [`FightClock::set`] and [`FightClock::advance`] move. Clones share it.
    Manual(Arc<ManualTime>),
}

impl FightClock {
    /// A manual clock at `now_ms`.
    pub fn manual(now_ms: i64) -> Self {
        Self::Manual(Arc::new(ManualTime {
            now_ms: AtomicI64::new(now_ms),
            moved: Notify::new(),
        }))
    }

    pub fn now_ms(&self) -> i64 {
        match self {
            Self::Real => wall_ms(),
            Self::Manual(time) => time.now_ms.load(Ordering::SeqCst),
        }
    }

    /// Moves a manual clock to `now_ms`. A real clock ignores it.
    pub fn set(&self, now_ms: i64) {
        if let Self::Manual(time) = self {
            time.now_ms.store(now_ms, Ordering::SeqCst);
            time.moved.notify_waiters();
        }
    }

    /// Moves a manual clock forward. A real clock ignores it.
    pub fn advance(&self, by_ms: i64) {
        self.set(self.now_ms().saturating_add(by_ms));
    }

    /// Resolves once the clock reaches `deadline_ms`.
    pub async fn sleep_until(&self, deadline_ms: i64) {
        match self {
            Self::Real => loop {
                let left = deadline_ms.saturating_sub(wall_ms());
                let Ok(left) = u64::try_from(left) else {
                    return;
                };
                if left == 0 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(left)).await;
            },
            Self::Manual(time) => loop {
                // Before the check: a move between the check and the await is not lost.
                let moved = time.moved.notified();
                if time.now_ms.load(Ordering::SeqCst) >= deadline_ms {
                    return;
                }
                moved.await;
            },
        }
    }

    /// The time since `started_at_ms` from now on (RF40).
    pub fn stopwatch(&self, started_at_ms: i64) -> Stopwatch {
        match self {
            Self::Real => Stopwatch::Real {
                at_start: wall_ms().saturating_sub(started_at_ms),
                start: Instant::now(),
            },
            Self::Manual(_) => Stopwatch::Manual {
                clock: self.clone(),
                started_at_ms,
            },
        }
    }
}

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

/// From the wall clock when it starts, and from the monotonic one after that: a jump of the wall
/// clock during a fight does not move it (RF40).
#[derive(Debug, Clone)]
pub enum Stopwatch {
    Real {
        at_start: i64,
        start: Instant,
    },
    Manual {
        clock: FightClock,
        started_at_ms: i64,
    },
}

impl Stopwatch {
    /// Milliseconds since the start, or `None` before it: a fight that has not started releases
    /// nothing, not even its first event (R4).
    pub fn elapsed_ms(&self) -> Option<u32> {
        let elapsed = match self {
            Self::Real { at_start, start } => {
                let since = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);
                at_start.saturating_add(since)
            }
            Self::Manual {
                clock,
                started_at_ms,
            } => clock.now_ms().saturating_sub(*started_at_ms),
        };
        if elapsed < 0 {
            return None;
        }
        Some(u32::try_from(elapsed).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_told_and_clones_share_it() {
        let clock = FightClock::manual(100);
        let other = clock.clone();
        assert_eq!(clock.now_ms(), 100);
        other.advance(50);
        assert_eq!(clock.now_ms(), 150);
        other.set(10);
        assert_eq!(clock.now_ms(), 10);
    }

    #[test]
    fn the_stopwatch_does_not_start_before_its_time_nor_overflow() {
        let clock = FightClock::manual(1000);
        let watch = clock.stopwatch(1500);
        assert_eq!(watch.elapsed_ms(), None);
        clock.set(1499);
        assert_eq!(watch.elapsed_ms(), None);
        clock.set(1500);
        assert_eq!(watch.elapsed_ms(), Some(0));
        clock.set(1501);
        assert_eq!(watch.elapsed_ms(), Some(1));
        // Saturating subtraction, on the extremes.
        assert_eq!(
            FightClock::manual(i64::MIN)
                .stopwatch(i64::MAX)
                .elapsed_ms(),
            None
        );
        assert_eq!(
            FightClock::manual(i64::MAX)
                .stopwatch(i64::MIN)
                .elapsed_ms(),
            Some(u32::MAX)
        );
    }

    #[test]
    fn a_real_stopwatch_uses_the_time_at_its_start() {
        let now = FightClock::Real.now_ms();
        assert_eq!(
            FightClock::Real.stopwatch(now + 3_600_000).elapsed_ms(),
            None
        );
        let elapsed = FightClock::Real
            .stopwatch(now - 3_600_000)
            .elapsed_ms()
            .unwrap();
        assert!(elapsed >= 3_600_000, "{elapsed}");
    }

    #[tokio::test]
    async fn sleeping_on_a_manual_clock_wakes_when_it_moves() {
        let clock = FightClock::manual(0);
        let waiter = clock.clone();
        let task = tokio::spawn(async move { waiter.sleep_until(100).await });
        tokio::task::yield_now().await;
        clock.set(99);
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        // Wait for it to be parked again, then reach the deadline.
        tokio::time::sleep(Duration::from_millis(20)).await;
        clock.set(100);
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn a_past_deadline_resolves_at_once() {
        FightClock::manual(5).sleep_until(5).await;
        let real = FightClock::Real;
        real.sleep_until(real.now_ms() - 1).await;
    }
}
