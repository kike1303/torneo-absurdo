//! The SQL of `botargas` and `botarga_unlocks`. Every statement names its user by the id of the
//! session: nothing here takes a user from a request.

use engine::loadout::{budget, slots};
use protocol::{
    BotargaBuild, BotargaDaily, BotargaProgress, BotargaStats, BotargaUnlocks, BotargaView,
    FighterCosmetics, IdleClip, VictoryClip,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::{Starter, xp_to_next};

/// A row that a `CHECK` of the table forbids, so only someone with the credential can have written
/// it. Its own error, so it is not taken for a failure of the protocol of the database.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("a row of botargas does not decode")]
pub struct BadBotargaRow;

/// The botarga of a new user and its `starter` unlocks (021 "Alta"). `false`: the user already had
/// one, and nothing was written. The level, the rating and the look are the defaults of the table.
///
/// # Errors
/// The database error.
pub async fn create(
    tx: &mut PgConnection,
    user_id: Uuid,
    starter: &Starter,
) -> Result<bool, sqlx::Error> {
    let s = starter.stats;
    let created = sqlx::query(
        "INSERT INTO botargas (user_id, hp, attack, defense, speed, special, style_id, abilities) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, ARRAY[$8]) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(i16::from(s.hp))
    .bind(i16::from(s.attack))
    .bind(i16::from(s.defense))
    .bind(i16::from(s.speed))
    .bind(i16::from(s.special))
    .bind(&starter.style_id)
    .bind(&starter.ability_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if created {
        sqlx::query(
            "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) \
             VALUES ($1, 'ability', $2, 'starter') ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(&starter.ability_id)
        .execute(&mut *tx)
        .await?;
    }
    Ok(created)
}

macro_rules! unlocked_sql {
    ($kind:literal) => {
        concat!(
            "ARRAY(SELECT item_id FROM botarga_unlocks u WHERE u.user_id = b.user_id \
             AND u.kind = '",
            $kind,
            "' ORDER BY item_id)"
        )
    };
}

/// One statement, so the build and the unlocks are of one snapshot.
const VIEW_SQL: &str = concat!(
    "SELECT b.level, b.xp, b.hp, b.attack, b.defense, b.speed, b.special, b.style_id, \
     b.abilities, b.accessories, b.cosmetic_idle, b.cosmetic_victory, b.body_type, b.face_id, \
     b.rating, b.rated_fights, ",
    unlocked_sql!("ability"),
    ", ",
    unlocked_sql!("accessory"),
    ", ",
    unlocked_sql!("cosmetic"),
    " FROM botargas b WHERE b.user_id = $1"
);

const VIEW_LOCKED_SQL: &str = concat!(
    "SELECT b.level, b.xp, b.hp, b.attack, b.defense, b.speed, b.special, b.style_id, \
     b.abilities, b.accessories, b.cosmetic_idle, b.cosmetic_victory, b.body_type, b.face_id, \
     b.rating, b.rated_fights, ",
    unlocked_sql!("ability"),
    ", ",
    unlocked_sql!("accessory"),
    ", ",
    unlocked_sql!("cosmetic"),
    " FROM botargas b WHERE b.user_id = $1 FOR UPDATE OF b"
);

fn bad() -> sqlx::Error {
    sqlx::Error::Decode(Box::new(BadBotargaRow))
}

fn stat(row: &PgRow, at: usize) -> Result<u8, sqlx::Error> {
    u8::try_from(row.try_get::<i16, _>(at)?).map_err(|_| bad())
}

fn decode(row: &PgRow) -> Result<BotargaView, sqlx::Error> {
    let level = u8::try_from(row.try_get::<i16, _>(0)?).map_err(|_| bad())?;
    let stats = BotargaStats {
        hp: stat(row, 2)?,
        attack: stat(row, 3)?,
        defense: stat(row, 4)?,
        speed: stat(row, 5)?,
        special: stat(row, 6)?,
    };
    let spent: u16 = [
        stats.hp,
        stats.attack,
        stats.defense,
        stats.speed,
        stats.special,
    ]
    .iter()
    .map(|&s| u16::from(s))
    .sum();
    let budget = budget(level).ok_or_else(bad)?;
    let idle: Option<String> = row.try_get(10)?;
    let victory: Option<String> = row.try_get(11)?;
    Ok(BotargaView {
        build: BotargaBuild {
            stats,
            style_id: row.try_get(7)?,
            abilities: row.try_get(8)?,
            accessories: row.try_get(9)?,
            // A code this binary does not know is the variant of the tournament, as in the renderer.
            cosmetics: FighterCosmetics {
                idle: idle.as_deref().and_then(IdleClip::parse),
                victory: victory.as_deref().and_then(VictoryClip::parse),
            },
        },
        progress: BotargaProgress {
            level,
            xp: u32::try_from(row.try_get::<i32, _>(1)?).map_err(|_| bad())?,
            xp_to_next: xp_to_next(level),
            budget_free: budget.saturating_sub(spent),
            slots_open: u8::try_from(slots(level)).map_err(|_| bad())?,
            // There are no player fights yet (021 phases 2 and 4): no attack today, at full pay.
            daily: BotargaDaily {
                attacks: 0,
                factor_pct: 100,
            },
        },
        unlocks: BotargaUnlocks {
            abilities: row.try_get(16)?,
            accessories: row.try_get(17)?,
            cosmetics: row.try_get(18)?,
        },
        body_type: row.try_get(12)?,
        face_id: row.try_get(13)?,
        rating: u16::try_from(row.try_get::<i32, _>(14)?).map_err(|_| bad())?,
        rated_fights: u32::try_from(row.try_get::<i32, _>(15)?).map_err(|_| bad())?,
    })
}

/// The botarga of `user_id`, or `None` without one. Read-only.
///
/// # Errors
/// The database error, or [`BadBotargaRow`] as a decode error.
pub async fn view(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<BotargaView>, sqlx::Error> {
    let row = sqlx::query(VIEW_SQL)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.as_ref().map(decode).transpose()
}

/// Like [`view`], with the row of `botargas` locked `FOR UPDATE` until the transaction ends.
///
/// # Errors
/// The database error, or [`BadBotargaRow`] as a decode error.
pub async fn view_locked(
    tx: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<BotargaView>, sqlx::Error> {
    let row = sqlx::query(VIEW_LOCKED_SQL)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
    row.as_ref().map(decode).transpose()
}

/// Writes a build that [`super::build::check`] accepted. `false`: the user has no botarga.
///
/// # Errors
/// The database error.
pub async fn save_build(
    tx: &mut PgConnection,
    user_id: Uuid,
    build: &BotargaBuild,
) -> Result<bool, sqlx::Error> {
    let s = build.stats;
    let done = sqlx::query(
        "UPDATE botargas SET hp = $2, attack = $3, defense = $4, speed = $5, special = $6, \
         style_id = $7, abilities = $8, accessories = $9, cosmetic_idle = $10, \
         cosmetic_victory = $11, updated_at = now() WHERE user_id = $1",
    )
    .bind(user_id)
    .bind(i16::from(s.hp))
    .bind(i16::from(s.attack))
    .bind(i16::from(s.defense))
    .bind(i16::from(s.speed))
    .bind(i16::from(s.special))
    .bind(&build.style_id)
    .bind(&build.abilities)
    .bind(&build.accessories)
    .bind(build.cosmetics.idle.map(IdleClip::as_str))
    .bind(build.cosmetics.victory.map(VictoryClip::as_str))
    .execute(&mut *tx)
    .await?;
    Ok(done.rows_affected() == 1)
}
