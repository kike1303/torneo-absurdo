//! The defender of an attack (021 "Emparejamiento"): windows of rating that widen by steps, each
//! with the level gap of the attacker; never oneself nor a banned user; without the last 3 rivals
//! unless that leaves nobody; and among the candidates of a step, those with a session in the last
//! 30 days first (`users.last_seen_at`, never a join to `sessions`).
//!
//! A candidate is drawn with `OsRng` among the rows of the step (a count, then an `OFFSET`, without
//! locks), and only that row is locked, with its filters checked again, `FOR UPDATE SKIP LOCKED`: an
//! attacker in the middle of its own attack, or a user saving a build, is skipped and never waited
//! for. The attacker is always locked first and the defender is the only other row, so there is no
//! order of locks to keep and a fight never waits for another fight. A candidate whose saved build
//! no longer resolves with the catalog of this binary is skipped as well.

use engine::CharacterSpec;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::rewards::{PROBATION_ATTACKS, PROBATION_DAYS};
use crate::botarga::{BotargaCatalog, build, store};

/// The windows of rating, by step.
pub const RATING_STEPS: [i32; 4] = [100, 200, 300, 400];

/// The rivals of the last attacks that are left out.
pub const RECENT_RIVALS: i64 = 3;

/// Days of `last_seen_at` that make a candidate preferred.
pub const ACTIVE_DAYS: i32 = 30;

/// Candidates drawn per step before the next one: each draw that fails (a locked row, a build
/// that does not resolve) leaves the candidate out for the rest of the search.
const DRAWS_PER_STEP: usize = 4;

/// The level gap of an attacker of `level`: `min(10, 3 + level / 5)`.
#[must_use]
pub fn level_gap(level: u8) -> i16 {
    i16::from(level / 5 + 3).min(10)
}

/// The attacker, as the search reads it.
#[derive(Debug, Clone, Copy)]
pub struct Seeker {
    pub user_id: Uuid,
    pub level: u8,
    pub rating: u16,
}

/// The defender, locked until the transaction ends.
#[derive(Debug, Clone)]
pub struct Defender {
    pub user_id: Uuid,
    pub display_name: String,
    pub spec: CharacterSpec,
    pub rating: u16,
    pub probation: bool,
}

/// The rows of a step. `$1` the users left out (the attacker among them), `$2` the rating and `$3`
/// its window, `$4` the level and `$5` its gap.
macro_rules! candidates {
    () => {
        " FROM botargas b JOIN users u ON u.id = b.user_id \
          WHERE u.banned_at IS NULL AND b.user_id <> ALL($1::uuid[]) \
          AND abs(b.rating - $2) <= $3 AND abs(b.level - $4) <= $5"
    };
}

macro_rules! active {
    () => {
        "u.last_seen_at > now() - make_interval(days => $6)"
    };
}

/// Whether a user is in the PvP probation (021 "Carencia PvP"): younger than 7 days, or with fewer
/// than 10 attacks. `$1` the user, `$2` the days and `$3` the attacks.
pub const PROBATION_SQL: &str = "SELECT u.created_at > now() - make_interval(days => $2) \
     OR (SELECT count(*) FROM pvp_fights f WHERE f.attacker_id = $1) < $3 \
     FROM users u WHERE u.id = $1";

/// Whether `user_id` is in the PvP probation. A missing user is.
///
/// # Errors
/// The database error.
pub async fn probation(conn: &mut PgConnection, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(PROBATION_SQL)
        .bind(user_id)
        .bind(PROBATION_DAYS)
        .bind(PROBATION_ATTACKS)
        .fetch_optional(&mut *conn)
        .await?;
    row.map_or(Ok(true), |row| row.try_get(0))
}

/// The defenders of the last attacks of `user_id`.
///
/// # Errors
/// The database error.
pub async fn recent_rivals(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT defender_id FROM pvp_fights WHERE attacker_id = $1 AND defender_id IS NOT NULL \
         ORDER BY id DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(RECENT_RIVALS)
    .fetch_all(&mut *conn)
    .await
}

/// The defender of `seeker`, or `None` if no user fits after the last step.
///
/// # Errors
/// The database error.
pub async fn pick(
    tx: &mut PgConnection,
    catalog: &BotargaCatalog,
    seeker: Seeker,
    recent: &[Uuid],
) -> Result<Option<Defender>, sqlx::Error> {
    let mut skipped = vec![seeker.user_id];
    // Without the recent rivals first; if that leaves nobody, with them.
    let passes: &[bool] = if recent.is_empty() {
        &[false]
    } else {
        &[true, false]
    };
    for &leave_recent_out in passes {
        for window in RATING_STEPS {
            for _ in 0..DRAWS_PER_STEP {
                let mut out = skipped.clone();
                if leave_recent_out {
                    out.extend_from_slice(recent);
                }
                let Some(id) = draw(tx, seeker, window, &out).await? else {
                    break;
                };
                skipped.push(id);
                if let Some(defender) = lock(tx, catalog, seeker, window, &out, id).await? {
                    return Ok(Some(defender));
                }
            }
        }
    }
    Ok(None)
}

/// One candidate of the step, drawn with `OsRng`, without locking anything.
async fn draw(
    tx: &mut PgConnection,
    seeker: Seeker,
    window: i32,
    out: &[Uuid],
) -> Result<Option<Uuid>, sqlx::Error> {
    let counts = sqlx::query(concat!(
        "SELECT count(*) FILTER (WHERE ",
        active!(),
        "), count(*)",
        candidates!()
    ))
    .bind(out)
    .bind(i32::from(seeker.rating))
    .bind(window)
    .bind(i16::from(seeker.level))
    .bind(level_gap(seeker.level))
    .bind(ACTIVE_DAYS)
    .fetch_one(&mut *tx)
    .await?;
    let active: i64 = counts.try_get(0)?;
    let total: i64 = counts.try_get(1)?;
    let only_active = active > 0;
    let n = if only_active { active } else { total };
    let Ok(n) = u64::try_from(n) else {
        return Ok(None);
    };
    if n == 0 {
        return Ok(None);
    }
    // Without entropy there is no fair draw: the attack fails rather than fall back in silence.
    let offset = super::uniform(n).ok_or_else(|| sqlx::Error::Protocol("os_rng".to_owned()))?;
    let offset = i64::try_from(offset).unwrap_or(0);
    sqlx::query_scalar(concat!(
        "SELECT b.user_id",
        candidates!(),
        " AND (NOT $7 OR ",
        active!(),
        ") ORDER BY b.user_id OFFSET $8 LIMIT 1"
    ))
    .bind(out)
    .bind(i32::from(seeker.rating))
    .bind(window)
    .bind(i16::from(seeker.level))
    .bind(level_gap(seeker.level))
    .bind(ACTIVE_DAYS)
    .bind(only_active)
    .bind(offset)
    .fetch_optional(&mut *tx)
    .await
}

/// `id` locked if it is still a candidate of the step and nobody holds it, with its build resolved.
async fn lock(
    tx: &mut PgConnection,
    catalog: &BotargaCatalog,
    seeker: Seeker,
    window: i32,
    out: &[Uuid],
    id: Uuid,
) -> Result<Option<Defender>, sqlx::Error> {
    let row = sqlx::query(concat!(
        "SELECT u.display_name",
        candidates!(),
        " AND b.user_id = $6 FOR UPDATE OF b SKIP LOCKED"
    ))
    .bind(out)
    .bind(i32::from(seeker.rating))
    .bind(window)
    .bind(i16::from(seeker.level))
    .bind(level_gap(seeker.level))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let display_name: String = row.try_get(0)?;
    // The row is ours until the commit: this read sees what the lock holds.
    let Some(view) = store::view(&mut *tx, id).await? else {
        return Ok(None);
    };
    let Ok(spec) = build::resolve(catalog, view.progress.level, &view.build) else {
        tracing::info!(event = "pvp.defender_build_invalid");
        return Ok(None);
    };
    Ok(Some(Defender {
        user_id: id,
        display_name,
        spec,
        rating: view.rating,
        probation: probation(&mut *tx, id).await?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_gap_follows_the_formula() {
        for (level, gap) in [
            (1, 3),
            (4, 3),
            (5, 4),
            (9, 4),
            (20, 7),
            (34, 9),
            (35, 10),
            (100, 10),
        ] {
            assert_eq!(level_gap(level), gap, "{level}");
        }
    }
}
