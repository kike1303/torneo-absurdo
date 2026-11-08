//! The facts of one account, in **one statement** on the connection it is given (012 RF30, RF32,
//! plan-b D2): the balance and the open stakes must come from the same snapshot, or a settlement
//! committing between two reads would show coins that are nowhere. `wallet`, `bonus`, `bailout` and
//! `POST /api/bets` decide with this same function, so they never disagree (RF32).

use protocol::{BailoutState, CoinAmount, Probation, TimestampMs, Wallet};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::{BetsConfig, probation};
use crate::ledger::Coins;

/// One account at one instant, with the database's `now()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub balance: Coins,
    /// Stakes of fights that are not `settled`: a halted fight still counts (010 RF16).
    pub open_stakes: Coins,
    /// Today's (UTC) `grant` exists.
    pub granted_today: bool,
    /// Today's (UTC) `bailout` exists.
    pub bailed_today: bool,
    pub age_ms: i64,
    /// Distinct fights with a bet won or lost (RF27): refunds do not count.
    pub settled_fights: i64,
    /// The next UTC midnight, in milliseconds since the epoch.
    pub day_resets_at_ms: i64,
}

/// The age of the account, in ms by the database's `now()`: `NULL` if the user does not exist.
macro_rules! age_ms_sql {
    () => {
        "(SELECT (EXTRACT(EPOCH FROM now() - u.created_at) * 1000)::bigint FROM users u \
           WHERE u.id = $1)"
    };
}

/// Distinct fights with a bet won or lost (RF27), by the partial index.
macro_rules! settled_fights_sql {
    () => {
        "(SELECT count(DISTINCT fight_id) FROM bet_settlements \
           WHERE user_id = $1 AND outcome <> 'refunded')"
    };
}

/// The single statement. Every sub-select runs in the same snapshot, and `now()` is the one of the
/// transaction.
const FACTS_SQL: &str = concat!(
    "SELECT \
      (SELECT COALESCE(SUM(p.amount), 0)::bigint FROM ledger_postings p \
         JOIN ledger_accounts a ON a.id = p.account_id WHERE a.user_id = $1), \
      (SELECT COALESCE(SUM(b.stake), 0)::bigint FROM bets b \
         JOIN fights f ON f.id = b.fight_id WHERE b.user_id = $1 AND f.state <> 'settled'), \
      EXISTS (SELECT 1 FROM ledger_entries WHERE user_id = $1 AND kind = 'grant' \
         AND grant_day = (now() AT TIME ZONE 'UTC')::date), \
      EXISTS (SELECT 1 FROM ledger_entries WHERE user_id = $1 AND kind = 'bailout' \
         AND grant_day = (now() AT TIME ZONE 'UTC')::date), ",
    age_ms_sql!(),
    ", ",
    settled_fights_sql!(),
    ", (EXTRACT(EPOCH FROM ((now() AT TIME ZONE 'UTC')::date + 1)::timestamp \
         AT TIME ZONE 'UTC') * 1000)::bigint"
);

/// Only what probation needs, with the same fragments as [`FACTS_SQL`], so the two never disagree.
const PROBATION_SQL: &str = concat!("SELECT ", age_ms_sql!(), ", ", settled_fights_sql!());

/// The facts of `user_id`, or `None` if the user does not exist.
///
/// # Errors
/// The database error.
pub async fn read(conn: &mut PgConnection, user_id: Uuid) -> Result<Option<Facts>, sqlx::Error> {
    let row = sqlx::query(FACTS_SQL)
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await?;
    let Some(age_ms) = row.try_get::<Option<i64>, _>(4)? else {
        return Ok(None);
    };
    Ok(Some(Facts {
        balance: Coins(row.try_get(0)?),
        open_stakes: Coins(row.try_get(1)?),
        granted_today: row.try_get(2)?,
        bailed_today: row.try_get(3)?,
        age_ms,
        settled_fights: row.try_get(5)?,
        day_resets_at_ms: row.try_get(6)?,
    }))
}

/// Only probation (RF27), for the reserve of the sockets (RF39): the age and the settled fights, by
/// the partial index, without summing the ledger. `None` if the user does not exist.
///
/// # Errors
/// The database error.
pub async fn probation_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<Option<Probation>>, sqlx::Error> {
    let row = sqlx::query(PROBATION_SQL)
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await?;
    let Some(age_ms) = row.try_get::<Option<i64>, _>(0)? else {
        return Ok(None);
    };
    Ok(Some(probation::of(age_ms, row.try_get(1)?)))
}

impl Facts {
    #[must_use]
    pub fn probation(&self) -> Option<Probation> {
        probation::of(self.age_ms, self.settled_fights)
    }

    /// What a bailout claim would answer, in the order of RF30.
    #[must_use]
    pub fn bailout(&self, config: BetsConfig) -> BailoutState {
        let holding = self.balance.0.saturating_add(self.open_stakes.0);
        if !self.granted_today {
            BailoutState::GrantFirst
        } else if holding >= config.min_bet.0 {
            BailoutState::NotEligible
        } else if self.bailed_today {
            BailoutState::AlreadyClaimed
        } else {
            BailoutState::Available
        }
    }

    /// The cap of one fight for this account (RF27).
    #[must_use]
    pub fn fight_cap(&self, config: BetsConfig) -> Coins {
        config.fight_cap(self.probation().is_some())
    }

    /// `GET /api/wallet` (RF32). `None` only for a figure a JavaScript number would lose.
    #[must_use]
    pub fn wallet(&self, config: BetsConfig) -> Option<Wallet> {
        Some(Wallet {
            balance: CoinAmount::new(self.balance.0)?,
            open_stakes: CoinAmount::new(self.open_stakes.0)?,
            bonus_available: !self.granted_today,
            bailout: self.bailout(config),
            bonus_amount: CoinAmount::new(config.grant.0)?,
            bailout_amount: CoinAmount::new(config.bailout().0)?,
            min_bet: CoinAmount::new(config.min_bet.0)?,
            fight_cap: CoinAmount::new(self.fight_cap(config).0)?,
            day_resets_at: TimestampMs::from_ms(self.day_resets_at_ms),
            probation: self.probation(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: BetsConfig = BetsConfig {
        grant: Coins(500),
        min_bet: Coins(25),
        cap_multiple: 5,
    };

    fn facts() -> Facts {
        Facts {
            balance: Coins(0),
            open_stakes: Coins(0),
            granted_today: true,
            bailed_today: false,
            age_ms: probation::PROBATION_MS,
            settled_fights: probation::PROBATION_FIGHTS,
            day_resets_at_ms: 86_400_000,
        }
    }

    /// RF30 and RF32: one row per value of `bailout`, in its order.
    #[test]
    fn the_bailout_follows_the_order_of_rf30() {
        assert_eq!(facts().bailout(CONFIG), BailoutState::Available);
        let no_grant = Facts {
            granted_today: false,
            bailed_today: true,
            ..facts()
        };
        assert_eq!(no_grant.bailout(CONFIG), BailoutState::GrantFirst);
        // Test 8: an open bet that leaves the balance at 0 is not being broke.
        let staked = Facts {
            open_stakes: Coins(25),
            ..facts()
        };
        assert_eq!(staked.bailout(CONFIG), BailoutState::NotEligible);
        let just_under = Facts {
            balance: Coins(24),
            ..facts()
        };
        assert_eq!(just_under.bailout(CONFIG), BailoutState::Available);
        let claimed = Facts {
            bailed_today: true,
            ..facts()
        };
        assert_eq!(claimed.bailout(CONFIG), BailoutState::AlreadyClaimed);
    }

    #[test]
    fn the_wallet_carries_the_caps_of_probation() {
        let out = facts().wallet(CONFIG).unwrap();
        assert_eq!(out.fight_cap.get(), 2500);
        assert_eq!(out.probation, None);
        assert!(!out.bonus_available);
        assert_eq!(out.bailout_amount.get(), 250);
        let new = Facts {
            age_ms: 0,
            granted_today: false,
            ..facts()
        };
        let wallet = new.wallet(CONFIG).unwrap();
        assert_eq!(wallet.fight_cap.get(), 500);
        assert_eq!(wallet.probation.map(|p| p.days_left), Some(7));
        assert!(wallet.bonus_available);
        assert_eq!(wallet.day_resets_at, TimestampMs::from_ms(86_400_000));
    }
}
