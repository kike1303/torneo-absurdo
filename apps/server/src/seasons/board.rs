//! What the leaderboard reads (020): the seasons and the eligible users of one, in its order.
//! Eligibility is decided at `T = min(now, ends_at)` by `bets::probation::of`, the only function that
//! decides probation, with the age and the fights of each candidate at `T` (ADR-0008 D8, test 27).

use protocol::{CoinAmount, LeaderboardEntry, SeasonView, TimestampMs};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::SEASON_MIN_FIGHTS;
use crate::bets::probation;

/// The entries of a leaderboard.
pub const LEADERBOARD_SIZE: usize = 50;
/// The seasons `GET /api/seasons` lists: ten years of months.
pub const SEASONS_LISTED: i64 = 120;
/// Candidates read at a time, before the filter of probation.
const CANDIDATE_BATCH: i64 = 100;
/// The most batches one read takes: the worst case stays constant, even with many accounts of high
/// profit still in probation (020, security review of #122).
const MAX_BATCHES: i64 = 20;

macro_rules! season_columns {
    () => {
        "id, (extract(epoch FROM starts_at) * 1000)::bigint, \
         (extract(epoch FROM ends_at) * 1000)::bigint, closed_at IS NOT NULL"
    };
}

/// The season of this moment, whether it has a row yet or not: its row is born with its first
/// settled fight.
const CURRENT_SQL: &str = "\
SELECT (extract(year FROM m) * 100 + extract(month FROM m))::int, \
       (extract(epoch FROM m AT TIME ZONE 'UTC') * 1000)::bigint, \
       (extract(epoch FROM (m + interval '1 month') AT TIME ZONE 'UTC') * 1000)::bigint, \
       COALESCE((SELECT closed_at IS NOT NULL FROM seasons \
                 WHERE id = (extract(year FROM m) * 100 + extract(month FROM m))::int), false) \
FROM (SELECT date_trunc('month', now() AT TIME ZONE 'UTC') AS m) AS month";

/// The candidates of one season, by the index of the board: neither banned nor excluded, with the
/// minimum of fights, and their age and fights at `T` for probation.
const CANDIDATES_SQL: &str = "\
SELECT u.display_name, st.ranked_profit, st.settled_fights, \
       (extract(epoch FROM cut.t - u.created_at) * 1000)::bigint, \
       (SELECT count(DISTINCT b.fight_id) FROM bet_settlements b \
          JOIN fights f ON f.id = b.fight_id \
          WHERE b.user_id = st.user_id AND outcome <> 'refunded' AND f.closes_at < cut.t), \
       st.user_id \
FROM (SELECT LEAST(now(), ends_at) AS t FROM seasons WHERE id = $1) AS cut, \
     season_stats st JOIN users u ON u.id = st.user_id \
WHERE st.season_id = $1 AND st.excluded_at IS NULL AND u.banned_at IS NULL \
  AND st.settled_fights >= $2 \
ORDER BY st.ranked_profit DESC, st.settled_fights DESC, st.user_id \
LIMIT $3 OFFSET $4";

fn protocol_error(what: &str) -> sqlx::Error {
    sqlx::Error::Protocol(what.to_owned())
}

fn season_of(row: &PgRow) -> Result<SeasonView, sqlx::Error> {
    Ok(SeasonView {
        id: u32::try_from(row.try_get::<i32, _>(0)?).map_err(|_| protocol_error("season"))?,
        starts_at: TimestampMs::from_ms(row.try_get(1)?),
        ends_at: TimestampMs::from_ms(row.try_get(2)?),
        closed: row.try_get(3)?,
    })
}

/// A season by its id, `None` if it has no row.
///
/// # Errors
/// A database error.
pub async fn season(conn: &mut PgConnection, id: i32) -> Result<Option<SeasonView>, sqlx::Error> {
    let sql = concat!("SELECT ", season_columns!(), " FROM seasons WHERE id = $1");
    let row = sqlx::query(sql).bind(id).fetch_optional(conn).await?;
    row.as_ref().map(season_of).transpose()
}

/// The season of this moment.
///
/// # Errors
/// A database error.
pub async fn current(conn: &mut PgConnection) -> Result<SeasonView, sqlx::Error> {
    let row = sqlx::query(CURRENT_SQL).fetch_one(conn).await?;
    season_of(&row)
}

/// The seasons with a row, the newest first.
///
/// # Errors
/// A database error.
pub async fn seasons(conn: &mut PgConnection) -> Result<Vec<SeasonView>, sqlx::Error> {
    let sql = concat!(
        "SELECT ",
        season_columns!(),
        " FROM seasons ORDER BY id DESC LIMIT $1"
    );
    let rows = sqlx::query(sql)
        .bind(SEASONS_LISTED)
        .fetch_all(conn)
        .await?;
    rows.iter().map(season_of).collect()
}

/// One eligible user of a season, in the order of its leaderboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked {
    pub user_id: Uuid,
    pub display_name: String,
    pub ranked_profit: i64,
    pub settled_fights: i32,
}

/// The leaderboard of a season: up to [`LEADERBOARD_SIZE`] eligible users, ranked from 1. Empty for a
/// season without a row.
///
/// # Errors
/// A database error, or a row the `CHECK`s of the tables forbid.
pub async fn entries(
    conn: &mut PgConnection,
    season_id: i32,
) -> Result<Vec<LeaderboardEntry>, sqlx::Error> {
    ranked(conn, season_id, LEADERBOARD_SIZE)
        .await?
        .into_iter()
        .enumerate()
        .map(|(i, user)| {
            Ok(LeaderboardEntry {
                rank: u16::try_from(i + 1).map_err(|_| protocol_error("rank"))?,
                display_name: user.display_name,
                ranked_profit: CoinAmount::new(user.ranked_profit)
                    .ok_or_else(|| protocol_error("ranked profit"))?,
                settled_fights: u32::try_from(user.settled_fights)
                    .map_err(|_| protocol_error("fights"))?,
            })
        })
        .collect()
}

/// The first `limit` eligible users of a season, in the order of its leaderboard: what the board shows
/// and what `farm-report` reviews (020, phase B).
///
/// # Errors
/// A database error.
pub async fn ranked(
    conn: &mut PgConnection,
    season_id: i32,
    limit: usize,
) -> Result<Vec<Ranked>, sqlx::Error> {
    let mut users = Vec::new();
    let mut offset = 0_i64;
    loop {
        let rows = sqlx::query(CANDIDATES_SQL)
            .bind(season_id)
            .bind(SEASON_MIN_FIGHTS)
            .bind(CANDIDATE_BATCH)
            .bind(offset)
            .fetch_all(&mut *conn)
            .await?;
        for row in &rows {
            let age_ms: i64 = row.try_get(3)?;
            let fights: i64 = row.try_get(4)?;
            if probation::of(age_ms, fights).is_some() {
                continue;
            }
            users.push(Ranked {
                user_id: row.try_get(5)?,
                display_name: row.try_get(0)?,
                ranked_profit: row.try_get(1)?,
                settled_fights: row.try_get(2)?,
            });
            if users.len() == limit {
                return Ok(users);
            }
        }
        offset += CANDIDATE_BATCH;
        if i64::try_from(rows.len()).unwrap_or(i64::MAX) < CANDIDATE_BATCH
            || offset >= CANDIDATE_BATCH * MAX_BATCHES
        {
            return Ok(users);
        }
    }
}
