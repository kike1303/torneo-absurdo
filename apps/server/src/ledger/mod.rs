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
