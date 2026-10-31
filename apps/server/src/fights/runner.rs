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

use protocol::{CancelReason, FightState};
use rand_core::{OsRng, TryRngCore};
use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection, PgPool};
use tokio::task::{Id, JoinError, JoinSet};
use tokio_util::sync::CancellationToken;

use super::hook::{SettlementError, SettlingFight};
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
    /// The result of `FightSnapshot::from_row` of each row: its bytes do not change.
    snapshot_errors: BTreeMap<i32, Option<&'static str>>,
    /// Rows whose snapshot failed in a state with a commit, and when they were last logged.
    snapshot_logged: BTreeMap<i32, i64>,
    tasks: JoinSet<TaskOutput>,
    task_kinds: HashMap<Id, (i32, TaskKind)>,
    last: Option<FightRow>,
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
            snapshot_errors: BTreeMap::new(),
            snapshot_logged: BTreeMap::new(),
            tasks: JoinSet::new(),
            task_kinds: HashMap::new(),
            last: None,
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
            let pass =
                tokio::time::timeout(Duration::from_millis(timings.pass_timeout_ms), self.pass());
            let passed = tokio::select! {
                biased;
                _ = lock.join_next() => return,
                passed = pass => passed.unwrap_or(Err(sqlx::Error::PoolTimedOut)),
            };
            let (wake, retry) = match passed {
                Ok(wake) => {
                    self.shared.initial_evaluation_done();
                    db_backoff = timings.db_backoff_min_ms;
                    (wake, None)
                }
                Err(error) => {
                    let kind = error_kind(&error);
                    if kind == "lock_timeout" {
                        tracing::warn!(event = "fights.lock_timeout", error_kind = kind);
                    } else {
                        tracing::warn!(event = "fights.db_unavailable", error_kind = kind);
                    }
                    self.dirty = true;
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
            if let Some(deadline) = self.step(id, now).await? {
                wake = wake.min(deadline);
            }
        }
        self.spawn_attempts(now, &mut wake);

        // After the transitions: a fight of this hour that is cancelled does not leave a later row
        // ahead of it, so its pair is not lost (RF20). A cancellation runs in its own task, and its
        // pair counts as used until its `COMMIT`, so nothing is announced while one is pending; its
        // end wakes this loop. No rows during a halt (RF16).
        let cadence = self.config.cadence_ms;
        let target = target_slot(now, cadence);
        let cancelling = self
            .finals
            .values()
            .any(|settling| matches!(settling.kind, SettleKind::Cancelled(_)));
        if !self.halted() && !cancelling && !self.rows.values().any(|row| row.opens_at == target) {
            self.announce(now).await?;
        }
        wake = wake.min(target);
        if let Some(until_halt) = self.next_halt_check(now) {
            wake = wake.min(until_halt);
        }
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
            .map(|first| first.saturating_add(halt_after))
            .filter(|hour| *hour > now)
            .min()
    }

    async fn reload(&mut self, now: i64) -> Result<(), sqlx::Error> {
        let rows = store::unsettled(&self.pool).await?;
        self.last = store::last_settled(&self.pool).await?;
        self.rows = rows.into_iter().map(|row| (row.id, row)).collect();
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
        Ok(())
    }

    fn forget(&mut self, id: i32) {
        if let Some(emission) = self.emissions.remove(&id) {
            emission.abort();
        }
        self.sealed.remove(&id);
        self.finals.remove(&id);
        self.safe_stops.remove(&id);
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
                halted: self.halted(),
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
        let mut tx = self.pool.begin().await?;
        store::set_lock_timeout(&mut tx, self.settings.timings.lock_timeout_ms).await?;
        match sealed::open_window(&mut tx, id, now).await {
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
        self.refresh(id).await
    }

    async fn close(&mut self, id: i32, now: i64) -> Result<(), sqlx::Error> {
        let closed =
            store::close(&self.pool, id, now, self.settings.timings.lock_timeout_ms).await?;
        if closed.is_some() {
            tracing::debug!(event = "fights.closed");
        }
        // Only the update of the row: the log is computed in the next action, from the row.
        self.refresh(id).await
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
        let Some(log) = self.sealed.remove(&id) else {
            return;
        };
        let clock = self.settings.clock.clone();
        let sink = self.settings.sink.clone();
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
            Err(error) => return Ok(Some(self.safe_stop(id, error.code(), now))),
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
                    Ok(Attempted::Confirmed | Attempted::AlreadySettled) => {
                        tracing::debug!(event = "fights.settled");
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

    fn publish(&self) {
        let mut rows: Vec<&FightRow> = self.rows.values().collect();
        rows.sort_unstable_by_key(|row| row.opens_at);
        let mut summaries = rows.iter().filter_map(|row| row.summary());
        let current = summaries.next();
        let next = summaries.next();
        let last = self.last.as_ref().and_then(FightRow::summary);
        self.shared.publish(self.halted(), current, next, last);
    }
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
                store::cancel(&mut tx, self.id, *reason, self.at).await?,
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
