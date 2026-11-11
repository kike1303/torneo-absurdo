//! The double-entry ledger (ADR-0005, 008 RF-B2): accounts, entries and postings. Coins only
//! move through entries; there is no balance column and no function here updates or deletes a
//! row (the database refuses it anyway, `TA001`). Every entry balances at commit (`TA002`).

use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

/// An amount of coins. Integer, never floating point (ADR-0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coins(pub i64);

/// The single issuance account, created by the migration: the source of every grant.
pub const ISSUANCE_ACCOUNT: Uuid = Uuid::from_u128(1);
/// What the open bets hold (012 RF1): only a `bet_stake` credits it (RF2).
pub const ESCROW_ACCOUNT: Uuid = Uuid::from_u128(2);
/// Where the rake and the residue go, and never come back from (012 RF1, RF10).
pub const RAKE_SINK_ACCOUNT: Uuid = Uuid::from_u128(3);

/// The ledger account of a new user, in the transaction of its first login.
pub async fn create_user_account(tx: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO ledger_accounts (id, kind, user_id) VALUES ($1, 'user', $2)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// The `grant` of today (UTC) for `user_id`: issuance −`amount`, user +`amount`. `None` if the
/// user already has today's grant: the partial unique index decides, never a prior check.
pub async fn grant(
    tx: &mut PgConnection,
    user_id: Uuid,
    amount: Coins,
) -> Result<Option<Uuid>, sqlx::Error> {
    let inserted = sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id) VALUES ($1, 'grant', $2) \
         ON CONFLICT (user_id, grant_day) WHERE kind = 'grant' DO NOTHING RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = inserted else {
        return Ok(None);
    };
    let entry: Uuid = row.try_get(0)?;
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT $1, $2, $3 \
         UNION ALL \
         SELECT $1, id, $4 FROM ledger_accounts WHERE user_id = $5",
    )
    .bind(entry)
    .bind(ISSUANCE_ACCOUNT)
    .bind(-amount.0)
    .bind(amount.0)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    Ok(Some(entry))
}

/// The `bailout` of today (UTC) for `user_id` (012 RF3): issuance −`amount`, user +`amount`. `None`
/// if the user already has today's: the partial unique index decides, never a prior check.
pub async fn bailout(
    tx: &mut PgConnection,
    user_id: Uuid,
    amount: Coins,
) -> Result<Option<Uuid>, sqlx::Error> {
    let inserted = sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id) VALUES ($1, 'bailout', $2) \
         ON CONFLICT (user_id, grant_day) WHERE kind = 'bailout' DO NOTHING RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = inserted else {
        return Ok(None);
    };
    let entry: Uuid = row.try_get(0)?;
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT $1, $2, $3 \
         UNION ALL \
         SELECT $1, id, $4 FROM ledger_accounts WHERE user_id = $5",
    )
    .bind(entry)
    .bind(ISSUANCE_ACCOUNT)
    .bind(-amount.0)
    .bind(amount.0)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    Ok(Some(entry))
}

/// The stake of a bet (012 RF25): user −`amount`, escrow +`amount`, one per bet (RF3).
pub async fn bet_stake(
    tx: &mut PgConnection,
    bet_id: Uuid,
    fight_id: i32,
    user_id: Uuid,
    amount: Coins,
) -> Result<Uuid, sqlx::Error> {
    let entry = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, fight_id, bet_id) \
         VALUES ($1, 'bet_stake', $2, $3, $4)",
    )
    .bind(entry)
    .bind(user_id)
    .bind(fight_id)
    .bind(bet_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT $1, $2, $3 \
         UNION ALL \
         SELECT $1, id, $4 FROM ledger_accounts WHERE user_id = $5",
    )
    .bind(entry)
    .bind(ESCROW_ACCOUNT)
    .bind(amount.0)
    .bind(-amount.0)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    Ok(entry)
}

/// The `rake` of a fight (012 RF10): escrow −`amount`, rake sink +`amount`, with no user, once per
/// fight (RF3). `amount` is `rake + residue`, and must be positive (`ledger_postings_amount_ck`).
pub async fn rake(
    tx: &mut PgConnection,
    fight_id: i32,
    amount: Coins,
) -> Result<Uuid, sqlx::Error> {
    let entry = Uuid::new_v4();
    sqlx::query("INSERT INTO ledger_entries (id, kind, fight_id) VALUES ($1, 'rake', $2)")
        .bind(entry)
        .bind(fight_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, $3), ($1, $4, $5)",
    )
    .bind(entry)
    .bind(ESCROW_ACCOUNT)
    .bind(-amount.0)
    .bind(RAKE_SINK_ACCOUNT)
    .bind(amount.0)
    .execute(&mut *tx)
    .await?;
    Ok(entry)
}

/// The coins of an attack (021 "Buscar pelea" step 5): issuance −`amount`, attacker +`amount`, once
/// per fight (`ledger_entries_pvp_reward_uq`). `amount` must be the `coins_attacker` of the fight and
/// positive: the database refuses anything else (`TA008`), and the caller writes no entry for 0.
pub async fn pvp_reward(
    tx: &mut PgConnection,
    pvp_fight_id: i32,
    user_id: Uuid,
    amount: Coins,
) -> Result<Uuid, sqlx::Error> {
    let entry = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, pvp_fight_id) VALUES ($1, 'pvp_reward', $2, $3)",
    )
    .bind(entry)
    .bind(user_id)
    .bind(pvp_fight_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT $1, $2, $3 \
         UNION ALL \
         SELECT $1, id, $4 FROM ledger_accounts WHERE user_id = $5",
    )
    .bind(entry)
    .bind(ISSUANCE_ACCOUNT)
    .bind(-amount.0)
    .bind(amount.0)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    Ok(entry)
}

/// Rows of each batch of [`settle_batch`] (012 RF2).
pub const SETTLE_BATCH_ROWS: usize = 1000;

/// What a settlement moves out of the escrow for a bet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleKind {
    Refund,
    Payout,
}

impl SettleKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Refund => "refund",
            Self::Payout => "bet_payout",
        }
    }
}

/// One entry of [`settle_batch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettleEntry {
    pub kind: SettleKind,
    pub bet_id: Uuid,
    pub user_id: Uuid,
    pub amount: Coins,
}

/// One entry per bet (escrow −`amount`, user +`amount`), by batches of [`SETTLE_BATCH_ROWS`]: a
/// settlement in its limit has 20 000 of them, inside the transaction of the reveal (012 RF15).
pub async fn settle_batch(
    tx: &mut PgConnection,
    fight_id: i32,
    entries: &[SettleEntry],
) -> Result<(), sqlx::Error> {
    for batch in entries.chunks(SETTLE_BATCH_ROWS) {
        let ids: Vec<Uuid> = batch.iter().map(|_| Uuid::new_v4()).collect();
        let kinds: Vec<&str> = batch.iter().map(|e| e.kind.as_str()).collect();
        let users: Vec<Uuid> = batch.iter().map(|e| e.user_id).collect();
        let bets: Vec<Uuid> = batch.iter().map(|e| e.bet_id).collect();
        let amounts: Vec<i64> = batch.iter().map(|e| e.amount.0).collect();
        sqlx::query(
            "INSERT INTO ledger_entries (id, kind, user_id, fight_id, bet_id) \
             SELECT t.id, t.kind, t.user_id, $1, t.bet_id \
             FROM UNNEST($2::uuid[], $3::text[], $4::uuid[], $5::uuid[]) \
             AS t(id, kind, user_id, bet_id)",
        )
        .bind(fight_id)
        .bind(&ids)
        .bind(&kinds)
        .bind(&users)
        .bind(&bets)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) \
             SELECT t.id, $1, -t.amount FROM UNNEST($2::uuid[], $3::bigint[]) AS t(id, amount) \
             UNION ALL \
             SELECT t.id, a.id, t.amount \
             FROM UNNEST($2::uuid[], $3::bigint[], $4::uuid[]) AS t(id, amount, user_id) \
             JOIN ledger_accounts a ON a.user_id = t.user_id",
        )
        .bind(ESCROW_ACCOUNT)
        .bind(&ids)
        .bind(&amounts)
        .bind(&users)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// The balance of `user_id`: the sum of its postings (0 without any).
pub async fn balance(pool: &PgPool, user_id: Uuid) -> Result<Coins, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM(p.amount), 0)::bigint FROM ledger_postings p \
         JOIN ledger_accounts a ON a.id = p.account_id WHERE a.user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(Coins(row.try_get(0)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_issuance_account_is_the_one_of_the_migration() {
        let migration = include_str!("../../migrations/20261028000000_auth.sql");
        assert!(migration.contains(&format!("'{ISSUANCE_ACCOUNT}', 'issuance'")));
    }

    #[test]
    fn the_accounts_of_the_bets_are_the_ones_of_the_migration() {
        let migration = include_str!("../../migrations/20261102000000_bets.sql");
        assert!(migration.contains(&format!("'{ESCROW_ACCOUNT}', 'bet_escrow'")));
        assert!(migration.contains(&format!("'{RAKE_SINK_ACCOUNT}', 'rake_sink'")));
    }

    /// Only this module writes ledger rows (review of #54): elsewhere there is no
    /// `INSERT INTO ledger_…` at all.
    #[test]
    fn only_the_ledger_module_inserts_ledger_rows() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let here = src.join("ledger");
        let needle = ["INSERT INTO", " LEDGER", "_"].concat();
        let mut stack = vec![src];
        let mut checked = 0;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.starts_with(&here) {
                    continue;
                }
                checked += 1;
                let text = std::fs::read_to_string(&path).unwrap().to_ascii_uppercase();
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(
                    !text.contains(&needle),
                    "{} inserts ledger rows",
                    path.display()
                );
            }
        }
        assert!(checked > 20, "{checked}");
    }

    /// No module updates or deletes ledger rows (the database refuses it, but the code must
    /// not even try).
    #[test]
    fn no_source_updates_or_deletes_the_ledger() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap().to_ascii_uppercase();
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                for verb in ["UPDATE", "DELETE FROM", "TRUNCATE"] {
                    let needle = [verb, " LEDGER", "_"].concat();
                    assert!(!text.contains(&needle), "{} has {needle}", path.display());
                }
            }
        }
    }
}
