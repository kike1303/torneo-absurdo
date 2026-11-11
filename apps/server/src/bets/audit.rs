//! The `auth_events` of the bets (ADR-0008 D8, 012 RF25, RF29, RF30, plan-b decision 2): `grant` and
//! `bailout` with their ledger entry, `bet` with its bet, and the `pvp_attack` of 021 with its
//! fight. Like `reauth`, `link` and `unlink`, `bets/` writes its own, because `login::event` has no
//! `bet_id` and is not of this domain.

use sqlx::PgConnection;
use uuid::Uuid;

/// What an event points to.
#[derive(Debug, Clone, Copy)]
pub enum Event {
    Grant {
        entry: Uuid,
    },
    Bailout {
        entry: Uuid,
    },
    Bet {
        bet: Uuid,
    },
    /// An attack of a player fight (021 step 5).
    PvpAttack {
        fight: i32,
    },
}

/// One row of `auth_events`, inside the transaction of the change.
///
/// # Errors
/// The database error.
pub async fn record(
    tx: &mut PgConnection,
    user_id: Uuid,
    event: Event,
    ip_hash: &[u8; 32],
) -> Result<(), sqlx::Error> {
    let (kind, entry, bet, fight) = match event {
        Event::Grant { entry } => ("grant", Some(entry), None, None),
        Event::Bailout { entry } => ("bailout", Some(entry), None, None),
        Event::Bet { bet } => ("bet", None, Some(bet), None),
        Event::PvpAttack { fight } => ("pvp_attack", None, None, Some(fight)),
    };
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, ledger_entry_id, bet_id, pvp_fight_id, ip_hash) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(kind)
    .bind(entry)
    .bind(bet)
    .bind(fight)
    .bind(ip_hash.as_slice())
    .execute(tx)
    .await?;
    Ok(())
}
