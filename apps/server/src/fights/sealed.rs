//! The only module that holds the seed and the log of a fight in the open (RF23). Everything else
//! gets opaque types: none has `Debug`, `Serialize`, `Clone` or `Display`, and no method gives a
//! future time, neither the duration, nor the `t` of the next event, nor how many are left. This
//! module has no `tracing` macro: nothing here can write a log line. `fights_source_guard.rs` keeps
//! the names of the engine out of the rest of the server (RF24).
//!
//! What must not compile (RF17, RF23):
//!
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedSeed;
//! fn debug<T: std::fmt::Debug>() {}
//! debug::<SealedSeed>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedSeed;
//! fn clone<T: Clone>() {}
//! clone::<SealedSeed>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedSeed;
//! fn display<T: std::fmt::Display>() {}
//! display::<SealedSeed>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedSeed;
//! fn serialize<T: serde::Serialize>() {}
//! serialize::<SealedSeed>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedLog;
//! fn clone<T: Clone>() {}
//! clone::<SealedLog>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedLog;
//! fn display<T: std::fmt::Display>() {}
//! display::<SealedLog>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedLog;
//! fn debug<T: std::fmt::Debug>() {}
//! debug::<SealedLog>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::SealedLog;
//! fn serialize<T: serde::Serialize>() {}
//! serialize::<SealedLog>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::FinalLog;
//! fn debug<T: std::fmt::Debug>() {}
//! debug::<FinalLog>();
//! ```
//! ```compile_fail,E0277
//! use server::fights::sealed::FinalLog;
//! fn serialize<T: serde::Serialize>() {}
//! serialize::<FinalLog>();
//! ```
//!
//! Using a seed after `compute` does not compile, because `compute` consumes it:
//!
//! ```compile_fail,E0382
//! use server::fights::sealed::{ClosedWindow, SealedLog, SealedSeed};
//! use server::fights::snapshot::FightSnapshot;
//! fn twice(seed: SealedSeed, snapshot: &FightSnapshot, window: &ClosedWindow) {
//!     let _ = SealedLog::compute(seed, snapshot, window);
//!     let _ = SealedLog::compute(seed, snapshot, window);
//! }
//! ```
//!
//! There is no way to ask a sealed log for its bytes, its duration or what comes next:
//!
//! ```compile_fail,E0599
//! fn bytes(log: &server::fights::sealed::SealedLog) { let _ = log.log_bytes(); }
//! ```
//! ```compile_fail,E0599
//! fn duration(log: &server::fights::sealed::SealedLog) { let _ = log.end_t(); }
//! ```
//! ```compile_fail,E0616
//! fn inner(log: &server::fights::sealed::SealedLog) { let _ = &log.log; }
//! ```
//! ```compile_fail,E0616
//! fn inner(seed: &server::fights::sealed::SealedSeed) { let _ = &seed.bytes; }
//! ```
//! ```compile_fail,E0616
//! fn inner(log: &server::fights::sealed::FinalLog) { let _ = &log.seed; }
//! ```
//!
//! A `ClosedWindow` cannot be made up, only obtained from the store:
//!
//! ```compile_fail,E0451
//! let _ = server::fights::sealed::ClosedWindow { id: 1 };
//! ```
//! ```compile_fail,E0624
//! let _ = server::fights::sealed::ClosedWindow::confirmed(1);
//! ```
//!
//! And the same shapes compile with what the API does give, so none of the above fails for an
//! unrelated reason:
//!
//! ```
//! use server::fights::sealed::{ClosedWindow, FinalLog, SealedLog, SealedSeed};
//! use server::fights::snapshot::FightSnapshot;
//! fn once(seed: SealedSeed, snapshot: &FightSnapshot, window: &ClosedWindow) {
//!     let _ = SealedLog::compute(seed, snapshot, window);
//! }
//! fn released(log: &mut SealedLog) { let _ = log.release_due(0); }
//! fn finish(log: SealedLog) { let _ = log.into_final(0); }
//! fn public(log: &FinalLog) { let _ = (log.id(), log.final_result(), log.settle_at()); }
//! fn send<T: Send>() {}
//! send::<SealedSeed>();
//! send::<SealedLog>();
//! send::<FinalLog>();
//! ```

use engine::{FightLog, Seed, canonical_json, simulate};
use protocol::{CancelReason, FightEvent, FightFinal};
use rand_core::{OsRng, TryRngCore};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

use super::SETTLE_DELAY_MS;
use super::snapshot::FightSnapshot;
use super::store;

/// The seed of a fight, read from its table and already checked against its commit.
pub struct SealedSeed {
    bytes: [u8; 32],
}

/// Why a seed cannot be read. RF16: both end in a safe stop, and neither carries a byte.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the secret is missing")]
    Missing,
    #[error("the secret does not match the commit")]
    Mismatch,
    #[error("the database failed")]
    Db(#[from] sqlx::Error),
}

impl SealedSeed {
    /// Reads the seed of fight `id` and compares `sha256(seed)` with `commit` (RF23).
    ///
    /// # Errors
    /// [`SecretError`] if the row is missing, is not the one of the commit, or the query fails.
    pub async fn load(
        conn: &mut PgConnection,
        id: i32,
        commit: &[u8],
    ) -> Result<Self, SecretError> {
        let row = sqlx::query("SELECT seed FROM fight_secrets WHERE fight_id = $1")
            .bind(id)
            .fetch_optional(conn)
            .await?
            .ok_or(SecretError::Missing)?;
        let seed: Vec<u8> = row.try_get(0)?;
        if Sha256::digest(&seed).as_slice() != commit {
            return Err(SecretError::Mismatch);
        }
        let bytes = seed.try_into().map_err(|_| SecretError::Mismatch)?;
        Ok(Self { bytes })
    }
}

/// The proof that the betting window of a fight is closed: it only comes from the `COMMIT` of the
/// close (`store::close`) or from reading a row in `betting_closed` or `live`
/// (`store::read_closed_window`). The log cannot be computed without it (RF17).
pub struct ClosedWindow {
    id: i32,
}

impl ClosedWindow {
    pub(super) const fn confirmed(id: i32) -> Self {
        Self { id }
    }

    pub const fn id(&self) -> i32 {
        self.id
    }
}

/// Why the log could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ComputeError {
    #[error("the engine rejected the fight")]
    Engine(&'static str),
    #[error("the log could not be encoded")]
    Encode,
    #[error("the log has no end")]
    NoEnd,
}

impl ComputeError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Engine(code) => code,
            Self::Encode => "encode_failed",
            Self::NoEnd => "log_without_end",
        }
    }
}

/// The log of a fight whose window is closed, in memory until `Settled` (RF17, RF23). It only gives
/// out what is already due.
pub struct SealedLog {
    id: i32,
    log: FightLog,
    canonical: Vec<u8>,
    seed: [u8; 32],
    released: usize,
    end_released: bool,
}

impl SealedLog {
    /// Runs the engine. Consumes the seed; on a failure it gives it back, so a failure does not
    /// lose it.
    ///
    /// # Errors
    /// The seed and the [`ComputeError`].
    pub fn compute(
        seed: SealedSeed,
        snapshot: &FightSnapshot,
        window: &ClosedWindow,
    ) -> Result<Self, (SealedSeed, ComputeError)> {
        let (a, b, ruleset) = snapshot.parts();
        let engine_seed = Seed::from_bytes(seed.bytes);
        let log = match simulate(&engine_seed, a, b, ruleset) {
            Ok(log) => log,
            Err(error) => return Err((seed, ComputeError::Engine(error.code()))),
        };
        let Ok(canonical) = canonical_json(&log) else {
            return Err((seed, ComputeError::Encode));
        };
        if log.outcome().is_none() {
            return Err((seed, ComputeError::NoEnd));
        }
        Ok(Self {
            id: window.id,
            log,
            canonical,
            seed: seed.bytes,
            released: 0,
            end_released: false,
        })
    }

    /// The events with `t <= elapsed_ms` not yet released. The engine writes them in order of `t`.
    pub fn release_due(&mut self, elapsed_ms: u32) -> Vec<FightEvent> {
        let due: Vec<FightEvent> = self
            .log
            .events
            .iter()
            .skip(self.released)
            .take_while(|event| event.t() <= elapsed_ms)
            .cloned()
            .collect();
        self.released = self.released.saturating_add(due.len());
        if due
            .iter()
            .any(|event| matches!(event, FightEvent::End { .. }))
        {
            self.end_released = true;
        }
        due
    }

    /// How many events were released: the index of the next batch, already public.
    pub const fn released_count(&self) -> usize {
        self.released
    }

    /// Whether `end` was already released. Says nothing before it happens.
    pub const fn end_released(&self) -> bool {
        self.end_released
    }

    /// The final, once `end` is out (RF18).
    ///
    /// # Errors
    /// [`NotFinished`] if `end` was not released yet.
    pub fn into_final(self, live_started_at_ms: i64) -> Result<FinalLog, NotFinished> {
        if !self.end_released {
            return Err(NotFinished);
        }
        let (winner, reason) = self.log.outcome().ok_or(NotFinished)?;
        let end_t = self
            .log
            .events
            .iter()
            .rev()
            .find(|event| matches!(event, FightEvent::End { .. }))
            .map(FightEvent::t)
            .ok_or(NotFinished)?;
        Ok(FinalLog {
            id: self.id,
            canonical: self.canonical,
            seed: self.seed,
            final_result: FightFinal::Decided { winner, reason },
            settle_at: live_started_at_ms
                .saturating_add(i64::from(end_t))
                .saturating_add(i64::from(SETTLE_DELAY_MS)),
        })
    }
}

/// `end` was not released yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the fight has not finished")]
pub struct NotFinished;

/// The log of a finished fight, with the bytes of the seed for the reveal, which no other type
/// gives out. It does not exist until `end` was released, and it is only written to the database
/// ([`settle_decided`]).
pub struct FinalLog {
    id: i32,
    canonical: Vec<u8>,
    seed: [u8; 32],
    final_result: FightFinal,
    settle_at: i64,
}

impl FinalLog {
    pub const fn id(&self) -> i32 {
        self.id
    }

    pub const fn final_result(&self) -> FightFinal {
        self.final_result
    }

    /// `live_started_at + end.t + settle_delay`, deducible from what is already public.
    pub const fn settle_at(&self) -> i64 {
        self.settle_at
    }
}

/// Opens the betting window of a `scheduled` fight, in the caller's transaction: a new seed from
/// `OsRng`, its row and `seed_commit = SHA-256(seed)` (RF3). `Ok(false)` if the row is no longer
/// `scheduled`.
///
/// # Errors
/// [`OpenError`], and the caller's transaction reverts.
pub async fn open_window(conn: &mut PgConnection, id: i32, at_ms: i64) -> Result<bool, OpenError> {
    if !store::lock_scheduled(&mut *conn, id).await? {
        return Ok(false);
    }
    let mut seed = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut seed)
        .map_err(|_| OpenError::OsRng)?;
    sqlx::query("INSERT INTO fight_secrets (fight_id, seed) VALUES ($1, $2)")
        .bind(id)
        .bind(seed.as_slice())
        .execute(&mut *conn)
        .await?;
    let commit = Sha256::digest(seed);
    store::mark_open(&mut *conn, id, commit.as_slice(), at_ms).await?;
    Ok(true)
}

/// Why a window did not open.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("the OS random source failed")]
    OsRng,
    #[error("the database failed")]
    Db(#[from] sqlx::Error),
}

/// The writes of `Settled` with a result, in the caller's transaction: the final, the seed and the
/// log, and the row of the secret goes (RF18). `Ok(false)` if the row is no longer `live`: someone
/// else settled it.
///
/// # Errors
/// The database error, and the caller's transaction reverts.
pub async fn settle_decided(
    conn: &mut PgConnection,
    log: &FinalLog,
    at_ms: i64,
) -> Result<bool, sqlx::Error> {
    let FightFinal::Decided { winner, reason } = log.final_result else {
        return Ok(false);
    };
    let written = store::write_decided(
        &mut *conn,
        log.id,
        winner,
        reason,
        &log.seed,
        &log.canonical,
        at_ms,
    )
    .await?;
    if !written {
        return Ok(false);
    }
    sqlx::query("DELETE FROM fight_secrets WHERE fight_id = $1")
        .bind(log.id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// The writes of a cancelled final, in the caller's transaction (RF19). A cancelled fight never has
/// a seed: if one was planted in its table, it goes too, since nobody committed to it. `Ok(false)` if
/// the row is no longer `scheduled`.
///
/// # Errors
/// The database error, and the caller's transaction reverts.
pub async fn cancel_fight(
    conn: &mut PgConnection,
    id: i32,
    reason: CancelReason,
    at_ms: i64,
) -> Result<bool, sqlx::Error> {
    if !store::cancel(&mut *conn, id, reason, at_ms).await? {
        return Ok(false);
    }
    sqlx::query("DELETE FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// The events of the log of a `settled` row (RF38, for the hub of phase B).
///
/// # Errors
/// [`NotFinished`] does not apply: `serde_json` failed on bytes that are not a log.
pub fn events_of_settled(log_bytes: &[u8]) -> Result<Vec<FightEvent>, BadLog> {
    serde_json::from_slice::<FightLog>(log_bytes)
        .map(|log| log.events)
        .map_err(|_| BadLog)
}

/// Bytes that are not a log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the stored bytes are not a log")]
pub struct BadLog;

#[cfg(test)]
mod tests {
    use engine::{CharacterSpec, Ruleset};
    use protocol::FightState;

    use super::*;
    use crate::fights::snapshot::{FightSnapshot, max_duration_ms, max_hp, snapshot_sha256};
    use crate::fights::state::FightRow;

    fn spec(name: &str) -> (CharacterSpec, Vec<u8>) {
        let text = match name {
            "striker" => include_str!("../../../../crates/engine/tests/golden/spec-striker.json"),
            _ => include_str!("../../../../crates/engine/tests/golden/spec-tank.json"),
        };
        let spec: CharacterSpec = serde_json::from_str(text).unwrap();
        let bytes = engine::canonical_spec_json(&spec).unwrap();
        (spec, bytes)
    }

    fn snapshot() -> FightSnapshot {
        let ruleset = Ruleset::for_version(1).unwrap();
        let (a, spec_a) = spec("striker");
        let (b, spec_b) = spec("tank");
        let row = FightRow {
            id: 1,
            state: FightState::BettingClosed,
            opens_at: 0,
            closes_at: 1,
            live_at: 2,
            ruleset_version: 1,
            snapshot_sha256: snapshot_sha256(1, &spec_a, &spec_b).to_vec(),
            spec_a,
            spec_b,
            slug_a: a.slug.clone(),
            slug_b: b.slug.clone(),
            max_hp_a: i32::from(max_hp(&ruleset, &a).unwrap()),
            max_hp_b: i32::from(max_hp(&ruleset, &b).unwrap()),
            time_limit_ms: i32::try_from(ruleset.time_limit_ms()).unwrap(),
            max_duration_ms: i32::try_from(max_duration_ms(&ruleset).unwrap()).unwrap(),
            matchup_round: 1,
            roster_sha256: vec![0; 32],
            seed_commit: None,
            opened_at: None,
            closed_at: None,
            live_started_at: None,
            settled_at: None,
            winner: None,
            end_reason: None,
            cancel_reason: None,
            void_reason: None,
            revealed_seed: None,
            log: None,
        };
        FightSnapshot::from_row(&row).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn sealed() -> SealedLog {
        SealedLog::compute(
            SealedSeed { bytes: [3; 32] },
            &snapshot(),
            &ClosedWindow::confirmed(1),
        )
        .unwrap_or_else(|(_, e)| panic!("{e:?}"))
    }

    fn expected() -> FightLog {
        let snapshot = snapshot();
        let (a, b, ruleset) = snapshot.parts();
        simulate(&Seed::from_bytes([3; 32]), a, b, ruleset).unwrap()
    }

    #[test]
    fn release_gives_each_event_once_and_only_when_due() {
        let events = expected().events;
        let end_t = events.last().unwrap().t();
        let mut log = sealed();
        let mut released = Vec::new();
        // In steps of 50 ms, as the task does.
        let mut elapsed = 0;
        while !log.end_released() {
            let batch = log.release_due(elapsed);
            for event in &batch {
                assert!(event.t() <= elapsed);
            }
            released.extend(batch);
            assert_eq!(log.released_count(), released.len());
            assert!(elapsed <= end_t + 50);
            elapsed += 50;
        }
        assert_eq!(released, events);
        assert!(log.release_due(u32::MAX).is_empty());
    }

    #[test]
    fn the_last_event_comes_exactly_at_its_time() {
        let events = expected().events;
        let end_t = events.last().unwrap().t();
        let mut log = sealed();
        let before = log.release_due(end_t - 1);
        assert!(!log.end_released());
        assert!(before.iter().all(|e| !matches!(e, FightEvent::End { .. })));
        let last = log.release_due(end_t);
        assert!(matches!(last.last(), Some(FightEvent::End { .. })));
        assert!(log.end_released());
    }

    #[test]
    fn the_final_only_exists_after_end() {
        let mut log = sealed();
        let _ = log.release_due(0);
        assert_eq!(log.into_final(0).err(), Some(NotFinished));

        let expected = expected();
        let end_t = expected.events.last().unwrap().t();
        let mut log = sealed();
        let _ = log.release_due(u32::MAX);
        let done = log.into_final(1000).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(done.id(), 1);
        assert_eq!(
            done.settle_at(),
            1000 + i64::from(end_t) + i64::from(SETTLE_DELAY_MS)
        );
        let (winner, reason) = expected.outcome().unwrap();
        assert_eq!(done.final_result(), FightFinal::Decided { winner, reason });
        assert_eq!(done.canonical, canonical_json(&expected).unwrap());
        assert_eq!(done.seed, [3; 32]);
    }

    #[test]
    fn the_events_of_a_settled_log_decode() {
        let expected = expected();
        let bytes = canonical_json(&expected).unwrap();
        assert_eq!(events_of_settled(&bytes).unwrap(), expected.events);
        assert_eq!(events_of_settled(b"{").err(), Some(BadLog));
    }

    /// The helper of `max_hp` says what the engine writes in the log (spec, "Componentes").
    #[test]
    fn max_hp_is_what_the_engine_writes() {
        let ruleset = Ruleset::for_version(1).unwrap();
        let log = expected();
        assert_eq!(max_hp(&ruleset, &spec("striker").0), Some(log.a.max_hp));
        assert_eq!(max_hp(&ruleset, &spec("tank").0), Some(log.b.max_hp));
    }
}
