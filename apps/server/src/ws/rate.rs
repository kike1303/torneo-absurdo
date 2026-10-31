//! Sliding window of 10 messages per 10 s per socket (RF-B13).

use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

pub const LIMIT: usize = 10;
pub const WINDOW: Duration = Duration::from_secs(10);

/// How many `fight_sync` a socket can ask for by subscribing, per minute (010 amendment DP-B7). A
/// message of 40 bytes asks for up to 47 KB, and the way out of the server is shared. The ones the
/// server sends by itself do not count.
pub const SUBSCRIBE_SYNC_LIMIT: usize = 3;
pub const SUBSCRIBE_SYNC_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct RateWindow {
    limit: usize,
    window: Duration,
    hits: VecDeque<Instant>,
}

impl Default for RateWindow {
    fn default() -> Self {
        Self::new(LIMIT, WINDOW)
    }
}

impl RateWindow {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            hits: VecDeque::with_capacity(limit),
        }
    }

    /// Records a message at `now`; `false` if it is over the limit.
    pub fn allow(&mut self, now: Instant) -> bool {
        while self
            .hits
            .front()
            .is_some_and(|hit| now.duration_since(*hit) >= self.window)
        {
            self.hits.pop_front();
        }
        if self.hits.len() >= self.limit {
            return false;
        }
        self.hits.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eleventh_message_in_the_window_is_refused() {
        let start = Instant::now();
        let mut window = RateWindow::default();
        for i in 0..10 {
            assert!(window.allow(start + Duration::from_millis(i * 100)), "{i}");
        }
        assert!(!window.allow(start + Duration::from_secs(9)));
    }

    #[test]
    fn old_messages_leave_the_window() {
        let start = Instant::now();
        let mut window = RateWindow::default();
        for _ in 0..10 {
            assert!(window.allow(start));
        }
        assert!(!window.allow(start + Duration::from_millis(9_999)));
        assert!(window.allow(start + Duration::from_secs(10)));
    }

    #[test]
    fn a_refused_message_does_not_count() {
        let start = Instant::now();
        let mut window = RateWindow::new(1, Duration::from_secs(1));
        assert!(window.allow(start));
        assert!(!window.allow(start + Duration::from_millis(500)));
        assert!(window.allow(start + Duration::from_secs(1)));
    }
}
