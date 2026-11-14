//! The PvP seasons (021 "Temporadas PvP", phase 4, TA-64): the calendar month in UTC of
//! `pvp_fights.season_id`, the close of each one with its soft reset, and the public leaderboard.
//!
//! **The close.** [`close_due`] runs in the jobs (at startup and every hour): each month before the
//! current one with a rated fight and no row in `pvp_seasons` is closed in one transaction, the
//! oldest first. It locks every botarga (in the order of `user_id`), so no fight commits in the
//! middle; a fight waiting for one of those rows ends in its own `lock_timeout` (or, if it began in
//! the month being closed, is refused by `pvp_fights_guard`) and the client retries. Then it writes the rating of the season of each user who played it, and the soft reset
//! of every rating. A close can come late (a server that was down): the ratings already carry the
//! rated fights of the months after, so the snapshot is `rating − Σ later deltas` and the reset keeps
//! those deltas on top. A month without a rated fight has nothing to rank, has no row and compresses
//! nothing. The row is born closed, so a second pass finds nothing to do.
//!
//! **The leaderboard.** The current month is read live: the rating of `botargas` and the rated fights
//! of the month in `pvp_fights`. That read grows with the month and is public, so [`BoardCache`]
//! keeps it [`LIVE_TTL`] and computes it once at a time: the database sees at most one per interval,
//! however many ask (security review of TA-64). A closed month is read from its snapshot. A month that has ended
//! and is not closed yet (until the next pass of the job) is not a season. Eligibility: at least
//! [`SEASON_MIN_FIGHTS`] rated fights in the season, not banned and not excluded. A rated fight needs
//! both sides out of the PvP probation, and nobody goes back into it, so this is already "out of
//! probation" (021 "Lectura").

use std::time::{Duration, Instant};

use protocol::{PvpLeaderboardEntry, SeasonView, TimestampMs};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Row};
use tokio::sync::Mutex;

use super::rewards::{RATING_MAX, RATING_MIN, RATING_START};
pub use crate::seasons::SEASON_MIN_FIGHTS;

/// The entries of a leaderboard.
pub const LEADERBOARD_SIZE: i64 = 50;
/// The closed seasons `GET /api/pvp/seasons` lists: ten years of months.
pub const SEASONS_LISTED: i64 = 120;
/// How long the live leaderboard is served from [`BoardCache`].
pub const LIVE_TTL: Duration = Duration::from_secs(30);
/// The key of the advisory lock that makes two closes (two processes in a deploy) take turns.
const CLOSE_LOCK: i64 = 0x7076_705f_636c_6f73; // "pvp_clos"

/// The `YYYYMM` of this moment in UTC.
macro_rules! current_month_sql {
    () => {
        "(extract(year FROM now() AT TIME ZONE 'UTC') * 100 \
          + extract(month FROM now() AT TIME ZONE 'UTC'))::int"
    };
}

/// The oldest month before the current one with a rated fight and no row yet, or NULL.
const DUE_SQL: &str = concat!(
    "SELECT min(season_id) FROM pvp_fights \
     WHERE rated AND season_id > COALESCE((SELECT max(id) FROM pvp_seasons), 0) \
       AND season_id < ",
    current_month_sql!()
);

const INSERT_SEASON_SQL: &str = "\
INSERT INTO pvp_seasons (id, starts_at, ends_at) \
SELECT $1, m AT TIME ZONE 'UTC', (m + interval '1 month') AT TIME ZONE 'UTC' \
FROM (SELECT make_timestamp($1 / 100, $1 % 100, 1, 0, 0, 0) AS m) AS month";

/// What each user moved in the rated fights after season `$1`: the attacker `+delta`, the defender
/// `−delta`.
macro_rules! later_cte {
    () => {
        "later AS (SELECT user_id, sum(d)::int AS d FROM ( \
           SELECT attacker_id AS user_id, rating_delta::int AS d FROM pvp_fights \
             WHERE rated AND season_id > $1 \
           UNION ALL \
           SELECT defender_id, defender_rating_delta::int FROM pvp_fights \
             WHERE rated AND season_id > $1) AS m GROUP BY user_id)"
    };
}

/// The rated fights, victories and defeats of each side in season `$1`.
macro_rules! played_cte {
    () => {
        "moves AS ( \
           SELECT attacker_id AS user_id, winner = 'a' AS won, winner = 'b' AS lost \
             FROM pvp_fights WHERE season_id = $1 AND rated \
           UNION ALL \
           SELECT defender_id, winner = 'b', winner = 'a' \
             FROM pvp_fights WHERE season_id = $1 AND rated), \
         played AS (SELECT user_id, count(*)::int AS n, \
             count(*) FILTER (WHERE won)::int AS w, count(*) FILTER (WHERE lost)::int AS l \
           FROM moves GROUP BY user_id)"
    };
}

const SNAPSHOT_SQL: &str = concat!(
    "WITH ",
    later_cte!(),
    ", ",
    played_cte!(),
    " INSERT INTO pvp_season_ratings (season_id, user_id, rating, rated_fights, wins, losses) \
     SELECT $1, p.user_id, LEAST($3, GREATEST($2, b.rating - COALESCE(l.d, 0))), p.n, p.w, p.l \
     FROM played p JOIN botargas b ON b.user_id = p.user_id \
     LEFT JOIN later l ON l.user_id = p.user_id"
);

/// `rating' = 1000 + (snapshot − 1000) / 2`, with the division of Postgres that truncates toward
/// zero as [`super::rewards::soft_reset`], and the later deltas on top.
const RESET_SQL: &str = concat!(
    "WITH ",
    later_cte!(),
    ", reset AS (SELECT b.user_id, LEAST($3, GREATEST($2, \
         $4 + (LEAST($3, GREATEST($2, b.rating - COALESCE(l.d, 0))) - $4) / 2 \
         + COALESCE(l.d, 0))) AS rating \
       FROM botargas b LEFT JOIN later l ON l.user_id = b.user_id) \
     UPDATE botargas b SET rating = reset.rating FROM reset \
     WHERE reset.user_id = b.user_id AND b.rating <> reset.rating"
);

/// Closes every PvP season that is due, the oldest first, each in its own transaction, and returns
/// how many it closed. Safe at any time and as often as wanted.
///
/// # Errors
/// A database error; the seasons closed before it stay closed.
pub async fn close_due(pool: &PgPool) -> Result<u32, sqlx::Error> {
    let mut closed = 0_u32;
    while close_one(pool).await? {
        closed += 1;
    }
    Ok(closed)
}

async fn close_one(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // How long it waits for the rows of the botargas before it leaves the month to the next pass.
    sqlx::query("SET LOCAL lock_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(CLOSE_LOCK)
        .execute(&mut *tx)
        .await?;
    let due: Option<i32> = sqlx::query_scalar(DUE_SQL).fetch_one(&mut *tx).await?;
    let Some(season) = due else {
        return Ok(false);
    };
    // Every row, in one order: a fight holds at most two, and takes the second `SKIP LOCKED`.
    sqlx::query("SELECT 1 FROM botargas ORDER BY user_id FOR UPDATE")
        .execute(&mut *tx)
        .await?;
    // From here on, each statement sees every fight that committed while this waited.
    sqlx::query(INSERT_SEASON_SQL)
        .bind(season)
        .execute(&mut *tx)
        .await?;
    sqlx::query(SNAPSHOT_SQL)
        .bind(season)
        .bind(RATING_MIN)
        .bind(RATING_MAX)
        .execute(&mut *tx)
        .await?;
    sqlx::query(RESET_SQL)
        .bind(season)
        .bind(RATING_MIN)
        .bind(RATING_MAX)
        .bind(RATING_START)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(event = "pvp.season_closed", season_id = season);
    Ok(true)
}

/// [`close_due`], logging a failure by its code and nothing else: it never stops the server.
pub async fn close_due_logged(pool: &PgPool) {
    if let Err(error) = close_due(pool).await {
        tracing::warn!(
            event = "pvp.season_close_failed",
            error_kind = crate::db::error_kind(&error)
        );
    }
}

fn protocol_error(what: &str) -> sqlx::Error {
    sqlx::Error::Protocol(what.to_owned())
}

/// The current month, as a season that is never closed.
const CURRENT_SQL: &str = "\
SELECT (extract(year FROM m) * 100 + extract(month FROM m))::int, \
       (extract(epoch FROM m AT TIME ZONE 'UTC') * 1000)::bigint, \
       (extract(epoch FROM (m + interval '1 month') AT TIME ZONE 'UTC') * 1000)::bigint, false \
FROM (SELECT date_trunc('month', now() AT TIME ZONE 'UTC') AS m) AS month";

macro_rules! closed_columns {
    () => {
        "id, (extract(epoch FROM starts_at) * 1000)::bigint, \
         (extract(epoch FROM ends_at) * 1000)::bigint, true"
    };
}

fn season_of(row: &PgRow) -> Result<SeasonView, sqlx::Error> {
    Ok(SeasonView {
        id: u32::try_from(row.try_get::<i32, _>(0)?).map_err(|_| protocol_error("season"))?,
        starts_at: TimestampMs::from_ms(row.try_get(1)?),
        ends_at: TimestampMs::from_ms(row.try_get(2)?),
        closed: row.try_get(3)?,
    })
}

/// The season of this moment.
///
/// # Errors
/// A database error.
pub async fn current(conn: &mut PgConnection) -> Result<SeasonView, sqlx::Error> {
    let row = sqlx::query(CURRENT_SQL).fetch_one(conn).await?;
    season_of(&row)
}

/// A closed season by its id, `None` if it is not one.
///
/// # Errors
/// A database error.
pub async fn closed(conn: &mut PgConnection, id: i32) -> Result<Option<SeasonView>, sqlx::Error> {
    let sql = concat!(
        "SELECT ",
        closed_columns!(),
        " FROM pvp_seasons WHERE id = $1"
    );
    let row = sqlx::query(sql).bind(id).fetch_optional(conn).await?;
    row.as_ref().map(season_of).transpose()
}

/// The current season and the closed ones, the newest first.
///
/// # Errors
/// A database error.
pub async fn seasons(conn: &mut PgConnection) -> Result<Vec<SeasonView>, sqlx::Error> {
    let mut list = vec![current(&mut *conn).await?];
    let sql = concat!(
        "SELECT ",
        closed_columns!(),
        " FROM pvp_seasons ORDER BY id DESC LIMIT $1"
    );
    let rows = sqlx::query(sql)
        .bind(SEASONS_LISTED)
        .fetch_all(conn)
        .await?;
    for row in &rows {
        list.push(season_of(row)?);
    }
    Ok(list)
}

/// The live board of season `$1`: `$2` the minimum of rated fights, `$3` the size.
const LIVE_SQL: &str = concat!(
    "WITH ",
    played_cte!(),
    " SELECT u.display_name, b.rating, p.n, p.w, p.l \
     FROM played p JOIN botargas b ON b.user_id = p.user_id JOIN users u ON u.id = p.user_id \
     WHERE p.n >= $2 AND u.banned_at IS NULL \
       AND NOT EXISTS (SELECT 1 FROM pvp_exclusions x \
                       WHERE x.season_id = $1 AND x.user_id = p.user_id) \
     ORDER BY b.rating DESC, p.n DESC, p.user_id LIMIT $3"
);

/// The board of the closed season `$1`, from its snapshot.
const CLOSED_SQL: &str = "\
SELECT u.display_name, r.rating, r.rated_fights, r.wins, r.losses \
FROM pvp_season_ratings r JOIN users u ON u.id = r.user_id \
WHERE r.season_id = $1 AND r.rated_fights >= $2 AND u.banned_at IS NULL \
  AND NOT EXISTS (SELECT 1 FROM pvp_exclusions x \
                  WHERE x.season_id = $1 AND x.user_id = r.user_id) \
ORDER BY r.rating DESC, r.rated_fights DESC, r.user_id LIMIT $3";

/// The leaderboard of `season`: live when it is the current one, from its snapshot when closed.
///
/// # Errors
/// A database error, or a row the `CHECK`s of the tables forbid.
pub async fn entries(
    conn: &mut PgConnection,
    season: &SeasonView,
) -> Result<Vec<PvpLeaderboardEntry>, sqlx::Error> {
    let id = i32::try_from(season.id).map_err(|_| protocol_error("season"))?;
    let sql = if season.closed { CLOSED_SQL } else { LIVE_SQL };
    let rows = sqlx::query(sql)
        .bind(id)
        .bind(SEASON_MIN_FIGHTS)
        .bind(LEADERBOARD_SIZE)
        .fetch_all(conn)
        .await?;
    let count = |row: &PgRow, at: usize| {
        u32::try_from(row.try_get::<i32, _>(at)?).map_err(|_| protocol_error("count"))
    };
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            Ok(PvpLeaderboardEntry {
                rank: u16::try_from(i + 1).map_err(|_| protocol_error("rank"))?,
                display_name: row.try_get(0)?,
                rating: u16::try_from(row.try_get::<i32, _>(1)?)
                    .map_err(|_| protocol_error("rating"))?,
                rated_fights: count(row, 2)?,
                wins: count(row, 3)?,
                losses: count(row, 4)?,
            })
        })
        .collect()
}

/// The live leaderboard, computed at most once per [`LIVE_TTL`]. The lock is held while it computes:
/// a burst of readers waits on it, not on connections of the pool, and finds the fresh board.
#[derive(Debug, Default)]
pub struct BoardCache {
    slot: Mutex<Option<Live>>,
}

#[derive(Debug, Clone)]
struct Live {
    at: Instant,
    season: SeasonView,
    entries: Vec<PvpLeaderboardEntry>,
}

impl BoardCache {
    /// The current season and its board: the cached ones while fresh, or read now from `pool`.
    ///
    /// # Errors
    /// A database error; nothing is cached then.
    pub async fn live(
        &self,
        pool: &PgPool,
    ) -> Result<(SeasonView, Vec<PvpLeaderboardEntry>), sqlx::Error> {
        let mut slot = self.slot.lock().await;
        if let Some(live) = slot.as_ref()
            && live.at.elapsed() < LIVE_TTL
        {
            return Ok((live.season.clone(), live.entries.clone()));
        }
        let mut conn = pool.acquire().await?;
        let season = current(&mut conn).await?;
        let entries = entries(&mut conn, &season).await?;
        *slot = Some(Live {
            at: Instant::now(),
            season: season.clone(),
            entries: entries.clone(),
        });
        Ok((season, entries))
    }
}
