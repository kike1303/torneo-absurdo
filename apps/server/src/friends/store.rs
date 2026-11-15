//! The queries of friends (023). Every function takes the connection or the transaction of its
//! caller; the ones that write expect the locks [`super::api`] takes.

use protocol::{
    BracketMatch, EndReason, FriendEntry, FriendFightSummary, FriendFightsPage, FriendTournament,
    FriendTournamentSummary, FriendTournamentsPage, PvpFightDetail, PvpFighter, PvpLook, PvpRole,
    Side, TimestampMs,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::botarga::{self, BotargaCatalog, build};

/// A page of fights.
const PAGE_SIZE: usize = 20;

fn bad() -> sqlx::Error {
    sqlx::Error::Protocol("friends_row".to_owned())
}

/// The two users of a pair in the order of `friendships`.
fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

/// The invite code of a new botarga; one that already has one keeps it.
///
/// # Errors
/// The database error, or `os_rng` without entropy.
pub async fn create_invite(tx: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    let code = super::new_code().ok_or_else(|| sqlx::Error::Protocol("os_rng".to_owned()))?;
    sqlx::query(
        "INSERT INTO friend_invites (user_id, code) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(code)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// A new code for `user_id`: the old one stops working.
///
/// # Errors
/// The database error, or `os_rng` without entropy.
pub async fn rotate_invite(tx: &mut PgConnection, user_id: Uuid) -> Result<String, sqlx::Error> {
    let code = super::new_code().ok_or_else(|| sqlx::Error::Protocol("os_rng".to_owned()))?;
    sqlx::query(
        "INSERT INTO friend_invites (user_id, code) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET code = EXCLUDED.code, created_at = now()",
    )
    .bind(user_id)
    .bind(&code)
    .execute(&mut *tx)
    .await?;
    Ok(code)
}

/// The code of `user_id`, if it has one.
///
/// # Errors
/// The database error.
pub async fn invite_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT code FROM friend_invites WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(conn)
        .await
}

/// The owner of `code` that `reader` may befriend, with its name: not the reader, not banned, and
/// not someone who blocked the reader. `None` for all the rest alike.
///
/// # Errors
/// The database error.
pub async fn invite_owner(
    conn: &mut PgConnection,
    reader: Uuid,
    code: &str,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT u.id, u.display_name FROM friend_invites i JOIN users u ON u.id = i.user_id \
         WHERE i.code = $1 AND u.banned_at IS NULL AND u.id <> $2 \
           AND NOT EXISTS (SELECT 1 FROM friend_blocks b \
                           WHERE b.owner_id = u.id AND b.blocked_id = $2)",
    )
    .bind(code)
    .bind(reader)
    .fetch_optional(conn)
    .await?;
    row.map(|row| Ok((row.try_get(0)?, row.try_get(1)?)))
        .transpose()
}

/// Locks the rows of `users` of both, in the order of their ids, and says whether both are there
/// (and, with `active`, not banned). Accepting and removing take this; a challenge never does.
///
/// # Errors
/// The database error (a `lock_timeout` included).
pub async fn lock_pair(
    tx: &mut PgConnection,
    a: Uuid,
    b: Uuid,
    active: bool,
) -> Result<bool, sqlx::Error> {
    sqlx::query("SET LOCAL lock_timeout = '1s'")
        .execute(&mut *tx)
        .await?;
    let (lo, hi) = ordered(a, b);
    let rows = sqlx::query(
        "SELECT id FROM users WHERE id IN ($1, $2) AND (NOT $3 OR banned_at IS NULL) \
         ORDER BY id FOR UPDATE",
    )
    .bind(lo)
    .bind(hi)
    .bind(active)
    .fetch_all(&mut *tx)
    .await?;
    Ok(rows.len() == 2)
}

/// Whether the two are friends.
///
/// # Errors
/// The database error.
pub async fn are_friends(conn: &mut PgConnection, a: Uuid, b: Uuid) -> Result<bool, sqlx::Error> {
    let (lo, hi) = ordered(a, b);
    let row = sqlx::query("SELECT 1 FROM friendships WHERE user_lo = $1 AND user_hi = $2")
        .bind(lo)
        .bind(hi)
        .fetch_optional(conn)
        .await?;
    Ok(row.is_some())
}

/// The friends of `user_id`.
///
/// # Errors
/// The database error.
pub async fn count(conn: &mut PgConnection, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM friendships WHERE user_lo = $1 OR user_hi = $1")
        .bind(user_id)
        .fetch_one(conn)
        .await
}

/// Makes the two friends, and lifts the block `a` put on `b`: accepting the invite of someone one
/// removed is the way back (owner, 2026-11-15).
///
/// # Errors
/// The database error.
pub async fn befriend(tx: &mut PgConnection, a: Uuid, b: Uuid) -> Result<(), sqlx::Error> {
    let (lo, hi) = ordered(a, b);
    sqlx::query("INSERT INTO friendships (user_lo, user_hi) VALUES ($1, $2)")
        .bind(lo)
        .bind(hi)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM friend_blocks WHERE owner_id = $1 AND blocked_id = $2")
        .bind(a)
        .bind(b)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Ends the friendship, if there was one, and blocks `other` from accepting the invite of `me`.
/// `false` if they were not friends: nothing is written then.
///
/// # Errors
/// The database error.
pub async fn unfriend(tx: &mut PgConnection, me: Uuid, other: Uuid) -> Result<bool, sqlx::Error> {
    let (lo, hi) = ordered(me, other);
    let removed = sqlx::query("DELETE FROM friendships WHERE user_lo = $1 AND user_hi = $2")
        .bind(lo)
        .bind(hi)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        == 1;
    if removed {
        sqlx::query(
            "INSERT INTO friend_blocks (owner_id, blocked_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(me)
        .bind(other)
        .execute(&mut *tx)
        .await?;
    }
    Ok(removed)
}

/// The friend of `me` whose name has `key`, if any (and, with `active`, not banned).
///
/// # Errors
/// The database error.
pub async fn friend_by_key(
    conn: &mut PgConnection,
    me: Uuid,
    key: &str,
    active: bool,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN friendships f \
           ON (f.user_lo = $1 AND f.user_hi = u.id) OR (f.user_hi = $1 AND f.user_lo = u.id) \
         WHERE u.display_name_key = $2 AND (NOT $3 OR u.banned_at IS NULL)",
    )
    .bind(me)
    .bind(key)
    .bind(active)
    .fetch_optional(conn)
    .await
}

/// The friends of `user_id` by name, each with its level and whether it can be challenged now (a
/// build that resolves with this catalog, and no ban; one answer for both).
///
/// # Errors
/// The database error.
pub async fn friends_of(
    conn: &mut PgConnection,
    catalog: &BotargaCatalog,
    user_id: Uuid,
) -> Result<Vec<FriendEntry>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT u.id, u.display_name, u.banned_at IS NULL FROM friendships f \
         JOIN users u ON u.id = CASE WHEN f.user_lo = $1 THEN f.user_hi ELSE f.user_lo END \
         WHERE f.user_lo = $1 OR f.user_hi = $1 \
         ORDER BY u.display_name_key",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut friends = Vec::with_capacity(rows.len());
    for row in &rows {
        let id: Uuid = row.try_get(0)?;
        let not_banned: bool = row.try_get(2)?;
        let view = botarga::store::view(&mut *conn, id).await?;
        let (level, ready) = match &view {
            Some(view) => (
                view.progress.level,
                not_banned && build::resolve(catalog, view.progress.level, &view.build).is_ok(),
            ),
            None => (1, false),
        };
        friends.push(FriendEntry {
            display_name: row.try_get(1)?,
            level,
            ready,
        });
    }
    Ok(friends)
}

/// The challenges of `user_id` today (UTC).
///
/// # Errors
/// The database error.
pub async fn challenges_today(tx: &mut PgConnection, user_id: Uuid) -> Result<i16, sqlx::Error> {
    let n: Option<i16> = sqlx::query_scalar(
        "SELECT challenges FROM friend_daily \
         WHERE user_id = $1 AND day = (now() AT TIME ZONE 'UTC')::date",
    )
    .bind(user_id)
    .fetch_optional(tx)
    .await?;
    Ok(n.unwrap_or(0))
}

/// One more challenge of `user_id` today.
///
/// # Errors
/// The database error (the `CHECK` of the cap included).
pub async fn count_challenge(tx: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO friend_daily (user_id, day, challenges) \
         VALUES ($1, (now() AT TIME ZONE 'UTC')::date, 1) \
         ON CONFLICT (user_id, day) DO UPDATE SET challenges = friend_daily.challenges + 1",
    )
    .bind(user_id)
    .execute(tx)
    .await?;
    Ok(())
}

/// The challenge `user_id` already made with `client_key`.
///
/// # Errors
/// The database error.
pub async fn by_key(
    tx: &mut PgConnection,
    user_id: Uuid,
    client_key: Uuid,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM friend_fights WHERE attacker_id = $1 AND client_key = $2")
        .bind(user_id)
        .bind(client_key)
        .fetch_optional(tx)
        .await
}

/// A challenge, as written.
pub struct NewChallenge<'a> {
    pub attacker_id: Uuid,
    pub defender_id: Uuid,
    pub client_key: Uuid,
    pub ruleset_version: u16,
    pub fight: &'a crate::pvp::resolve::Resolved,
    pub look_a: &'a PvpLook,
    pub look_b: &'a PvpLook,
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

fn look_json(look: &PvpLook) -> Result<String, sqlx::Error> {
    serde_json::to_string(look).map_err(|_| bad())
}

fn look_of(row: &PgRow, at: usize) -> Result<Option<PvpLook>, sqlx::Error> {
    let text: Option<String> = row.try_get(at)?;
    text.map(|text| serde_json::from_str(&text).map_err(|_| bad()))
        .transpose()
}

/// Writes a challenge and returns its id.
///
/// # Errors
/// The database error.
pub async fn insert_challenge(
    tx: &mut PgConnection,
    c: &NewChallenge<'_>,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO friend_fights (kind, attacker_id, defender_id, client_key, ruleset_version, \
         spec_a, spec_b, seed, log, winner, end_reason, look_a, look_b) \
         VALUES ('challenge', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12::jsonb) \
         RETURNING id",
    )
    .bind(c.attacker_id)
    .bind(c.defender_id)
    .bind(c.client_key)
    .bind(i16::try_from(c.ruleset_version).map_err(|_| bad())?)
    .bind(&c.fight.spec_a)
    .bind(&c.fight.spec_b)
    .bind(c.fight.seed.as_slice())
    .bind(&c.fight.log)
    .bind(c.fight.winner.map(side_code))
    .bind(reason_code(c.fight.end_reason))
    .bind(look_json(c.look_a)?)
    .bind(look_json(c.look_b)?)
    .fetch_one(tx)
    .await
}

fn text(row: &PgRow, at: usize) -> Result<String, sqlx::Error> {
    String::from_utf8(row.try_get(at)?).map_err(|_| bad())
}

/// The detail of fight `id` for `reader`, who must be one of its two sides or, for a match of a
/// bracket, a player of its tournament: `None` otherwise, as for a fight that does not exist.
///
/// # Errors
/// The database error, or a row the `CHECK`s forbid.
pub async fn detail(
    conn: &mut PgConnection,
    reader: Uuid,
    id: i32,
) -> Result<Option<PvpFightDetail>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT f.id, ua.display_name, ud.display_name, f.spec_a, f.spec_b, f.seed, f.log, \
                f.ruleset_version, f.winner, f.end_reason, f.look_a::text, f.look_b::text, \
                (extract(epoch FROM f.created_at) * 1000)::bigint \
         FROM friend_fights f JOIN users ua ON ua.id = f.attacker_id \
         JOIN users ud ON ud.id = f.defender_id \
         WHERE f.id = $1 AND (f.attacker_id = $2 OR f.defender_id = $2 \
           OR EXISTS (SELECT 1 FROM friend_tournament_slots s \
                      WHERE s.tournament_id = f.tournament_id AND s.user_id = $2))",
    )
    .bind(id)
    .bind(reader)
    .fetch_optional(conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let seed: Vec<u8> = row.try_get(5)?;
    Ok(Some(PvpFightDetail {
        id: u32::try_from(row.try_get::<i32, _>(0)?).map_err(|_| bad())?,
        a: PvpFighter::Player {
            display_name: row.try_get(1)?,
        },
        b: PvpFighter::Player {
            display_name: row.try_get(2)?,
        },
        spec_a: text(&row, 3)?,
        spec_b: text(&row, 4)?,
        seed: crate::fights::hex(&seed),
        log: text(&row, 6)?,
        ruleset_version: u16::try_from(row.try_get::<i16, _>(7)?).map_err(|_| bad())?,
        winner: side_of(row.try_get(8)?)?,
        end_reason: reason_of(row.try_get(9)?)?,
        rating_delta: 0,
        look_a: look_of(&row, 10)?,
        look_b: look_of(&row, 11)?,
        created_at: TimestampMs::from_ms(row.try_get(12)?),
    }))
}

/// A page of the challenges of `user_id`, the newest first. Never a match of a bracket: how many
/// one played would say how far one went, which the bracket hides until asked (011 RF20); those
/// are reached from their tournament.
///
/// # Errors
/// The database error, or a row the `CHECK`s forbid.
pub async fn page(
    conn: &mut PgConnection,
    user_id: Uuid,
    before: Option<i32>,
) -> Result<FriendFightsPage, sqlx::Error> {
    let limit = i64::try_from(PAGE_SIZE + 1).unwrap_or(i64::MAX);
    let rows = sqlx::query(
        "SELECT f.id, f.attacker_id = $1, ua.display_name, ud.display_name, f.winner, \
                f.end_reason, (extract(epoch FROM f.created_at) * 1000)::bigint \
         FROM friend_fights f JOIN users ua ON ua.id = f.attacker_id \
         JOIN users ud ON ud.id = f.defender_id \
         WHERE f.kind = 'challenge' AND (f.attacker_id = $1 OR f.defender_id = $1) \
           AND ($2::int IS NULL OR f.id < $2) \
         ORDER BY f.id DESC LIMIT $3",
    )
    .bind(user_id)
    .bind(before)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    let more = rows.len() > PAGE_SIZE;
    let mut fights = Vec::with_capacity(PAGE_SIZE);
    for row in rows.iter().take(PAGE_SIZE) {
        let attacker: bool = row.try_get(1)?;
        fights.push(FriendFightSummary {
            id: u32::try_from(row.try_get::<i32, _>(0)?).map_err(|_| bad())?,
            role: if attacker {
                PvpRole::Attacker
            } else {
                PvpRole::Defender
            },
            opponent: row.try_get(if attacker { 3 } else { 2 })?,
            winner: side_of(row.try_get(4)?)?,
            end_reason: reason_of(row.try_get(5)?)?,
            created_at: TimestampMs::from_ms(row.try_get(6)?),
        });
    }
    let next_before = if more {
        fights.last().map(|f| f.id)
    } else {
        None
    };
    Ok(FriendFightsPage {
        fights,
        next_before,
    })
}

/// The tournaments of `user_id` today (UTC).
///
/// # Errors
/// The database error.
pub async fn tournaments_today(tx: &mut PgConnection, user_id: Uuid) -> Result<i16, sqlx::Error> {
    let n: Option<i16> = sqlx::query_scalar(
        "SELECT tournaments FROM friend_daily \
         WHERE user_id = $1 AND day = (now() AT TIME ZONE 'UTC')::date",
    )
    .bind(user_id)
    .fetch_optional(tx)
    .await?;
    Ok(n.unwrap_or(0))
}

/// One more tournament of `user_id` today.
///
/// # Errors
/// The database error (the `CHECK` of the cap included).
pub async fn count_tournament(tx: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO friend_daily (user_id, day, tournaments) \
         VALUES ($1, (now() AT TIME ZONE 'UTC')::date, 1) \
         ON CONFLICT (user_id, day) DO UPDATE SET tournaments = friend_daily.tournaments + 1",
    )
    .bind(user_id)
    .execute(tx)
    .await?;
    Ok(())
}

/// The tournament `owner` already made with `client_key`.
///
/// # Errors
/// The database error.
pub async fn tournament_by_key(
    tx: &mut PgConnection,
    owner: Uuid,
    client_key: Uuid,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM friend_tournaments WHERE owner_id = $1 AND client_key = $2")
        .bind(owner)
        .bind(client_key)
        .fetch_optional(tx)
        .await
}

/// Writes a tournament and its slots, `players` in the order of the bracket, and returns its id.
///
/// # Errors
/// The database error.
pub async fn insert_tournament(
    tx: &mut PgConnection,
    owner: Uuid,
    client_key: Uuid,
    players: &[Uuid],
) -> Result<i32, sqlx::Error> {
    let size = i16::try_from(players.len()).map_err(|_| bad())?;
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO friend_tournaments (owner_id, size, client_key) VALUES ($1, $2, $3) \
         RETURNING id",
    )
    .bind(owner)
    .bind(size)
    .bind(client_key)
    .fetch_one(&mut *tx)
    .await?;
    let slots: Vec<i16> = (0..size).collect();
    sqlx::query(
        "INSERT INTO friend_tournament_slots (tournament_id, slot, user_id) \
         SELECT $1, * FROM UNNEST($2::smallint[], $3::uuid[])",
    )
    .bind(id)
    .bind(&slots)
    .bind(players)
    .execute(&mut *tx)
    .await?;
    Ok(id)
}

/// A match of a bracket, as written. `attacker_id` is side `a`.
pub struct NewMatch<'a> {
    pub tournament_id: i32,
    pub round: u8,
    pub slot: u8,
    pub attacker_id: Uuid,
    pub defender_id: Uuid,
    pub ruleset_version: u16,
    pub fight: &'a crate::pvp::resolve::Resolved,
    pub look_a: &'a PvpLook,
    pub look_b: &'a PvpLook,
    pub advances: Side,
}

/// Writes a match of a bracket and returns its id.
///
/// # Errors
/// The database error (the `CHECK` of `advances` included).
pub async fn insert_match(tx: &mut PgConnection, m: &NewMatch<'_>) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO friend_fights (kind, attacker_id, defender_id, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, look_a, look_b, tournament_id, round, slot, \
         advances) \
         VALUES ('bracket', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb, $11::jsonb, $12, $13, \
                 $14, $15) \
         RETURNING id",
    )
    .bind(m.attacker_id)
    .bind(m.defender_id)
    .bind(i16::try_from(m.ruleset_version).map_err(|_| bad())?)
    .bind(&m.fight.spec_a)
    .bind(&m.fight.spec_b)
    .bind(m.fight.seed.as_slice())
    .bind(&m.fight.log)
    .bind(m.fight.winner.map(side_code))
    .bind(reason_code(m.fight.end_reason))
    .bind(look_json(m.look_a)?)
    .bind(look_json(m.look_b)?)
    .bind(m.tournament_id)
    .bind(i16::from(m.round))
    .bind(i16::from(m.slot))
    .bind(side_code(m.advances))
    .fetch_one(tx)
    .await
}

fn small(value: i16) -> Result<u8, sqlx::Error> {
    u8::try_from(value).map_err(|_| bad())
}

fn id_of(value: i32) -> Result<u32, sqlx::Error> {
    u32::try_from(value).map_err(|_| bad())
}

/// Tournament `id` for `reader`, who must hold one of its slots: `None` otherwise, as for a
/// tournament that does not exist.
///
/// # Errors
/// The database error, or rows the `CHECK`s forbid (a bracket without its final, say).
pub async fn tournament(
    conn: &mut PgConnection,
    reader: Uuid,
    id: i32,
) -> Result<Option<FriendTournament>, sqlx::Error> {
    let head = sqlx::query(
        "SELECT t.size, u.display_name, (extract(epoch FROM t.created_at) * 1000)::bigint \
         FROM friend_tournaments t JOIN users u ON u.id = t.owner_id \
         WHERE t.id = $1 AND EXISTS (SELECT 1 FROM friend_tournament_slots s \
                                     WHERE s.tournament_id = t.id AND s.user_id = $2)",
    )
    .bind(id)
    .bind(reader)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(head) = head else {
        return Ok(None);
    };
    let players: Vec<String> = sqlx::query_scalar(
        "SELECT u.display_name FROM friend_tournament_slots s JOIN users u ON u.id = s.user_id \
         WHERE s.tournament_id = $1 ORDER BY s.slot",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let rows = sqlx::query(
        "SELECT f.round, f.slot, ua.display_name, ud.display_name, f.id, f.advances \
         FROM friend_fights f JOIN users ua ON ua.id = f.attacker_id \
         JOIN users ud ON ud.id = f.defender_id \
         WHERE f.tournament_id = $1 ORDER BY f.round, f.slot",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let mut matches = Vec::with_capacity(rows.len());
    for row in &rows {
        matches.push(BracketMatch {
            round: small(row.try_get(0)?)?,
            slot: small(row.try_get(1)?)?,
            a: row.try_get(2)?,
            b: row.try_get(3)?,
            fight_id: id_of(row.try_get(4)?)?,
            advances: side_of(row.try_get(5)?)?.ok_or_else(bad)?,
        });
    }
    let last = matches.last().ok_or_else(bad)?;
    let champion = match last.advances {
        Side::A => last.a.clone(),
        Side::B => last.b.clone(),
    };
    Ok(Some(FriendTournament {
        id: id_of(id)?,
        size: small(head.try_get(0)?)?,
        owner: head.try_get(1)?,
        players,
        matches,
        champion,
        created_at: TimestampMs::from_ms(head.try_get(2)?),
    }))
}

/// A page of the tournaments `user_id` played, the newest first.
///
/// # Errors
/// The database error, or a row the `CHECK`s forbid.
pub async fn tournaments_page(
    conn: &mut PgConnection,
    user_id: Uuid,
    before: Option<i32>,
) -> Result<FriendTournamentsPage, sqlx::Error> {
    let limit = i64::try_from(PAGE_SIZE + 1).unwrap_or(i64::MAX);
    // The champion is the side that advances from the last match (the final).
    let rows = sqlx::query(
        "SELECT t.id, t.size, uo.display_name, \
                CASE fin.advances WHEN 'a' THEN ua.display_name ELSE ud.display_name END, \
                (extract(epoch FROM t.created_at) * 1000)::bigint \
         FROM friend_tournament_slots s JOIN friend_tournaments t ON t.id = s.tournament_id \
         JOIN users uo ON uo.id = t.owner_id \
         JOIN LATERAL (SELECT f.attacker_id, f.defender_id, f.advances FROM friend_fights f \
                       WHERE f.tournament_id = t.id ORDER BY f.round DESC LIMIT 1) fin ON true \
         JOIN users ua ON ua.id = fin.attacker_id JOIN users ud ON ud.id = fin.defender_id \
         WHERE s.user_id = $1 AND ($2::int IS NULL OR t.id < $2) \
         ORDER BY t.id DESC LIMIT $3",
    )
    .bind(user_id)
    .bind(before)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    let more = rows.len() > PAGE_SIZE;
    let mut tournaments = Vec::with_capacity(PAGE_SIZE);
    for row in rows.iter().take(PAGE_SIZE) {
        tournaments.push(FriendTournamentSummary {
            id: id_of(row.try_get(0)?)?,
            size: small(row.try_get(1)?)?,
            owner: row.try_get(2)?,
            champion: row.try_get(3)?,
            created_at: TimestampMs::from_ms(row.try_get(4)?),
        });
    }
    let next_before = if more {
        tournaments.last().map(|t| t.id)
    } else {
        None
    };
    Ok(FriendTournamentsPage {
        tournaments,
        next_before,
    })
}
