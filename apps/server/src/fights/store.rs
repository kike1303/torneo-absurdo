//! The only SQL of `fights` (spec, "Componentes"). Hours travel as milliseconds since the epoch:
//! read with `extract(epoch …)`, which is exact, and written as `epoch + n * 1 ms`. Every transition
//! is `UPDATE … WHERE id = $1 AND state = $previous`: with no row, someone else already did it, and
//! nothing happens (RF8).

use protocol::{CancelReason, EndReason, FightState, Side};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Row};

use super::sealed::ClosedWindow;
use super::state::FightRow;

macro_rules! columns {
    () => {
        "id, state, \
         (extract(epoch FROM opens_at) * 1000)::bigint, \
         (extract(epoch FROM closes_at) * 1000)::bigint, \
         (extract(epoch FROM live_at) * 1000)::bigint, \
         ruleset_version, spec_a, spec_b, snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, \
         time_limit_ms, max_duration_ms, matchup_round, roster_sha256, seed_commit, \
         (extract(epoch FROM opened_at) * 1000)::bigint, \
         (extract(epoch FROM closed_at) * 1000)::bigint, \
         (extract(epoch FROM live_started_at) * 1000)::bigint, \
         (extract(epoch FROM settled_at) * 1000)::bigint, \
         winner, end_reason, cancel_reason, void_reason, revealed_seed, log"
    };
}

fn decode(row: &PgRow) -> Result<FightRow, sqlx::Error> {
    let state: String = row.try_get(1)?;
    let state = FightState::parse(&state)
        .ok_or_else(|| sqlx::Error::Protocol("fights.state".to_owned()))?;
    Ok(FightRow {
        id: row.try_get(0)?,
        state,
        opens_at: row.try_get(2)?,
        closes_at: row.try_get(3)?,
        live_at: row.try_get(4)?,
        ruleset_version: row.try_get(5)?,
        spec_a: row.try_get(6)?,
        spec_b: row.try_get(7)?,
        snapshot_sha256: row.try_get(8)?,
        slug_a: row.try_get(9)?,
        slug_b: row.try_get(10)?,
        max_hp_a: row.try_get(11)?,
        max_hp_b: row.try_get(12)?,
        time_limit_ms: row.try_get(13)?,
        max_duration_ms: row.try_get(14)?,
        matchup_round: row.try_get(15)?,
        roster_sha256: row.try_get(16)?,
        seed_commit: row.try_get(17)?,
        opened_at: row.try_get(18)?,
        closed_at: row.try_get(19)?,
        live_started_at: row.try_get(20)?,
        settled_at: row.try_get(21)?,
        winner: row.try_get(22)?,
        end_reason: row.try_get(23)?,
        cancel_reason: row.try_get(24)?,
        void_reason: row.try_get(25)?,
        revealed_seed: row.try_get(26)?,
        log: row.try_get(27)?,
    })
}

/// `SET LOCAL lock_timeout` for this transaction (RF8).
pub async fn set_lock_timeout(conn: &mut PgConnection, ms: u64) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('lock_timeout', $1, true)")
        .bind(format!("{ms}ms"))
        .execute(conn)
        .await
        .map(|_| ())
}

/// A fight to announce (RF1): the row does not exist without these.
pub struct NewFight<'a> {
    pub opens_at: i64,
    pub closes_at: i64,
    pub live_at: i64,
    pub ruleset_version: i16,
    pub spec_a: &'a [u8],
    pub spec_b: &'a [u8],
    pub snapshot_sha256: &'a [u8],
    pub slug_a: &'a str,
    pub slug_b: &'a str,
    pub max_hp_a: i32,
    pub max_hp_b: i32,
    pub time_limit_ms: i32,
    pub max_duration_ms: i32,
    pub matchup_round: i32,
    pub roster_sha256: &'a [u8],
}

/// Announces a fight. `None` if its slot already has one (`UNIQUE (opens_at)`, RF12).
pub async fn insert(pool: &PgPool, fight: &NewFight<'_>) -> Result<Option<i32>, sqlx::Error> {
    let row = sqlx::query(
        "INSERT INTO fights (state, opens_at, closes_at, live_at, ruleset_version, spec_a, spec_b, \
         snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, time_limit_ms, max_duration_ms, \
         matchup_round, roster_sha256) \
         VALUES ('scheduled', \
         timestamptz 'epoch' + $1::bigint * interval '1 millisecond', \
         timestamptz 'epoch' + $2::bigint * interval '1 millisecond', \
         timestamptz 'epoch' + $3::bigint * interval '1 millisecond', \
         $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
         ON CONFLICT ON CONSTRAINT fights_opens_at_key DO NOTHING RETURNING id",
    )
    .bind(fight.opens_at)
    .bind(fight.closes_at)
    .bind(fight.live_at)
    .bind(fight.ruleset_version)
    .bind(fight.spec_a)
    .bind(fight.spec_b)
    .bind(fight.snapshot_sha256)
    .bind(fight.slug_a)
    .bind(fight.slug_b)
    .bind(fight.max_hp_a)
    .bind(fight.max_hp_b)
    .bind(fight.time_limit_ms)
    .bind(fight.max_duration_ms)
    .bind(fight.matchup_round)
    .bind(fight.roster_sha256)
    .fetch_optional(pool)
    .await?;
    row.map(|row| row.try_get(0)).transpose()
}

/// The fight with this id.
pub async fn by_id(pool: &PgPool, id: i32) -> Result<Option<FightRow>, sqlx::Error> {
    let row = sqlx::query(concat!("SELECT ", columns!(), " FROM fights WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(decode).transpose()
}

/// The unsettled fights, the oldest first.
pub async fn unsettled(pool: &PgPool) -> Result<Vec<FightRow>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM fights WHERE state <> 'settled' ORDER BY opens_at"
    ))
    .fetch_all(pool)
    .await?;
    rows.iter().map(decode).collect()
}

/// The last settled fight.
pub async fn last_settled(pool: &PgPool) -> Result<Option<FightRow>, sqlx::Error> {
    let row = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM fights WHERE state = 'settled' ORDER BY opens_at DESC LIMIT 1"
    ))
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(decode).transpose()
}

/// What choosing the next pair needs (RF20).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchupContext {
    /// `roster_sha256` of the last row, cancelled or not.
    pub last_roster: Option<Vec<u8>>,
    /// The current round: the highest of the fights that were not cancelled.
    pub round: i32,
    /// The ordered pairs of that round that were not cancelled.
    pub used: Vec<(String, String)>,
    /// The last four fights that were not cancelled, the most recent last.
    pub recent: Vec<(String, String)>,
}

pub async fn matchup_context(pool: &PgPool) -> Result<MatchupContext, sqlx::Error> {
    let last_roster =
        sqlx::query("SELECT roster_sha256 FROM fights ORDER BY opens_at DESC LIMIT 1")
            .fetch_optional(pool)
            .await?
            .map(|row| row.try_get(0))
            .transpose()?;
    let round: i32 = sqlx::query(
        "SELECT COALESCE(max(matchup_round), 0) FROM fights WHERE cancel_reason IS NULL",
    )
    .fetch_one(pool)
    .await?
    .try_get(0)?;
    let pair = |row: &PgRow| -> Result<(String, String), sqlx::Error> {
        Ok((row.try_get(0)?, row.try_get(1)?))
    };
    let used = sqlx::query(
        "SELECT slug_a, slug_b FROM fights WHERE cancel_reason IS NULL AND matchup_round = $1",
    )
    .bind(round)
    .fetch_all(pool)
    .await?
    .iter()
    .map(pair)
    .collect::<Result<_, _>>()?;
    let mut recent = sqlx::query(
        "SELECT slug_a, slug_b FROM fights WHERE cancel_reason IS NULL \
         ORDER BY opens_at DESC LIMIT 4",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(pair)
    .collect::<Result<Vec<_>, _>>()?;
    recent.reverse();
    Ok(MatchupContext {
        last_roster,
        round,
        used,
        recent,
    })
}

/// Locks a `scheduled` row for its opening. `false` if it no longer is.
pub async fn lock_scheduled(conn: &mut PgConnection, id: i32) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT 1 FROM fights WHERE id = $1 AND state = 'scheduled' FOR UPDATE")
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(row.is_some())
}

/// `scheduled` → `betting_open`, in the transaction of `sealed::open_window`.
pub(super) async fn mark_open(
    conn: &mut PgConnection,
    id: i32,
    commit: &[u8],
    at_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE fights SET state = 'betting_open', seed_commit = $2, \
         opened_at = timestamptz 'epoch' + $3::bigint * interval '1 millisecond' \
         WHERE id = $1 AND state = 'scheduled'",
    )
    .bind(id)
    .bind(commit)
    .bind(at_ms)
    .execute(conn)
    .await
    .map(|_| ())
}

/// `betting_open` → `betting_closed`, in its own transaction. The `UPDATE` waits for every
/// transaction holding the row `FOR SHARE`, so every bet that saw it open confirms first (RF29).
/// The proof it returns exists only after the `COMMIT` (RF17). `None` if someone else closed it.
pub async fn close(
    pool: &PgPool,
    id: i32,
    at_ms: i64,
    lock_timeout_ms: u64,
) -> Result<Option<ClosedWindow>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx, lock_timeout_ms).await?;
    let updated = sqlx::query(
        "UPDATE fights SET state = 'betting_closed', \
         closed_at = timestamptz 'epoch' + $2::bigint * interval '1 millisecond' \
         WHERE id = $1 AND state = 'betting_open' RETURNING id",
    )
    .bind(id)
    .bind(at_ms)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(updated.map(|_| ClosedWindow::confirmed(id)))
}

/// The proof that the window of fight `id` is closed, read from its row (RF17). `None` in any other
/// state, a settled one included: there is nothing left to compute there.
pub async fn read_closed_window(
    pool: &PgPool,
    id: i32,
) -> Result<Option<ClosedWindow>, sqlx::Error> {
    let row =
        sqlx::query("SELECT 1 FROM fights WHERE id = $1 AND state IN ('betting_closed', 'live')")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|_| ClosedWindow::confirmed(id)))
}

/// `betting_closed` → `live`. `false` if someone else did it.
pub async fn go_live(
    pool: &PgPool,
    id: i32,
    started_at_ms: i64,
    lock_timeout_ms: u64,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx, lock_timeout_ms).await?;
    let done = sqlx::query(
        "UPDATE fights SET state = 'live', \
         live_started_at = timestamptz 'epoch' + $2::bigint * interval '1 millisecond' \
         WHERE id = $1 AND state = 'betting_closed'",
    )
    .bind(id)
    .bind(started_at_ms)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(done == 1)
}

/// `scheduled` → `settled` with `cancelled`, in the caller's transaction: no seed ever existed
/// (RF19). `false` if the row is no longer `scheduled`.
pub async fn cancel(
    conn: &mut PgConnection,
    id: i32,
    reason: CancelReason,
    at_ms: i64,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE fights SET state = 'settled', cancel_reason = $2, \
         settled_at = timestamptz 'epoch' + $3::bigint * interval '1 millisecond' \
         WHERE id = $1 AND state = 'scheduled'",
    )
    .bind(id)
    .bind(reason.as_str())
    .bind(at_ms)
    .execute(conn)
    .await?
    .rows_affected();
    Ok(done == 1)
}

/// `live` → `settled` with `decided`, in the transaction of `sealed::settle_decided`. `false` if
/// the row is no longer `live`.
pub(super) async fn write_decided(
    conn: &mut PgConnection,
    id: i32,
    winner: Option<Side>,
    reason: EndReason,
    seed: &[u8],
    log: &[u8],
    at_ms: i64,
) -> Result<bool, sqlx::Error> {
    let winner = winner.map(|side| match side {
        Side::A => "a",
        Side::B => "b",
    });
    let reason = match reason {
        EndReason::Ko => "ko",
        EndReason::Decision => "decision",
        EndReason::Draw => "draw",
    };
    let done = sqlx::query(
        "UPDATE fights SET state = 'settled', winner = $2, end_reason = $3, revealed_seed = $4, \
         log = $5, settled_at = timestamptz 'epoch' + $6::bigint * interval '1 millisecond' \
         WHERE id = $1 AND state = 'live'",
    )
    .bind(id)
    .bind(winner)
    .bind(reason)
    .bind(seed)
    .bind(log)
    .bind(at_ms)
    .execute(conn)
    .await?
    .rows_affected();
    Ok(done == 1)
}

/// The share lock of RF29: no row once `closes_at` passed, so it never waits past the hour. Returns
/// its state and `closes_at`.
pub(super) async fn lock_for_bet(
    conn: &mut PgConnection,
    id: i32,
    now_ms: i64,
) -> Result<Option<(FightState, i64)>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT state, (extract(epoch FROM closes_at) * 1000)::bigint FROM fights \
         WHERE id = $1 AND closes_at > timestamptz 'epoch' + $2::bigint * interval '1 millisecond' \
         FOR SHARE",
    )
    .bind(id)
    .bind(now_ms)
    .fetch_optional(conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let state: String = row.try_get(0)?;
    let state = FightState::parse(&state)
        .ok_or_else(|| sqlx::Error::Protocol("fights.state".to_owned()))?;
    Ok(Some((state, row.try_get(1)?)))
}

/// The ids of the unsettled fights with `closes_at` in `[from, to)` (RF32).
pub(super) async fn unsettled_ids_in(
    pool: &PgPool,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query(
        "SELECT id FROM fights WHERE state <> 'settled' \
         AND closes_at >= timestamptz 'epoch' + $1::bigint * interval '1 millisecond' \
         AND closes_at < timestamptz 'epoch' + $2::bigint * interval '1 millisecond' \
         ORDER BY opens_at",
    )
    .bind(from_ms)
    .bind(to_ms)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| row.try_get(0))
    .collect()
}
