//! The runner (RF10, RF12 to RF18): one task, with one advisory lock, that takes every fight through
//! its states. Its deadlines are absolute, so a restart only needs the rows. Shutting down changes
//! no state: an open transaction reverts, and the next start goes on from the database.
//!
//! The release of each fight and each settlement attempt are tasks of a `JoinSet` of this loop. They
//! outlive a lost lock (amendment DP-4), and a panic of the loop aborts them with it, so a fight
//! never has two emissions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use protocol::{CancelReason, FightEvent, FightState};
use rand_core::{OsRng, TryRngCore};
use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection, PgPool};
use tokio::task::{Id, JoinError, JoinSet};
use tokio_util::sync::CancellationToken;

use super::board::BoardUpdate;
use super::hook::{SettlementError, SettlingFight};
use super::hub::{HubUpdate, SettledFight};
use super::roster::Roster;
use super::sealed::{self, FinalLog, SealedLog, SealedSeed, SecretError};
use super::snapshot::FightSnapshot;
use super::state::{Action, Context, FightRow, next_action, target_slot};
use super::{FightSettings, FightsConfig, FightsShared, release, store};
use crate::db::error_kind;

/// `application_name` of the lock connection (the tests find it in `pg_stat_activity`).
pub const APPLICATION_NAME: &str = "torneo-fight-runner";
/// The key of `pg_try_advisory_lock`: `TAFIGHT` in ASCII. Advisory locks are per database.
const LOCK_KEY: i64 = 0x0054_4146_4947_4854;
/// How long opening the lock connection may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The TCP settings of the lock session (R5), so a network partition frees the lock too. In seconds,
/// and milliseconds for the last one.
const TCP_KEEPALIVES_IDLE: u32 = 5;
const TCP_KEEPALIVES_INTERVAL: u32 = 2;
const TCP_KEEPALIVES_COUNT: u32 = 3;
const TCP_USER_TIMEOUT_MS: u32 = 15_000;

/// Starts the runner: only `app::serve` does, so the tests in process do not program fights unless
/// they ask (RF14). It ends when `stop` is cancelled. A panic of the loop is logged and the loop
/// starts over from the database.
pub async fn run(
    pool: PgPool,
    shared: Arc<FightsShared>,
    config: FightsConfig,
    settings: FightSettings,
    stop: CancellationToken,
) {
    let roster = Roster::load(&config.content_dir).await.map(Arc::new);
    let backoff = Duration::from_millis(settings.timings.panic_backoff_ms);
    loop {
        let runner = Runner::new(
            pool.clone(),
            shared.clone(),
            config.clone(),
            settings.clone(),
            roster.clone(),
        );
        let mut task = tokio::spawn(runner.run());
        let ended = tokio::select! {
            () = stop.cancelled() => None,
            ended = &mut task => Some(ended),
        };
        shared.lock_lost();
        match ended {
            None => {
                task.abort();
                let _ = task.await;
                return;
            }
            Some(Ok(())) => return,
            Some(Err(_)) => tracing::error!(event = "fights.runner_panic"),
        }
        tokio::select! {
            () = stop.cancelled() => return,
            () = tokio::time::sleep(backoff) => {}
        }
    }
}

/// What a task of the loop gives back.
enum TaskOutput {
    Released {
        id: i32,
        final_log: Option<FinalLog>,
    },
    Attempt {
        id: i32,
        result: Result<Attempted, &'static str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempted {
    Confirmed,
    /// The row was no longer in its state: someone else settled it (RF18, RF59).
    AlreadySettled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskKind {
    Release,
    Attempt,
}

#[derive(Clone)]
enum SettleKind {
    Decided(Arc<FinalLog>),
    Cancelled(CancelReason),
}

/// A final waiting for its transaction.
struct Settling {
    kind: SettleKind,
    closes_at: i64,
    due_at: i64,
    running: bool,
    first_failed_at: Option<i64>,
    backoff_ms: i64,
}

/// A row whose step failed with an error of the database.
struct RowFailure {
    since: i64,
    /// Its state then: reaching another one is what ends the failure.
    state: FightState,
}

/// A fight whose log cannot be rebuilt (RF16).
struct SafeStop {
    next_retry: i64,
}

struct Runner {
    pool: PgPool,
    shared: Arc<FightsShared>,
    config: FightsConfig,
    settings: FightSettings,
    roster: Option<Arc<Roster>>,
    rows: BTreeMap<i32, FightRow>,
    /// Rebuilt logs, waiting for `live`.
    sealed: BTreeMap<i32, SealedLog>,
    /// The fights being released. An entry is removed only when its task ended or was aborted.
    emissions: BTreeMap<i32, tokio::task::AbortHandle>,
    finals: BTreeMap<i32, Settling>,
    safe_stops: BTreeMap<i32, SafeStop>,
    /// The rows whose last step failed with an error of the database, and since when. They do not
    /// stop the other rows, and they are tried again at the backoff of the database.
    row_failures: BTreeMap<i32, RowFailure>,
    /// Since when the passes keep aborting, with the lock alive.
    passes_failing_since: Option<i64>,
    /// The result of `FightSnapshot::from_row` of each row, until the next reread: a row repaired
    /// by hand has to be seen (only with its guard off, but that is the way out of a corrupt one).
    snapshot_errors: BTreeMap<i32, Option<&'static str>>,
    /// Rows whose snapshot failed in a state with a commit, and when they were last logged.
    snapshot_logged: BTreeMap<i32, i64>,
    tasks: JoinSet<TaskOutput>,
    task_kinds: HashMap<Id, (i32, TaskKind)>,
    last: Option<FightRow>,
    /// The events of the log of `last`, decoded once per fight.
    last_events: Option<(i32, Option<Arc<[FightEvent]>>)>,
    /// What the last reread found of the orphans of the hub, until the next publication.
    orphan_finals: Vec<SettledFight>,
    gone: Vec<i32>,
    dirty: bool,
    next_reread: i64,
}

impl Runner {
    fn new(
        pool: PgPool,
        shared: Arc<FightsShared>,
        config: FightsConfig,
        settings: FightSettings,
        roster: Option<Arc<Roster>>,
    ) -> Self {
        Self {
            pool,
            shared,
            config,
            settings,
            roster,
            rows: BTreeMap::new(),
            sealed: BTreeMap::new(),
            emissions: BTreeMap::new(),
            finals: BTreeMap::new(),
            safe_stops: BTreeMap::new(),
            row_failures: BTreeMap::new(),
            passes_failing_since: None,
            snapshot_errors: BTreeMap::new(),
            snapshot_logged: BTreeMap::new(),
            tasks: JoinSet::new(),
            task_kinds: HashMap::new(),
            last: None,
            last_events: None,
            orphan_finals: Vec::new(),
            gone: Vec::new(),
            dirty: true,
            next_reread: i64::MIN,
        }
    }

    async fn run(mut self) {
        loop {
            let mut lock = self.acquire_lock().await;
            self.shared.lock_taken();
            // Everything may have changed while the lock was away (R6): only what the rows say
            // decides, and the emissions in memory stay.
            self.dirty = true;
            self.held(&mut lock).await;
            self.shared.lock_lost();
            tracing::warn!(event = "fights.lock_lost");
        }
    }

    fn now(&self) -> i64 {
        self.settings.clock.now_ms()
    }

    /// Without the lock nothing starts: no rows, no transitions, no settlements and no emissions.
    /// The emissions already running go on, and their results wait (amendment DP-4).
    async fn acquire_lock(&mut self) -> JoinSet<()> {
        let timings = self.settings.timings;
        loop {
            if let Some(connection) = try_lock(&self.pool, &self.settings).await {
                let mut holder = JoinSet::new();
                holder.spawn(heartbeat(
                    connection,
                    Duration::from_millis(timings.heartbeat_ms),
                    Duration::from_millis(timings.heartbeat_timeout_ms),
                ));
                return holder;
            }
            let wait = tokio::time::sleep(Duration::from_millis(timings.lock_retry_ms));
            tokio::pin!(wait);
            loop {
                tokio::select! {
                    () = &mut wait => break,
                    Some(done) = self.tasks.join_next_with_id(), if !self.tasks.is_empty() => {
                        self.on_task(done);
                    }
                }
            }
        }
    }

    /// The loop with the lock: a pass, then a wait for what comes first. It returns when the lock is
    /// lost.
    async fn held(&mut self, lock: &mut JoinSet<()>) {
        let timings = self.settings.timings;
        let mut db_backoff = timings.db_backoff_min_ms;
        loop {
            // A hung database must not hide a lost lock, and a pass cut short is safe: an open
            // transaction reverts, and the rows are read again.
            let started = self.now();
            let pass =
                tokio::time::timeout(Duration::from_millis(timings.pass_timeout_ms), self.pass());
            let passed = tokio::select! {
                biased;
                _ = lock.join_next() => return,
                passed = pass => passed.unwrap_or(Err(sqlx::Error::PoolTimedOut)),
            };
            let (wake, retry) = match passed {
                Ok(wake) => {
                    // The first evaluation is not over while a row with a commit is unevaluated,
                    // unless it already halted the fights, which is what it would say.
                    if !self.commit_row_failing() || self.halted() {
                        self.shared.initial_evaluation_done();
                    }
                    if self.row_failures.is_empty() {
                        db_backoff = timings.db_backoff_min_ms;
                        (wake, None)
                    } else {
                        let retry = Duration::from_millis(db_backoff);
                        db_backoff = db_backoff.saturating_mul(2).min(timings.db_backoff_max_ms);
                        (wake, Some(retry))
                    }
                }
                Err(error) => {
                    let kind = error_kind(&error);
                    if kind == "lock_timeout" {
                        tracing::warn!(event = "fights.lock_timeout", error_kind = kind);
                    } else {
                        tracing::warn!(event = "fights.db_unavailable", error_kind = kind);
                    }
                    self.dirty = true;
                    // A pass that aborts publishes nothing by itself. What it reaches is a halt, as
                    // a failing settlement does, and it says so from memory.
                    self.passes_failing_since.get_or_insert(started);
                    // In this order: nobody may read a finished evaluation with an old `halted`.
                    self.publish();
                    if self.halted() {
                        self.shared.initial_evaluation_done();
                    }
                    let retry = Duration::from_millis(db_backoff);
                    db_backoff = db_backoff.saturating_mul(2).min(timings.db_backoff_max_ms);
                    (i64::MAX, Some(retry))
                }
            };
            // A pass may have awaited nothing, and the clock may have moved past `wake`: never hold
            // the worker of the release tasks.
            tokio::task::yield_now().await;
            let clock = self.settings.clock.clone();
            let sleep = clock.sleep_until(wake);
            let retrying = tokio::time::sleep(retry.unwrap_or(Duration::MAX));
            tokio::select! {
                biased;
                _ = lock.join_next() => return,
                Some(done) = self.tasks.join_next_with_id(), if !self.tasks.is_empty() => {
                    self.on_task(done);
                }
                () = sleep => {}
                () = retrying, if retry.is_some() => {}
            }
        }
    }

    fn halted(&self) -> bool {
        self.halted_in_pass() || self.passes_have_been_failing()
    }

    /// What a pass that is running goes by: if it got this far, passes are not failing anymore.
    fn halted_in_pass(&self) -> bool {
        self.halted_by_a_fight() || self.halted_by_a_failing_row()
    }

    /// Every cause except the failures of the database.
    fn halted_by_a_fight(&self) -> bool {
        if !self.safe_stops.is_empty() {
            return true;
        }
        let halting = self.snapshot_errors.iter().any(|(id, error)| {
            error.is_some()
                && self
                    .rows
                    .get(id)
                    .is_some_and(|row| row.state != FightState::Scheduled)
        });
        if halting {
            return true;
        }
        let now = self.now();
        let halt_after = self.settings.timings.settle_halt_ms;
        self.finals.values().any(|settling| {
            settling
                .first_failed_at
                .is_some_and(|first| now.saturating_sub(first) >= halt_after)
        })
    }

    fn halted_by_a_failing_row(&self) -> bool {
        let now = self.now();
        let halt_after = self.settings.timings.settle_halt_ms;
        self.commit_row_failures()
            .any(|first| now.saturating_sub(first) >= halt_after)
    }

    fn passes_have_been_failing(&self) -> bool {
        let halt_after = self.settings.timings.settle_halt_ms;
        self.passes_failing_since
            .is_some_and(|first| self.now().saturating_sub(first) >= halt_after)
    }

    /// The halt a row has to respect. One that is failing ignores what failing rows cause, its own
    /// or another's: they have to keep trying, or the halt they cause would never end.
    fn halted_for(&self, id: i32) -> bool {
        if self.row_failures.contains_key(&id) {
            self.halted_by_a_fight()
        } else {
            self.halted_in_pass()
        }
    }

    /// Since when each failing row with a commit has been failing. One without a commit never halts
    /// anything: it has no seed nor bets, and it ends cancelled when it loses its window.
    fn commit_row_failures(&self) -> impl Iterator<Item = i64> + '_ {
        self.row_failures.iter().filter_map(|(id, failure)| {
            let row = self.rows.get(id)?;
            (row.state != FightState::Scheduled).then_some(failure.since)
        })
    }

    fn commit_row_failing(&self) -> bool {
        self.commit_row_failures().next().is_some()
    }

    /// One evaluation of every unsettled fight, the oldest first. Returns the next hour to look.
    async fn pass(&mut self) -> Result<i64, sqlx::Error> {
        self.settings.probe.before_pass();
        let now = self.now();
        if self.dirty || now >= self.next_reread {
            self.reload(now).await?;
        }
        self.check_snapshots(now);

        let mut order: Vec<(i64, i32)> = self.rows.values().map(|r| (r.opens_at, r.id)).collect();
        order.sort_unstable();
        let mut wake = self.next_reread;
        for (_, id) in order {
            match self.step(id, now).await {
                Ok(deadline) => {
                    self.row_recovered(id);
                    if let Some(deadline) = deadline {
                        wake = wake.min(deadline);
                    }
                }
                // Something this row does, not the connection: the rest of the rows go on.
                Err(error) if error.as_database_error().is_some() => {
                    self.row_failed(id, &error, now);
                }
                Err(error) => return Err(error),
            }
        }
        self.spawn_attempts(now, &mut wake);

        // After the transitions: a fight of this hour that is cancelled does not leave a later row
        // ahead of it, so its pair is not lost (RF20). A row past its hour and still scheduled is
        // being cancelled, in its own task, or cannot open and may still lose its window. Its pair
        // counts as used until then, so nothing is announced behind it, and the end of its
        // cancellation wakes this loop. No rows during a halt (RF16).
        let cadence = self.config.cadence_ms;
        let target = target_slot(now, cadence);
        let undecided = self
            .rows
            .values()
            .any(|row| row.state == FightState::Scheduled && row.opens_at <= now);
        if !self.halted_in_pass()
            && !undecided
            && !self.rows.values().any(|row| row.opens_at == target)
        {
            self.announce(now).await?;
        }
        wake = wake.min(target);
        if let Some(until_halt) = self.next_halt_check(now) {
            wake = wake.min(until_halt);
        }
        self.passes_failing_since = None;
        self.publish();
        Ok(wake)
    }

    /// The next hour a failing settlement becomes a halt. Only a future one: one already past is a
    /// halt, and waking for it would make this loop spin.
    fn next_halt_check(&self, now: i64) -> Option<i64> {
        let halt_after = self.settings.timings.settle_halt_ms;
        self.finals
            .values()
            .filter_map(|s| s.first_failed_at)
            .chain(self.commit_row_failures())
            .map(|first| first.saturating_add(halt_after))
            .filter(|hour| *hour > now)
            .min()
    }

    fn row_failed(&mut self, id: i32, error: &sqlx::Error, now: i64) {
        let kind = error_kind(error);
        if kind == "lock_timeout" {
            tracing::warn!(event = "fights.lock_timeout", error_kind = kind);
        } else {
            tracing::warn!(event = "fights.row_failed", error_kind = kind);
        }
        let state = self.rows.get(&id).map(|row| row.state);
        self.row_failures.entry(id).or_insert(RowFailure {
            since: now,
            state: state.unwrap_or(FightState::Scheduled),
        });
        self.dirty = true;
    }

    /// A step without an error ends the failure of a row, unless the row only sat out a halt: it
    /// tried nothing, so that says nothing. Reaching another state always does.
    fn row_recovered(&mut self, id: i32) {
        let Some(failure) = self.row_failures.get(&id) else {
            return;
        };
        let moved = self
            .rows
            .get(&id)
            .is_none_or(|row| row.state != failure.state);
        if moved || !self.halted_by_a_fight() {
            self.row_failures.remove(&id);
        }
    }

    async fn reload(&mut self, now: i64) -> Result<(), sqlx::Error> {
        let rows = store::unsettled(&self.pool).await?;
        self.last = store::last_settled(&self.pool).await?;
        self.rows = rows.into_iter().map(|row| (row.id, row)).collect();
        self.read_orphans().await?;
        self.dirty = false;
        self.next_reread = now.saturating_add(self.settings.timings.reread_ms);
        // What is no longer unsettled was settled by someone else (RF18, RF59): its emission closes.
        let gone: BTreeSet<i32> = self
            .emissions
            .keys()
            .chain(self.sealed.keys())
            .chain(self.finals.keys())
            .chain(self.safe_stops.keys())
            .chain(self.snapshot_errors.keys())
            .filter(|id| !self.rows.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            self.forget(id);
        }
        self.snapshot_errors.clear();
        Ok(())
    }

    /// Amendment DP-B8: what the hub still has as unsettled and is no longer in the rows. Its `settled`
    /// summary is the only thing that counts as its final. Anything else ends its entry, and a row that
    /// does not decode must not stop the live fights.
    async fn read_orphans(&mut self) -> Result<(), sqlx::Error> {
        let known: BTreeSet<i32> = self.rows.keys().copied().collect();
        let last = self.last.as_ref().map(|row| row.id);
        for id in self.shared.hub().orphans(&known, last) {
            match orphan_outcome(store::by_id(&self.pool, id).await)? {
                Orphan::Settled(fight) => self.orphan_finals.push(*fight),
                Orphan::Gone => self.gone.push(id),
                Orphan::Invalid => {
                    tracing::warn!(event = "fights.orphan_invalid", error_kind = "bad_row");
                    self.gone.push(id);
                }
            }
        }
        Ok(())
    }

    fn forget(&mut self, id: i32) {
        if let Some(emission) = self.emissions.remove(&id) {
            emission.abort();
        }
        self.sealed.remove(&id);
        self.finals.remove(&id);
        self.safe_stops.remove(&id);
        self.row_failures.remove(&id);
        self.snapshot_errors.remove(&id);
        self.snapshot_logged.remove(&id);
    }

    /// R8: a snapshot that fails halts the fights from `betting_open` on. It comes only from public
    /// data, so it says nothing of the result. A scheduled row is handled by its action.
    fn check_snapshots(&mut self, now: i64) {
        let reread = self.settings.timings.reread_ms;
        for (id, row) in &self.rows {
            let error = *self
                .snapshot_errors
                .entry(*id)
                .or_insert_with(|| FightSnapshot::from_row(row).err().map(|error| error.code()));
            if let Some(kind) = error
                && row.state != FightState::Scheduled
            {
                let due = self
                    .snapshot_logged
                    .get(id)
                    .is_none_or(|last| now.saturating_sub(*last) >= reread);
                if due {
                    tracing::error!(event = "fights.safe_stop", error_kind = kind);
                    self.snapshot_logged.insert(*id, now);
                }
            }
        }
    }

    /// Applies the actions of a row until one says to wait.
    async fn step(&mut self, id: i32, now: i64) -> Result<Option<i64>, sqlx::Error> {
        // More than the whole path of a row, as a guard.
        for _ in 0..8 {
            if self.finals.contains_key(&id) {
                return Ok(None);
            }
            let Some(row) = self.rows.get(&id) else {
                return Ok(None);
            };
            let context = Context {
                other_live: self
                    .rows
                    .values()
                    .any(|other| other.id != id && other.state == FightState::Live),
                halted: self.halted_for(id),
                log_ready: self.sealed.contains_key(&id) || self.emissions.contains_key(&id),
            };
            let closes_at = row.closes_at;
            match next_action(row, now, &context) {
                Action::Wait(deadline) => return Ok(Some(deadline)),
                Action::Idle => return Ok(None),
                Action::Open => self.open(id, now).await?,
                Action::Cancel(reason) => {
                    self.finals.insert(
                        id,
                        Settling {
                            kind: SettleKind::Cancelled(reason),
                            closes_at,
                            due_at: now,
                            running: false,
                            first_failed_at: None,
                            backoff_ms: self.settings.timings.settle_backoff_min_ms,
                        },
                    );
                    return Ok(None);
                }
                Action::Close => self.close(id, now).await?,
                Action::Recompute => {
                    if let Some(retry) = self.recompute(id, now).await? {
                        return Ok(Some(retry));
                    }
                }
                Action::GoLive { started_at_ms } => self.go_live(id, started_at_ms).await?,
            }
        }
        Ok(None)
    }

    async fn refresh(&mut self, id: i32) -> Result<(), sqlx::Error> {
        match store::by_id(&self.pool, id).await? {
            Some(row) if row.state != FightState::Settled => {
                self.rows.insert(id, row);
            }
            _ => {
                self.rows.remove(&id);
                self.forget(id);
                self.dirty = true;
            }
        }
        Ok(())
    }

    /// RF3: the seed, its row, the commit and the state in one transaction.
    async fn open(&mut self, id: i32, now: i64) -> Result<(), sqlx::Error> {
        #[cfg(feature = "dev-fights")]
        let scripted = self.scripted_seed(id).await?;
        let mut tx = self.pool.begin().await?;
        store::set_lock_timeout(&mut tx, self.settings.timings.lock_timeout_ms).await?;
        #[cfg(feature = "dev-fights")]
        let opened = match scripted {
            Some(seed) => sealed::open_window_with_seed(&mut tx, id, &seed, now).await,
            None => sealed::open_window(&mut tx, id, now).await,
        };
        #[cfg(not(feature = "dev-fights"))]
        let opened = sealed::open_window(&mut tx, id, now).await;
        match opened {
            Ok(_) => {
                tx.commit().await?;
                tracing::debug!(event = "fights.opened");
            }
            Err(sealed::OpenError::Db(error)) => return Err(error),
            Err(sealed::OpenError::OsRng) => {
                tx.rollback().await?;
                return Err(sqlx::Error::Protocol("os_rng".to_owned()));
            }
        }
        self.refresh(id).await?;
        self.publish();
        Ok(())
    }

    /// RF45: the seed of the row of this fight in the script, if there is one.
    #[cfg(feature = "dev-fights")]
    async fn scripted_seed(&self, id: i32) -> Result<Option<[u8; 32]>, sqlx::Error> {
        let (Some(script), Some(row)) = (&self.settings.script, self.rows.get(&id)) else {
            return Ok(None);
        };
        let rank = store::fight_rank(&self.pool, id).await?;
        Ok(script.seed_for(rank, &row.spec_a, &row.spec_b))
    }

    async fn close(&mut self, id: i32, now: i64) -> Result<(), sqlx::Error> {
        let closed =
            store::close(&self.pool, id, now, self.settings.timings.lock_timeout_ms).await?;
        if closed.is_some() {
            tracing::debug!(event = "fights.closed");
        }
        // Only the update of the row: the log is computed in the next action, from the row. What the
        // sockets see of the close goes first, so its hour does not depend on that computation.
        self.refresh(id).await?;
        self.publish();
        Ok(())
    }

    async fn go_live(&mut self, id: i32, started_at_ms: i64) -> Result<(), sqlx::Error> {
        let done = store::go_live(
            &self.pool,
            id,
            started_at_ms,
            self.settings.timings.lock_timeout_ms,
        )
        .await?;
        if done {
            tracing::debug!(event = "fights.live");
        }
        self.refresh(id).await?;
        self.publish();
        self.start_emission_if_ready(id);
        Ok(())
    }

    /// The release of a fight in `live` whose log is in memory.
    fn start_emission_if_ready(&mut self, id: i32) {
        let Some(row) = self.rows.get(&id) else {
            return;
        };
        if row.state != FightState::Live || self.emissions.contains_key(&id) {
            return;
        }
        let Some(started_at_ms) = row.live_started_at else {
            return;
        };
        if !self.sealed.contains_key(&id) {
            return;
        }
        // Only with the row in `live`: this also runs when a computation ends in `betting_closed`,
        // and nothing may go out at an hour that depends on it. The hub knows the fight before its
        // first tick, also when resuming with no transition.
        self.publish();
        let Some(log) = self.sealed.remove(&id) else {
            return;
        };
        let clock = self.settings.clock.clone();
        let sink: Arc<dyn release::EventSink> = Arc::new(release::HubAndSink {
            hub: self.shared.hub().clone(),
            sink: self.settings.sink.clone(),
        });
        let handle = self.tasks.spawn(async move {
            let final_log = release::run(id, log, started_at_ms, clock, sink).await;
            TaskOutput::Released { id, final_log }
        });
        self.task_kinds.insert(handle.id(), (id, TaskKind::Release));
        self.emissions.insert(id, handle);
    }

    /// RF17: rebuilds the log of a row in `betting_closed` or `live`. `Some(hour)` if it failed and
    /// is a safe stop, with the hour of the next try.
    async fn recompute(&mut self, id: i32, now: i64) -> Result<Option<i64>, sqlx::Error> {
        if let Some(stop) = self.safe_stops.get(&id)
            && now < stop.next_retry
        {
            return Ok(Some(stop.next_retry));
        }
        let Some(row) = self.rows.get(&id) else {
            return Ok(None);
        };
        let snapshot = match FightSnapshot::from_row(row) {
            Ok(snapshot) => snapshot,
            // `check_snapshots` logs this one, on its own timer.
            Err(_) => {
                let next_retry = now.saturating_add(self.settings.timings.reread_ms);
                self.safe_stops.insert(id, SafeStop { next_retry });
                return Ok(Some(next_retry));
            }
        };
        let Some(commit) = row.seed_commit.clone() else {
            return Ok(Some(self.safe_stop(id, "commit_missing", now)));
        };
        let Some(window) = store::read_closed_window(&self.pool, id).await? else {
            self.refresh(id).await?;
            return Ok(None);
        };
        self.settings.compute_gate.wait().await;
        let mut connection = self.pool.acquire().await?;
        let seed = match SealedSeed::load(&mut connection, id, &commit).await {
            Ok(seed) => seed,
            Err(SecretError::Missing) => {
                return Ok(Some(self.safe_stop(id, "secret_missing", now)));
            }
            Err(SecretError::Mismatch) => {
                return Ok(Some(self.safe_stop(id, "secret_mismatch", now)));
            }
            Err(SecretError::Db(error)) => return Err(error),
        };
        drop(connection);
        // Off the runtime, and a panic of the engine is one more failure to rebuild the log. The
        // seed is read again on the next try.
        let computed =
            tokio::task::spawn_blocking(move || SealedLog::compute(seed, &snapshot, &window)).await;
        match computed {
            Ok(Ok(log)) => {
                self.safe_stops.remove(&id);
                self.sealed.insert(id, log);
                self.start_emission_if_ready(id);
                Ok(None)
            }
            Ok(Err((_seed, error))) => Ok(Some(self.safe_stop(id, error.code(), now))),
            Err(_) => Ok(Some(self.safe_stop(id, "engine_panic", now))),
        }
    }

    /// Records a safe stop and logs it, once per try: every `reread` (RF16).
    fn safe_stop(&mut self, id: i32, kind: &'static str, now: i64) -> i64 {
        tracing::error!(event = "fights.safe_stop", error_kind = kind);
        let next_retry = now.saturating_add(self.settings.timings.reread_ms);
        self.safe_stops.insert(id, SafeStop { next_retry });
        next_retry
    }

    fn spawn_attempts(&mut self, now: i64, wake: &mut i64) {
        let ids: Vec<i32> = self.finals.keys().copied().collect();
        for id in ids {
            let Some(settling) = self.finals.get_mut(&id) else {
                continue;
            };
            if settling.running {
                continue;
            }
            if now < settling.due_at {
                *wake = (*wake).min(settling.due_at);
                continue;
            }
            settling.running = true;
            let attempt = Attempt {
                pool: self.pool.clone(),
                settings: self.settings.clone(),
                id,
                kind: settling.kind.clone(),
                closes_at: settling.closes_at,
                at: now,
            };
            let timeout = Duration::from_millis(self.settings.timings.attempt_timeout_ms);
            let handle = self.tasks.spawn(async move {
                let result = tokio::time::timeout(timeout, attempt.run())
                    .await
                    .unwrap_or(Err("attempt_timeout"));
                TaskOutput::Attempt { id, result }
            });
            self.task_kinds.insert(handle.id(), (id, TaskKind::Attempt));
        }
    }

    fn on_task(&mut self, done: Result<(Id, TaskOutput), JoinError>) {
        match done {
            Ok((task, TaskOutput::Released { id, final_log })) => {
                self.task_kinds.remove(&task);
                self.emissions.remove(&id);
                let Some(final_log) = final_log else {
                    return;
                };
                if let Some(row) = self.rows.get(&id) {
                    let due_at = final_log.settle_at();
                    let closes_at = row.closes_at;
                    self.finals.insert(
                        id,
                        Settling {
                            kind: SettleKind::Decided(Arc::new(final_log)),
                            closes_at,
                            due_at,
                            running: false,
                            first_failed_at: None,
                            backoff_ms: self.settings.timings.settle_backoff_min_ms,
                        },
                    );
                }
            }
            Ok((task, TaskOutput::Attempt { id, result })) => {
                self.task_kinds.remove(&task);
                match result {
                    Ok(attempted) => {
                        if attempted == Attempted::Confirmed {
                            tracing::debug!(event = "fights.settled");
                        }
                        self.rows.remove(&id);
                        self.forget(id);
                        self.dirty = true;
                    }
                    Err(kind) => self.attempt_failed(id, kind),
                }
            }
            Err(error) => {
                let Some((id, kind)) = self.task_kinds.remove(&error.id()) else {
                    return;
                };
                match kind {
                    // R7: the log died with its task. `log_ready` is false now, so the next pass
                    // rebuilds it, and the release goes on from what is already due.
                    TaskKind::Release => {
                        self.emissions.remove(&id);
                    }
                    TaskKind::Attempt => {
                        if error.is_panic() {
                            self.attempt_failed(id, "panic");
                        } else if let Some(settling) = self.finals.get_mut(&id) {
                            settling.running = false;
                        }
                    }
                }
            }
        }
    }

    /// RF18: one more failure. A short one does not halt the live fights.
    fn attempt_failed(&mut self, id: i32, kind: &'static str) {
        tracing::error!(event = "fights.settle_failed", error_kind = kind);
        let now = self.now();
        let max = self.settings.timings.settle_backoff_max_ms;
        if let Some(settling) = self.finals.get_mut(&id) {
            settling.running = false;
            settling.first_failed_at.get_or_insert(now);
            settling.due_at = now.saturating_add(settling.backoff_ms);
            settling.backoff_ms = settling.backoff_ms.saturating_mul(2).min(max);
        }
    }

    /// RF12, RF20, RF21: the row of the next slot, from the roster loaded at startup.
    async fn announce(&mut self, now: i64) -> Result<(), sqlx::Error> {
        #[cfg(feature = "dev-fights")]
        if let Some(script) = self.settings.script.clone() {
            script.announce(&self.pool, &self.config, now).await?;
            return self.reload_rows_only().await;
        }
        let Some(roster) = self.roster.clone() else {
            return Ok(());
        };
        let roll = OsRng
            .try_next_u32()
            .map_err(|_| sqlx::Error::Protocol("os_rng".to_owned()))?;
        roster.announce(&self.pool, &self.config, now, roll).await?;
        self.reload_rows_only().await
    }

    async fn reload_rows_only(&mut self) -> Result<(), sqlx::Error> {
        let rows = store::unsettled(&self.pool).await?;
        self.rows = rows.into_iter().map(|row| (row.id, row)).collect();
        Ok(())
    }

    fn publish(&mut self) {
        let mut rows: Vec<&FightRow> = self.rows.values().collect();
        rows.sort_unstable_by_key(|row| row.opens_at);
        let summaries: Vec<_> = rows.iter().filter_map(|row| row.summary()).collect();
        let last = self.last.as_ref().and_then(FightRow::summary);
        let events = self.events_of_last();
        let update = BoardUpdate {
            halted: self.halted(),
            current: summaries.first().cloned(),
            next: summaries.get(1).cloned(),
            last: last.clone(),
            hub: HubUpdate {
                rows: summaries,
                last: last.map(|summary| SettledFight { summary, events }),
                finals: std::mem::take(&mut self.orphan_finals),
                gone: std::mem::take(&mut self.gone),
            },
        };
        self.shared.publish(update);
    }

    /// RF38: after a restart, `last` only has its events in its log. They are decoded once per fight.
    fn events_of_last(&mut self) -> Option<Arc<[FightEvent]>> {
        let row = self.last.as_ref()?;
        if let Some((id, events)) = &self.last_events
            && *id == row.id
        {
            return events.clone();
        }
        let events = events_of_row(row);
        self.last_events = Some((row.id, events.clone()));
        events
    }
}

/// The events of a settled row. A log that does not decode leaves the summary without events.
fn events_of_row(row: &FightRow) -> Option<Arc<[FightEvent]>> {
    let bytes = row.log.as_deref()?;
    match sealed::events_of_settled(bytes) {
        Ok(events) => Some(events.into()),
        Err(_) => {
            tracing::warn!(event = "fights.last_log_invalid", error_kind = "bad_log");
            None
        }
    }
}

/// What the row of an orphan of the hub says.
enum Orphan {
    Settled(Box<SettledFight>),
    /// No row, or not settled: for instance, a restored backup. It goes without a word.
    Gone,
    Invalid,
}

/// Only a settled summary is a final, and only an error talking to the database stops the pass.
fn orphan_outcome(read: Result<Option<FightRow>, sqlx::Error>) -> Result<Orphan, sqlx::Error> {
    let row = match read {
        Ok(Some(row)) => row,
        Ok(None) => return Ok(Orphan::Gone),
        Err(
            sqlx::Error::Protocol(_)
            | sqlx::Error::ColumnDecode { .. }
            | sqlx::Error::ColumnNotFound(_)
            | sqlx::Error::ColumnIndexOutOfBounds { .. }
            | sqlx::Error::Decode(_),
        ) => return Ok(Orphan::Invalid),
        Err(error) => return Err(error),
    };
    if row.state != FightState::Settled {
        return Ok(Orphan::Gone);
    }
    Ok(row.summary().map_or(Orphan::Invalid, |summary| {
        let events = events_of_row(&row);
        Orphan::Settled(Box::new(SettledFight { summary, events }))
    }))
}

/// One transaction of a final, in its own task: a panic here is one more failure, and never kills
/// the runner in silence (RF18).
struct Attempt {
    pool: PgPool,
    settings: FightSettings,
    id: i32,
    kind: SettleKind,
    closes_at: i64,
    at: i64,
}

impl Attempt {
    async fn run(self) -> Result<Attempted, &'static str> {
        self.transaction().await.map_err(|error| error.kind)
    }

    async fn transaction(&self) -> Result<Attempted, SettlementError> {
        let mut tx = self.pool.begin().await?;
        store::set_lock_timeout(&mut tx, self.settings.timings.lock_timeout_ms).await?;
        let (written, final_result) = match &self.kind {
            SettleKind::Decided(final_log) => (
                sealed::settle_decided(&mut tx, final_log, self.at).await?,
                final_log.final_result(),
            ),
            SettleKind::Cancelled(reason) => (
                sealed::cancel_fight(&mut tx, self.id, *reason, self.at).await?,
                protocol::FightFinal::Cancelled { reason: *reason },
            ),
        };
        if !written {
            return Ok(Attempted::AlreadySettled);
        }
        let fight = SettlingFight {
            id: self.id,
            final_result,
            closes_at: self.closes_at,
        };
        self.settings.hook.settle(&mut tx, &fight).await?;
        tx.commit().await?;
        Ok(Attempted::Confirmed)
    }
}

/// The options of the lock session. `idle_session_timeout` is what bounds a hung client whose
/// connection stays up (R5): the heartbeat runs well under it, and without one the server closes the
/// session, and with it the lock. The TCP settings cover a partition.
fn lock_options(pool: &PgPool, settings: &FightSettings) -> PgConnectOptions {
    (*pool.connect_options())
        .clone()
        .application_name(APPLICATION_NAME)
        .options([
            (
                "idle_session_timeout",
                settings.timings.idle_session_ms.to_string(),
            ),
            ("tcp_keepalives_idle", TCP_KEEPALIVES_IDLE.to_string()),
            (
                "tcp_keepalives_interval",
                TCP_KEEPALIVES_INTERVAL.to_string(),
            ),
            ("tcp_keepalives_count", TCP_KEEPALIVES_COUNT.to_string()),
            ("tcp_user_timeout", TCP_USER_TIMEOUT_MS.to_string()),
        ])
}

/// One connection of its own, kept for as long as the lock lasts: never a pool, whose `acquire`
/// after a failure would open another connection, and a `SELECT 1` would pass without the lock.
async fn try_lock(pool: &PgPool, settings: &FightSettings) -> Option<PgConnection> {
    let options = lock_options(pool, settings);
    let timeout = Duration::from_millis(settings.timings.heartbeat_timeout_ms);
    let connecting = tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(&options));
    let mut connection = match connecting.await {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => {
            tracing::warn!(
                event = "fights.db_unavailable",
                error_kind = error_kind(&error)
            );
            return None;
        }
        Err(_) => {
            tracing::warn!(event = "fights.db_unavailable", error_kind = "timeout");
            return None;
        }
    };
    let asked = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&mut connection);
    match tokio::time::timeout(timeout, asked).await {
        Ok(Ok(true)) => Some(connection),
        Ok(Ok(false)) => None,
        Ok(Err(error)) => {
            tracing::warn!(
                event = "fights.db_unavailable",
                error_kind = error_kind(&error)
            );
            None
        }
        Err(_) => {
            tracing::warn!(event = "fights.db_unavailable", error_kind = "timeout");
            None
        }
    }
}

/// Holds the connection and proves it alive. It ends on the first failure or timeout, and that is
/// what the loop takes as a lost lock. Its own task, so a slow pass never delays it past the idle
/// timeout of the session.
async fn heartbeat(mut connection: PgConnection, every: Duration, timeout: Duration) {
    loop {
        tokio::time::sleep(every).await;
        let proof = tokio::time::timeout(timeout, connection.ping()).await;
        if !matches!(proof, Ok(Ok(()))) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::tests::row;
    use super::*;

    /// Amendment DP-B8: only a settled summary is a final.
    #[test]
    fn only_a_settled_row_is_the_final_of_an_orphan() {
        let invalid = orphan_outcome(Err(sqlx::Error::Protocol("state".to_owned())));
        assert!(matches!(invalid, Ok(Orphan::Invalid)));
        assert!(matches!(orphan_outcome(Ok(None)), Ok(Orphan::Gone)));
        // A database that is down is another thing: the pass aborts, and it is read again.
        assert!(orphan_outcome(Err(sqlx::Error::PoolTimedOut)).is_err());
        // Another state, as after restoring a backup.
        for state in [FightState::Scheduled, FightState::Live] {
            assert!(matches!(
                orphan_outcome(Ok(Some(row(state)))),
                Ok(Orphan::Gone)
            ));
        }
        // Settled with no final that the protocol can say: as a row that does not decode.
        assert!(matches!(
            orphan_outcome(Ok(Some(row(FightState::Settled)))),
            Ok(Orphan::Invalid)
        ));
        let mut settled = row(FightState::Settled);
        settled.cancel_reason = Some("missed_window".to_owned());
        assert!(matches!(
            orphan_outcome(Ok(Some(settled))),
            Ok(Orphan::Settled(fight)) if fight.events.is_none()
        ));
    }
}
