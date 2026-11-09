//! The real settlement hook (012 RF7, RF8, RF9), inside the transaction of the reveal. It decides
//! only with the final and the rows of `bets`: nothing a user does after betting changes what it
//! writes (RF14), so it reads no account state and takes no lock on a person.
//!
//! A decided fight with stakes on both sides is split (RF10): everything is computed and checked in
//! memory first, and only then written, starting with `pool_settlements` (RF8, RF13).

use protocol::{FightFinal, Side};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::parimutuel;
use crate::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use crate::ledger::{self, Coins, SettleEntry, SettleKind};

/// The kind of the error of a split that does not add up (RF13): nothing is written.
pub const SPLIT_MISMATCH: &str = "split_mismatch";

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
    ranked_stake: bool,
}

/// The hook of `app::serve` and of the CLI of `fight void`. It needs no configuration: the rake is
/// the fight's own (RF10), and a refund charges none.
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
    let rows = sqlx::query(
        "SELECT id, user_id, side, stake, ranked_stake FROM bets WHERE fight_id = $1 ORDER BY id",
    )
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
                ranked_stake: row.try_get(4)?,
            })
        })
        .collect()
}

/// The pool, then the season (020): `seasons::tally` sums the results just written in the season of
/// the fight, in this same transaction, and marks the fight so nothing sums it twice.
async fn settle(tx: &mut PgConnection, fight: &SettlingFight) -> Result<(), SettlementError> {
    settle_pool(&mut *tx, fight).await?;
    crate::seasons::tally(tx, fight.id).await?;
    Ok(())
}

async fn settle_pool(tx: &mut PgConnection, fight: &SettlingFight) -> Result<(), SettlementError> {
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
    let result = PoolResult::of(fight.final_result, pool_a, pool_b);
    let (
        FightFinal::Decided {
            winner: Some(winner),
            ..
        },
        None,
    ) = (fight.final_result, result.refund_code())
    else {
        return refund(tx, fight, &bets, result, pool_a, pool_b).await;
    };
    pay(tx, fight, &bets, winner, pool_a, pool_b).await
}

/// The split of RF10: computed and checked in memory, then written.
async fn pay(
    tx: &mut PgConnection,
    fight: &SettlingFight,
    bets: &[PlacedBet],
    winner: Side,
    pool_a: i64,
    pool_b: i64,
) -> Result<(), SettlementError> {
    let rake_bp: i16 = sqlx::query_scalar("SELECT rake_bp FROM fights WHERE id = $1")
        .bind(fight.id)
        .fetch_one(&mut *tx)
        .await?;
    let rake_bp = u16::try_from(rake_bp).map_err(|_| bad_row())?;
    let (winners, losers): (Vec<&PlacedBet>, Vec<&PlacedBet>) =
        bets.iter().partition(|bet| bet.side == winner);
    let winning: Vec<i64> = winners.iter().map(|bet| bet.stake).collect();
    let losing = if winner == Side::A { pool_b } else { pool_a };
    let losing_ranked = losers
        .iter()
        .filter(|bet| bet.ranked_stake)
        .try_fold(0_i64, |sum, bet| sum.checked_add(bet.stake))
        .ok_or_else(bad_row)?;
    let split = parimutuel::split(&winning, losing, rake_bp).map_err(|_| bad_row())?;
    // RF13: every coin of the pool goes somewhere, before anything is written.
    if !parimutuel::check_split(&split, &winning, losing) {
        return Err(SettlementError {
            kind: SPLIT_MISMATCH,
        });
    }
    let mut results = Vec::with_capacity(bets.len());
    for (bet, payout) in winners.iter().zip(&split.payouts) {
        let net = payout.checked_sub(bet.stake).ok_or_else(bad_row)?;
        let ranked = parimutuel::ranked_profit(true, *payout, bet.stake, losing, losing_ranked)
            .map_err(|_| bad_row())?;
        results.push((*bet, "won", *payout, net, ranked));
    }
    for bet in &losers {
        let ranked = parimutuel::ranked_profit(false, 0, bet.stake, losing, losing_ranked)
            .map_err(|_| bad_row())?;
        results.push((*bet, "lost", 0, -bet.stake, ranked));
    }
    let sink = split.rake.checked_add(split.residue).ok_or_else(bad_row)?;

    // RF8: the first write. Without a row it was already settled, and nothing else is written.
    let first = sqlx::query(
        "INSERT INTO pool_settlements (fight_id, outcome, pool_a, pool_b, rake_bp, rake, residue) \
         VALUES ($1, 'paid', $2, $3, $4, $5, $6) ON CONFLICT (fight_id) DO NOTHING RETURNING fight_id",
    )
    .bind(fight.id)
    .bind(pool_a)
    .bind(pool_b)
    .bind(i32::from(rake_bp))
    .bind(split.rake)
    .bind(split.residue)
    .fetch_optional(&mut *tx)
    .await?;
    if first.is_none() {
        return Ok(());
    }
    let entries: Vec<SettleEntry> = winners
        .iter()
        .zip(&split.payouts)
        .map(|(bet, payout)| SettleEntry {
            kind: SettleKind::Payout,
            bet_id: bet.id,
            user_id: bet.user_id,
            amount: Coins(*payout),
        })
        .collect();
    ledger::settle_batch(&mut *tx, fight.id, &entries).await?;
    if sink > 0 {
        ledger::rake(&mut *tx, fight.id, Coins(sink)).await?;
    }
    for batch in results.chunks(ledger::SETTLE_BATCH_ROWS) {
        let ids: Vec<Uuid> = batch.iter().map(|r| r.0.id).collect();
        let owners: Vec<Uuid> = batch.iter().map(|r| r.0.user_id).collect();
        let outcomes: Vec<&str> = batch.iter().map(|r| r.1).collect();
        let payouts: Vec<i64> = batch.iter().map(|r| r.2).collect();
        let nets: Vec<i64> = batch.iter().map(|r| r.3).collect();
        let ranked: Vec<i64> = batch.iter().map(|r| r.4).collect();
        sqlx::query(
            "INSERT INTO bet_settlements \
             (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
             SELECT t.bet_id, $1, t.user_id, t.kind, t.payout, t.net, t.ranked \
             FROM UNNEST($2::uuid[], $3::uuid[], $4::text[], $5::bigint[], $6::bigint[], \
             $7::bigint[]) AS t(bet_id, user_id, kind, payout, net, ranked)",
        )
        .bind(fight.id)
        .bind(&ids)
        .bind(&owners)
        .bind(&outcomes)
        .bind(&payouts)
        .bind(&nets)
        .bind(&ranked)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// A refund of every bet (RF9), with no rake.
async fn refund(
    tx: &mut PgConnection,
    fight: &SettlingFight,
    bets: &[PlacedBet],
    result: PoolResult,
    pool_a: i64,
    pool_b: i64,
) -> Result<(), SettlementError> {
    // A split never reaches here; a decided fight with a winner and both sides is paid above.
    let code = result.refund_code().ok_or_else(bad_row)?;
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
