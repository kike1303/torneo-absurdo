//! What the runner publishes in memory: the board of `GET /api/fights/current`, which never touches
//! the database (RF35), the gate of `lock_open_for_bet` (RF29) and the hub of the sockets (RF38).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use protocol::{FightSummary, FightsCurrent, TimestampMs};

use super::FightClock;
use super::hub::{FightHub, HubUpdate};

#[derive(Debug, Clone, Default)]
struct Board {
    current: Option<FightSummary>,
    next: Option<FightSummary>,
    last: Option<FightSummary>,
}

/// The board without its hour: what a socket on `schedule` has to be told about.
type Announced = (
    Option<bool>,
    Option<FightSummary>,
    Option<FightSummary>,
    Option<FightSummary>,
);

/// What the runner says in one go: the board and the fights of the hub.
#[derive(Default)]
pub struct BoardUpdate {
    pub halted: bool,
    pub current: Option<FightSummary>,
    pub next: Option<FightSummary>,
    pub last: Option<FightSummary>,
    pub hub: HubUpdate,
}

/// Shared between the runner, the API, the sockets and, from TA-27, the bets.
pub struct FightsShared {
    clock: FightClock,
    has_lock: AtomicBool,
    initial_done: AtomicBool,
    halted: AtomicBool,
    board: RwLock<Board>,
    hub: Arc<FightHub>,
    announced: Mutex<Announced>,
}

impl std::fmt::Debug for FightsShared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FightsShared(..)")
    }
}

impl FightsShared {
    pub fn new(clock: FightClock) -> Self {
        Self {
            hub: Arc::new(FightHub::new(clock.clone())),
            announced: Mutex::new((None, None, None, None)),
            clock,
            has_lock: AtomicBool::new(false),
            initial_done: AtomicBool::new(false),
            halted: AtomicBool::new(false),
            board: RwLock::new(Board::default()),
        }
    }

    pub const fn clock(&self) -> &FightClock {
        &self.clock
    }

    pub const fn hub(&self) -> &Arc<FightHub> {
        &self.hub
    }

    /// Tells the sockets if the board changed, with or without a pass of the runner: `halted` also
    /// changes with the lock and with the first evaluation (amendment DP-B3). The hour alone is no
    /// change, so more passes never mean more messages.
    fn announce(&self) {
        let mut announced = self
            .announced
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let board = self.current();
        let now = (
            board.halted,
            board.current.clone(),
            board.next.clone(),
            board.last.clone(),
        );
        if *announced != now {
            *announced = now;
            self.hub.schedule_changed(board);
        }
    }

    pub(super) fn lock_taken(&self) {
        self.has_lock.store(true, Ordering::SeqCst);
        self.announce();
    }

    /// Also resets the initial evaluation: it has to be done again with the lock.
    pub(super) fn lock_lost(&self) {
        self.has_lock.store(false, Ordering::SeqCst);
        self.initial_done.store(false, Ordering::SeqCst);
        self.announce();
    }

    pub(super) fn initial_evaluation_done(&self) {
        self.initial_done.store(true, Ordering::SeqCst);
        self.announce();
    }

    pub(super) fn publish(&self, update: BoardUpdate) {
        self.halted.store(update.halted, Ordering::SeqCst);
        *self.board.write().unwrap_or_else(PoisonError::into_inner) = Board {
            current: update.current,
            next: update.next,
            last: update.last,
        };
        self.hub.update(update.hub);
        self.announce();
    }

    /// `false` unless the runner has the lock, finished its evaluation and nothing is halted: a bet
    /// is only ever accepted when all three hold (RF29).
    pub fn accepts_bets(&self) -> bool {
        self.halted_state() == Some(false)
    }

    /// `null` without the lock or before the first evaluation, so it never says `false` without
    /// knowing (RF35).
    fn halted_state(&self) -> Option<bool> {
        (self.has_lock.load(Ordering::SeqCst) && self.initial_done.load(Ordering::SeqCst))
            .then(|| self.halted.load(Ordering::SeqCst))
    }

    /// The body of `GET /api/fights/current`.
    pub fn current(&self) -> FightsCurrent {
        let board = self.board.read().unwrap_or_else(PoisonError::into_inner);
        FightsCurrent {
            server_time: TimestampMs::from_ms(self.clock.now_ms()),
            halted: self.halted_state(),
            current: board.current.clone(),
            next: board.next.clone(),
            last: board.last.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_only_says_it_is_not_halted_when_it_knows() {
        let shared = FightsShared::new(FightClock::manual(5));
        let current = shared.current();
        assert_eq!(current.halted, None);
        assert!(current.current.is_none() && current.next.is_none() && current.last.is_none());
        assert_eq!(current.server_time.as_ms(), Some(5));
        assert!(!shared.accepts_bets());

        shared.lock_taken();
        assert_eq!(shared.current().halted, None);
        assert!(!shared.accepts_bets());
        shared.initial_evaluation_done();
        assert_eq!(shared.current().halted, Some(false));
        assert!(shared.accepts_bets());

        shared.publish(BoardUpdate {
            halted: true,
            ..BoardUpdate::default()
        });
        assert_eq!(shared.current().halted, Some(true));
        assert!(!shared.accepts_bets());
        shared.publish(BoardUpdate::default());
        assert!(shared.accepts_bets());

        // Losing the lock: unknown again, and the evaluation has to be done again.
        shared.lock_lost();
        assert_eq!(shared.current().halted, None);
        assert!(!shared.accepts_bets());
        shared.lock_taken();
        assert!(!shared.accepts_bets());
    }
}
