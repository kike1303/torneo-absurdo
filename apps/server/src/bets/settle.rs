//! The real settlement hook (012 RF7, RF8, RF9), inside the transaction of the reveal. It decides
//! only with the final and the rows of `bets`: nothing a user does after betting changes what it
//! writes (RF14), so it reads no account state and takes no lock on a person.
//!
//! In phase A there is no split yet: a decided fight with stakes on both sides returns an error, which
//! ends in a halt, never in a fight settled without its entries. That case has no route without the
//! API of phase B.

use protocol::{FightFinal, Side};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use crate::ledger::{self, Coins, SettleEntry, SettleKind};

/// The kind of the error of a decided fight with both sides in phase A.
pub const PAYOUT_UNAVAILABLE: &str = "payout_unavailable";

/// What a pool ends in (RF9): the value of `pool_settlements.outcome`, or the split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolResult {
    Split,
    RefundOneSided,
    RefundDraw,
    RefundVoided,
    RefundCancelled,
}

impl PoolResult {
    /// The table of RF9. Pure: only the final and the two sums of the pool.
    #[must_use]
    pub const fn of(final_result: FightFinal, pool_a: i64, pool_b: i64) -> Self {
        match final_result {
            FightFinal::Decided { winner: None, .. } => Self::RefundDraw,
            FightFinal::Decided {
                winner: Some(_), ..
            } => {
                if pool_a > 0 && pool_b > 0 {
                    Self::Split
                } else {
                    Self::RefundOneSided
                }
            }
            FightFinal::Voided { .. } => Self::RefundVoided,
            FightFinal::Cancelled { .. } => Self::RefundCancelled,
        }
    }

    const fn refund_code(self) -> Option<&'static str> {
        match self {
            Self::Split => None,
            Self::RefundOneSided => Some("refund_one_sided"),
            Self::RefundDraw => Some("refund_draw"),
            Self::RefundVoided => Some("refund_voided"),
            Self::RefundCancelled => Some("refund_cancelled"),
        }
    }
}

struct PlacedBet {
    id: Uuid,
    user_id: Uuid,
    side: Side,
    stake: i64,
}

/// The hook of `app::serve` and of the CLI of `fight void`. It needs no configuration: a refund
/// charges no rake, and so it does not read one.
pub struct BetsHook;

impl SettlementHook for BetsHook {
    fn settle<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        Box::pin(settle(tx, fight))
    }
}

/// A row that a `CHECK` of the table forbids.
const fn bad_row() -> SettlementError {
    SettlementError {
        kind: "row_invalid",
    }
}

async fn placed_bets(
    tx: &mut PgConnection,
    fight_id: i32,
) -> Result<Vec<PlacedBet>, SettlementError> {
    let rows =
        sqlx::query("SELECT id, user_id, side, stake FROM bets WHERE fight_id = $1 ORDER BY id")
            .bind(fight_id)
            .fetch_all(&mut *tx)
            .await?;
    rows.iter()
        .map(|row| {
            let side: String = row.try_get(2)?;
            let side = match side.as_str() {
                "a" => Side::A,
                "b" => Side::B,
                _ => return Err(bad_row()),
            };
            Ok(PlacedBet {
                id: row.try_get(0)?,
                user_id: row.try_get(1)?,
                side,
                stake: row.try_get(3)?,
            })
        })
        .collect()
}

async fn settle(tx: &mut PgConnection, fight: &SettlingFight) -> Result<(), SettlementError> {
    let bets = placed_bets(&mut *tx, fight.id).await?;
    if bets.is_empty() {
        return Ok(());
    }
    let mut pool_a = 0_i64;
    let mut pool_b = 0_i64;
    for bet in &bets {
        let pool = match bet.side {
            Side::A => &mut pool_a,
            Side::B => &mut pool_b,
        };
        *pool = pool.checked_add(bet.stake).ok_or_else(bad_row)?;
    }
    let Some(code) = PoolResult::of(fight.final_result, pool_a, pool_b).refund_code() else {
        return Err(SettlementError {
            kind: PAYOUT_UNAVAILABLE,
        });
    };
    // RF8: the first write. Without a row it was already settled, and nothing else is written.
    let first = sqlx::query(
        "INSERT INTO pool_settlements (fight_id, outcome, pool_a, pool_b, rake_bp, rake, residue) \
         VALUES ($1, $2, $3, $4, 0, 0, 0) ON CONFLICT (fight_id) DO NOTHING RETURNING fight_id",
    )
    .bind(fight.id)
    .bind(code)
    .bind(pool_a)
    .bind(pool_b)
    .fetch_optional(&mut *tx)
    .await?;
    if first.is_none() {
        return Ok(());
    }
    let entries: Vec<SettleEntry> = bets
        .iter()
        .map(|bet| SettleEntry {
            kind: SettleKind::Refund,
            bet_id: bet.id,
            user_id: bet.user_id,
            amount: Coins(bet.stake),
        })
        .collect();
    ledger::settle_batch(&mut *tx, fight.id, &entries).await?;
    for batch in bets.chunks(ledger::SETTLE_BATCH_ROWS) {
        let ids: Vec<Uuid> = batch.iter().map(|bet| bet.id).collect();
        let owners: Vec<Uuid> = batch.iter().map(|bet| bet.user_id).collect();
        let stakes: Vec<i64> = batch.iter().map(|bet| bet.stake).collect();
        sqlx::query(
            "INSERT INTO bet_settlements \
             (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
             SELECT t.bet_id, $1, t.user_id, 'refunded', t.stake, 0, 0 \
             FROM UNNEST($2::uuid[], $3::uuid[], $4::bigint[]) AS t(bet_id, user_id, stake)",
        )
        .bind(fight.id)
        .bind(&ids)
        .bind(&owners)
        .bind(&stakes)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// How many bets a fight refunded, and how many coins: what `admin_audit` records (RF18).
///
/// # Errors
/// The database error.
pub async fn refund_totals(
    tx: &mut PgConnection,
    fight_id: i32,
) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query(
        "SELECT count(*), COALESCE(SUM(payout), 0)::bigint FROM bet_settlements \
         WHERE fight_id = $1 AND outcome = 'refunded'",
    )
    .bind(fight_id)
    .fetch_one(tx)
    .await?;
    Ok((row.try_get(0)?, row.try_get(1)?))
}

#[cfg(test)]
mod tests {
    use protocol::{CancelReason, EndReason, VoidReason};

    use super::*;

    const KO_A: FightFinal = FightFinal::Decided {
        winner: Some(Side::A),
        reason: EndReason::Ko,
    };
    const KO_B: FightFinal = FightFinal::Decided {
        winner: Some(Side::B),
        reason: EndReason::Ko,
    };
    const DRAW: FightFinal = FightFinal::Decided {
        winner: None,
        reason: EndReason::Draw,
    };

    /// The table of RF9, row by row.
    #[test]
    fn the_table_of_rf9() {
        assert_eq!(PoolResult::of(KO_A, 5, 5), PoolResult::Split);
        assert_eq!(PoolResult::of(KO_A, 5, 0), PoolResult::RefundOneSided);
        assert_eq!(PoolResult::of(KO_A, 0, 5), PoolResult::RefundOneSided);
        assert_eq!(PoolResult::of(KO_B, 5, 0), PoolResult::RefundOneSided);
        assert_eq!(PoolResult::of(DRAW, 5, 5), PoolResult::RefundDraw);
        assert_eq!(PoolResult::of(DRAW, 5, 0), PoolResult::RefundDraw);
        for reason in [
            VoidReason::SnapshotInvalid,
            VoidReason::EngineError,
            VoidReason::SecretLost,
        ] {
            let voided = FightFinal::Voided { reason };
            assert_eq!(PoolResult::of(voided, 5, 5), PoolResult::RefundVoided);
        }
        let cancelled = FightFinal::Cancelled {
            reason: CancelReason::MissedWindow,
        };
        assert_eq!(PoolResult::of(cancelled, 5, 5), PoolResult::RefundCancelled);
    }
}
