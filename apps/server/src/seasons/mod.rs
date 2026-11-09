//! Seasons (feature 020, ADR-0008 D8): a calendar month in UTC, the `RankedProfit` of each user in it,
//! and the public leaderboard. The season of a fight is the month of its `closes_at`.
//!
//! Only [`tally`] adds to `season_stats`: from the settlement hook, in the transaction that settles
//! the fight, and from [`reconcile`], for what was settled without it (before this feature, or by an
//! old binary during a deploy). The mark of `season_tallies` makes every path sum a fight once.

pub mod api;
pub mod board;

use sqlx::{PgConnection, PgPool};

/// The fights a user needs won or lost in a season to appear in its leaderboard (owner, 2026-11-08).
pub const SEASON_MIN_FIGHTS: i32 = 10;

/// The month of `closes_at`, as a UTC timestamp: the same formula in every query of the feature,
/// never the session's `TimeZone` (020, security review 1).
macro_rules! season_month_sql {
    () => {
        "date_trunc('month', closes_at AT TIME ZONE 'UTC')"
    };
}

const CREATE_SEASON_SQL: &str = concat!(
    "INSERT INTO seasons (id, starts_at, ends_at) \
     SELECT (extract(year FROM m) * 100 + extract(month FROM m))::int, \
            m AT TIME ZONE 'UTC', (m + interval '1 month') AT TIME ZONE 'UTC' \
     FROM (SELECT ",
    season_month_sql!(),
    " AS m FROM fights WHERE id = $1) AS month \
     ON CONFLICT (id) DO NOTHING"
);

/// The row of the season of a fight, `FOR KEY SHARE`: before its mark and its stats, since `close`
/// takes the same row `FOR UPDATE` before them (020, phase B). Without it, a sum would insert its mark
/// and only then wait on the season (by the foreign key), while `close` waited on that mark: a cycle.
/// Sums of the same season do not block each other.
const LOCK_SEASON_SQL: &str = concat!(
    "SELECT 1 FROM seasons WHERE id = \
     (SELECT (extract(year FROM m) * 100 + extract(month FROM m))::int \
      FROM (SELECT ",
    season_month_sql!(),
    " AS m FROM fights WHERE id = $1) AS month) \
     FOR KEY SHARE"
);

const MARK_SQL: &str = concat!(
    "INSERT INTO season_tallies (fight_id, season_id) \
     SELECT id, (extract(year FROM m) * 100 + extract(month FROM m))::int \
     FROM (SELECT id, ",
    season_month_sql!(),
    " AS m FROM fights WHERE id = $1) AS month \
     ON CONFLICT (fight_id) DO NOTHING RETURNING season_id"
);

/// One row per user, in the order of `user_id`: two settlements with users in common take their
/// locks in the same order, so they never deadlock (020, security review 8). Refunds add nothing.
const ADD_SQL: &str = "\
INSERT INTO season_stats (season_id, user_id, ranked_profit, settled_fights) \
SELECT $2, user_id, sum(ranked_profit)::bigint, 1 FROM bet_settlements \
WHERE fight_id = $1 AND outcome <> 'refunded' GROUP BY user_id ORDER BY user_id \
ON CONFLICT (season_id, user_id) DO UPDATE SET \
    ranked_profit = season_stats.ranked_profit + EXCLUDED.ranked_profit, \
    settled_fights = season_stats.settled_fights + 1";

/// Sums the results of a settled fight in its season, inside the caller's transaction. `false` if
/// it was already summed. A closed season still gets the sum, never an error that would undo the
/// settlement (RF31 of 010): it cannot happen, since `close` sums what is missing under its lock,
/// and if it does, it leaves a log.
///
/// # Errors
/// A database error.
pub async fn tally(conn: &mut PgConnection, fight_id: i32) -> Result<bool, sqlx::Error> {
    sqlx::query(CREATE_SEASON_SQL)
        .bind(fight_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(LOCK_SEASON_SQL)
        .bind(fight_id)
        .execute(&mut *conn)
        .await?;
    let season: Option<i32> = sqlx::query_scalar(MARK_SQL)
        .bind(fight_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(season) = season else {
        return Ok(false);
    };
    sqlx::query(ADD_SQL)
        .bind(fight_id)
        .bind(season)
        .execute(&mut *conn)
        .await?;
    let closed: bool =
        sqlx::query_scalar("SELECT closed_at IS NOT NULL FROM seasons WHERE id = $1")
            .bind(season)
            .fetch_one(&mut *conn)
            .await?;
    if closed {
        tracing::warn!(event = "seasons.tally_after_close", season_id = season);
    }
    Ok(true)
}

/// Settled fights without a mark, a batch at a time, after the last one seen.
const UNTALLIED_SQL: &str = "\
SELECT f.id FROM fights f WHERE f.state = 'settled' AND f.id > $1 \
AND NOT EXISTS (SELECT 1 FROM season_tallies t WHERE t.fight_id = f.id) \
ORDER BY f.id LIMIT $2";

const RECONCILE_BATCH: i64 = 200;

/// Sums every settled fight without a mark, each in its own transaction, and returns how many it
/// summed. Safe at any time and as often as wanted: a fight the hook sums meanwhile keeps its mark,
/// and this one skips it. A fight whose sum fails is logged and left for the next pass, without
/// holding back the ones after it; `close` sees it as pending and refuses.
///
/// # Errors
/// A database error while listing the fights; what was summed before it stays.
pub async fn reconcile(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let mut summed = 0_u64;
    let mut after = 0_i32;
    loop {
        let ids: Vec<i32> = sqlx::query_scalar(UNTALLIED_SQL)
            .bind(after)
            .bind(RECONCILE_BATCH)
            .fetch_all(pool)
            .await?;
        let Some(&last) = ids.last() else {
            return Ok(summed);
        };
        for id in ids {
            match tally_alone(pool, id).await {
                Ok(true) => summed += 1,
                Ok(false) => {}
                Err(error) => tracing::warn!(
                    event = "seasons.tally_failed",
                    error_kind = crate::db::error_kind(&error)
                ),
            }
        }
        after = last;
    }
}

async fn tally_alone(pool: &PgPool, fight_id: i32) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let summed = tally(&mut tx, fight_id).await?;
    tx.commit().await?;
    Ok(summed)
}

/// [`reconcile`], logging a failure by its code and nothing else: it never stops the server.
pub async fn reconcile_logged(pool: &PgPool) {
    if let Err(error) = reconcile(pool).await {
        tracing::warn!(
            event = "seasons.reconcile_failed",
            error_kind = crate::db::error_kind(&error)
        );
    }
}
