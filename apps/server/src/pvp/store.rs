//! The SQL of `pvp_fights` and `pvp_daily`. The day is always the UTC day of the database's
//! `now()`, the instant of the transaction, so the counter, the pair limit and the season of a
//! fight agree.

use protocol::{
    EndReason, PvpFightDetail, PvpFightSummary, PvpFighter, PvpFightsPage, PvpReward, PvpRole,
    Side, TimestampMs,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::rewards::{Rewards, factor_pct};

/// Fights of a page of `GET /api/pvp/fights`.
pub const PAGE_SIZE: usize = 20;

/// A row that a `CHECK` of the table forbids, so only someone with the credential can have written
/// it.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("a row of pvp_fights does not decode")]
pub struct BadPvpRow;

fn bad() -> sqlx::Error {
    sqlx::Error::Decode(Box::new(BadPvpRow))
}

/// The UTC day of `now()`, as a `date`.
macro_rules! today {
    () => {
        "(now() AT TIME ZONE 'UTC')::date"
    };
}

/// The attacks of `user_id` today.
///
/// # Errors
/// The database error.
pub async fn attacks_today(conn: &mut PgConnection, user_id: Uuid) -> Result<u16, sqlx::Error> {
    let attacks: Option<i16> = sqlx::query_scalar(concat!(
        "SELECT attacks FROM pvp_daily WHERE user_id = $1 AND day = ",
        today!()
    ))
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    u16::try_from(attacks.unwrap_or(0)).map_err(|_| bad())
}

/// One more attack of `user_id` today. Only the attacker's: a defense never counts.
///
/// # Errors
/// The database error.
pub async fn count_attack(tx: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(concat!(
        "INSERT INTO pvp_daily (user_id, day, attacks) VALUES ($1, ",
        today!(),
        ", 1) ON CONFLICT (user_id, day) DO UPDATE SET attacks = pvp_daily.attacks + 1"
    ))
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The fights between `a` and `b` today, in either direction.
///
/// # Errors
/// The database error.
pub async fn pair_fights_today(
    conn: &mut PgConnection,
    a: Uuid,
    b: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM pvp_fights \
         WHERE created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' \
         AND ((attacker_id = $1 AND defender_id = $2) OR (attacker_id = $2 AND defender_id = $1))",
    )
    .bind(a)
    .bind(b)
    .fetch_one(&mut *conn)
    .await
}

/// The fight an attacker already created with `client_key`.
///
/// # Errors
/// The database error.
pub async fn by_key(
    conn: &mut PgConnection,
    attacker_id: Uuid,
    client_key: Uuid,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM pvp_fights WHERE attacker_id = $1 AND client_key = $2")
        .bind(attacker_id)
        .bind(client_key)
        .fetch_optional(&mut *conn)
        .await
}

/// Who the attacker fought.
pub enum DefenderRef<'a> {
    Player(Uuid),
    Tournament(&'a str),
}

/// A fight the engine resolved, ready to be written.
pub struct NewFight<'a> {
    pub attacker_id: Uuid,
    pub defender: DefenderRef<'a>,
    pub client_key: Uuid,
    pub ruleset_version: u16,
    pub spec_a: &'a [u8],
    pub spec_b: &'a [u8],
    pub seed: &'a [u8; 32],
    pub log: &'a [u8],
    pub winner: Option<Side>,
    pub end_reason: EndReason,
    pub rewards: &'a Rewards,
    pub daily_index: u16,
}

const fn side_code(side: Side) -> &'static str {
    match side {
        Side::A => "a",
        Side::B => "b",
    }
}

const fn reason_code(reason: EndReason) -> &'static str {
    match reason {
        EndReason::Ko => "ko",
        EndReason::Decision => "decision",
        EndReason::Draw => "draw",
    }
}

fn side_of(code: Option<&str>) -> Result<Option<Side>, sqlx::Error> {
    match code {
        None => Ok(None),
        Some("a") => Ok(Some(Side::A)),
        Some("b") => Ok(Some(Side::B)),
        Some(_) => Err(bad()),
    }
}

fn reason_of(code: &str) -> Result<EndReason, sqlx::Error> {
    match code {
        "ko" => Ok(EndReason::Ko),
        "decision" => Ok(EndReason::Decision),
        "draw" => Ok(EndReason::Draw),
        _ => Err(bad()),
    }
}

/// Writes `fight` and returns its id. `created_at` and `season_id` are the database's.
///
/// # Errors
/// The database error; a numbers out of the columns' range is a decode error, and nothing is
/// written.
pub async fn insert(tx: &mut PgConnection, fight: &NewFight<'_>) -> Result<i32, sqlx::Error> {
    let (defender_id, defender_slug) = match fight.defender {
        DefenderRef::Player(id) => (Some(id), None),
        DefenderRef::Tournament(slug) => (None, Some(slug)),
    };
    let int = |value: u32| i32::try_from(value).map_err(|_| bad());
    let r = fight.rewards;
    sqlx::query_scalar(
        "INSERT INTO pvp_fights (attacker_id, defender_id, defender_slug, client_key, \
         ruleset_version, spec_a, spec_b, seed, log, winner, end_reason, rating_delta, \
         xp_attacker, xp_defender, coins_attacker, daily_index, rated, defender_rating_delta, \
         season_id, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, \
         0, now()) \
         RETURNING id",
    )
    .bind(fight.attacker_id)
    .bind(defender_id)
    .bind(defender_slug)
    .bind(fight.client_key)
    .bind(i16::try_from(fight.ruleset_version).map_err(|_| bad())?)
    .bind(fight.spec_a)
    .bind(fight.spec_b)
    .bind(fight.seed.as_slice())
    .bind(fight.log)
    .bind(fight.winner.map(side_code))
    .bind(reason_code(fight.end_reason))
    .bind(r.rating_delta)
    .bind(int(r.xp_attacker)?)
    .bind(int(r.xp_defender)?)
    .bind(int(r.coins_attacker)?)
    .bind(i16::try_from(fight.daily_index).map_err(|_| bad())?)
    .bind(r.rated)
    .bind(r.defender_rating_delta)
    .fetch_one(&mut *tx)
    .await
}

/// Moves the rating of `user_id` by `delta`, within the bounds of 021, and counts one more rated
/// fight (the K of the next ones). The row is the transaction's: the attacker's lock or the
/// defender's `SKIP LOCKED`.
///
/// # Errors
/// The database error.
pub async fn rate(tx: &mut PgConnection, user_id: Uuid, delta: i32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE botargas SET rating = LEAST($3, GREATEST($2, rating + $1)), \
         rated_fights = rated_fights + 1 WHERE user_id = $4",
    )
    .bind(delta)
    .bind(super::rewards::RATING_MIN)
    .bind(super::rewards::RATING_MAX)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The columns of a fight that the two reads share, in the order [`fighter`] and [`detail_of`]
/// read them.
macro_rules! fight_sql {
    ($columns:literal, $suffix:literal) => {
        concat!(
            "SELECT f.id, ua.display_name, ud.display_name, f.defender_slug, f.winner, \
             f.end_reason, f.rating_delta, (extract(epoch FROM f.created_at) * 1000)::bigint",
            $columns,
            " FROM pvp_fights f JOIN users ua ON ua.id = f.attacker_id \
             LEFT JOIN users ud ON ud.id = f.defender_id",
            $suffix
        )
    };
}

/// Side `b` of a row: the user's name, or the slug of the character of the tournament.
fn defender_of(row: &PgRow) -> Result<PvpFighter, sqlx::Error> {
    match (row.try_get::<Option<String>, _>(2)?, row.try_get(3)?) {
        (Some(display_name), None) => Ok(PvpFighter::Player { display_name }),
        (None, Some(slug)) => Ok(PvpFighter::Tournament { slug }),
        _ => Err(bad()),
    }
}

fn attacker_of(row: &PgRow) -> Result<PvpFighter, sqlx::Error> {
    Ok(PvpFighter::Player {
        display_name: row.try_get(1)?,
    })
}

fn id_of(row: &PgRow) -> Result<u32, sqlx::Error> {
    u32::try_from(row.try_get::<i32, _>(0)?).map_err(|_| bad())
}

fn text(row: &PgRow, at: usize) -> Result<String, sqlx::Error> {
    String::from_utf8(row.try_get(at)?).map_err(|_| bad())
}

fn detail_of(row: &PgRow) -> Result<PvpFightDetail, sqlx::Error> {
    let seed: Vec<u8> = row.try_get(10)?;
    Ok(PvpFightDetail {
        id: id_of(row)?,
        a: attacker_of(row)?,
        b: defender_of(row)?,
        spec_a: text(row, 8)?,
        spec_b: text(row, 9)?,
        seed: crate::fights::hex(&seed),
        log: text(row, 11)?,
        ruleset_version: u16::try_from(row.try_get::<i16, _>(12)?).map_err(|_| bad())?,
        winner: side_of(row.try_get(4)?)?,
        end_reason: reason_of(row.try_get(5)?)?,
        rating_delta: row.try_get(6)?,
        created_at: TimestampMs::from_ms(row.try_get(7)?),
    })
}

/// The public detail of fight `id`.
///
/// # Errors
/// The database error, or [`BadPvpRow`] as a decode error.
pub async fn detail(
    conn: &mut PgConnection,
    id: i32,
) -> Result<Option<PvpFightDetail>, sqlx::Error> {
    let row = sqlx::query(fight_sql!(
        ", f.spec_a, f.spec_b, f.seed, f.log, f.ruleset_version",
        " WHERE f.id = $1"
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    row.as_ref().map(detail_of).transpose()
}

fn count(row: &PgRow, at: usize) -> Result<u32, sqlx::Error> {
    u32::try_from(row.try_get::<i32, _>(at)?).map_err(|_| bad())
}

/// What fight `row` gave to `role`, each side with its own change of rating (TA-64: each its own
/// K). Read with the columns of [`page`].
fn reward_of(row: &PgRow, role: PvpRole) -> Result<PvpReward, sqlx::Error> {
    let delta: i16 = row.try_get(6)?;
    let daily_index = u16::try_from(row.try_get::<i16, _>(11)?).map_err(|_| bad())?;
    Ok(match role {
        PvpRole::Attacker => PvpReward {
            rating_delta: delta,
            xp: count(row, 9)?,
            coins: count(row, 8)?,
            factor_pct: factor_pct(daily_index),
        },
        PvpRole::Defender => PvpReward {
            rating_delta: row.try_get(12)?,
            xp: count(row, 10)?,
            coins: 0,
            factor_pct: factor_pct(daily_index),
        },
    })
}

/// What fight `id` gave to its attacker `attacker_id`, for the answer of a repeated `client_key`.
///
/// # Errors
/// The database error, or [`BadPvpRow`] as a decode error.
pub async fn attacker_reward(
    conn: &mut PgConnection,
    id: i32,
    attacker_id: Uuid,
) -> Result<Option<PvpReward>, sqlx::Error> {
    let row = sqlx::query(fight_sql!(
        ", f.coins_attacker, f.xp_attacker, f.xp_defender, f.daily_index, \
         f.defender_rating_delta",
        " WHERE f.id = $1 AND f.attacker_id = $2"
    ))
    .bind(id)
    .bind(attacker_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.as_ref()
        .map(|row| reward_of(row, PvpRole::Attacker))
        .transpose()
}

/// A page of the fights of `user_id`, as attacker and as defender, the most recent first.
///
/// # Errors
/// The database error, or [`BadPvpRow`] as a decode error.
pub async fn page(
    conn: &mut PgConnection,
    user_id: Uuid,
    before: Option<i32>,
) -> Result<PvpFightsPage, sqlx::Error> {
    let limit = i64::try_from(PAGE_SIZE + 1).unwrap_or(i64::MAX);
    let rows = sqlx::query(fight_sql!(
        ", f.coins_attacker, f.xp_attacker, f.xp_defender, f.daily_index, \
         f.defender_rating_delta, f.attacker_id = $1",
        " WHERE (f.attacker_id = $1 OR f.defender_id = $1) AND ($2::int IS NULL OR f.id < $2) \
         ORDER BY f.id DESC LIMIT $3"
    ))
    .bind(user_id)
    .bind(before)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    let more = rows.len() > PAGE_SIZE;
    let mut fights = Vec::with_capacity(PAGE_SIZE);
    for row in rows.iter().take(PAGE_SIZE) {
        let role = if row.try_get(13)? {
            PvpRole::Attacker
        } else {
            PvpRole::Defender
        };
        fights.push(PvpFightSummary {
            id: id_of(row)?,
            role,
            opponent: match role {
                PvpRole::Attacker => defender_of(row)?,
                PvpRole::Defender => attacker_of(row)?,
            },
            winner: side_of(row.try_get(4)?)?,
            end_reason: reason_of(row.try_get(5)?)?,
            reward: reward_of(row, role)?,
            created_at: TimestampMs::from_ms(row.try_get(7)?),
        });
    }
    let next_before = if more {
        fights.last().map(|fight| fight.id)
    } else {
        None
    };
    Ok(PvpFightsPage {
        fights,
        next_before,
    })
}
