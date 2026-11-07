//! The split of a pool (012 RF10 to RF12, RF47) and `RankedProfit` (RF11, ADR-0008 D8). Pure: no
//! SQL, no clock, `i128` arithmetic rounded down, and no `as`. A figure that does not fit an `i64`
//! is an error, never a wrap.

/// Basis points in a whole.
const BP: i128 = 10_000;

/// A figure out of range: a stake that is not positive, a rake outside `0..=10000` bp, or a sum
/// that does not fit an `i64`. The hook turns it into `row_invalid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfRange;

/// What a winning pool pays (RF10). `payouts` follows the order of the winning stakes it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    pub rake: i64,
    pub residue: i64,
    pub payouts: Vec<i64>,
}

fn to_i64(value: i128) -> Result<i64, OutOfRange> {
    i64::try_from(value).map_err(|_| OutOfRange)
}

fn sum(stakes: &[i64]) -> Result<i128, OutOfRange> {
    stakes.iter().try_fold(0_i128, |total, stake| {
        if *stake > 0 {
            Ok(total + i128::from(*stake))
        } else {
            Err(OutOfRange)
        }
    })
}

/// `rake = min(⌊T · r / 10000⌋, L)`, the rake of a pool of `total` with a losing side of `losing`.
fn rake_of(total: i128, losing: i128, rake_bp: u16) -> Result<i128, OutOfRange> {
    let rake_bp = i128::from(rake_bp);
    if rake_bp > BP {
        return Err(OutOfRange);
    }
    Ok((total * rake_bp / BP).min(losing))
}

/// The split of RF10: `winning` are the stakes of the winning side, in any order, and `losing` is
/// `L`. The winning side must not be empty, and every stake must be positive.
///
/// # Errors
/// [`OutOfRange`] for an empty or non-positive side, a rake over 10 000 bp or a figure out of `i64`.
pub fn split(winning: &[i64], losing: i64, rake_bp: u16) -> Result<Split, OutOfRange> {
    let w = sum(winning)?;
    if w == 0 || losing <= 0 {
        return Err(OutOfRange);
    }
    let l = i128::from(losing);
    let total = w + l;
    let rake = rake_of(total, l, rake_bp)?;
    let net = total - rake;
    let mut paid = 0_i128;
    let mut payouts = Vec::with_capacity(winning.len());
    for stake in winning {
        let payout = i128::from(*stake) * net / w;
        paid += payout;
        payouts.push(to_i64(payout)?);
    }
    Ok(Split {
        rake: to_i64(rake)?,
        residue: to_i64(net - paid)?,
        payouts,
    })
}

/// The check of the hook before its first write (RF13): every coin of the pool goes somewhere, and
/// no winner collects less than its stake.
#[must_use]
pub fn check_split(split: &Split, winning: &[i64], losing: i64) -> bool {
    if split.payouts.len() != winning.len() || split.rake < 0 || split.residue < 0 {
        return false;
    }
    let (Ok(w), Ok(paid)) = (sum(winning), sum_any(&split.payouts)) else {
        return false;
    };
    let total = w + i128::from(losing);
    let winners_whole = split
        .payouts
        .iter()
        .zip(winning)
        .all(|(payout, stake)| payout >= stake);
    winners_whole
        && i128::from(split.rake) <= i128::from(losing)
        && paid + i128::from(split.rake) + i128::from(split.residue) == total
}

fn sum_any(values: &[i64]) -> Result<i128, OutOfRange> {
    Ok(values.iter().map(|v| i128::from(*v)).sum())
}

/// What 10 000 coins on `own` would collect now (RF34): `⌊10000 · N / W⌋` with this pool as if
/// `own` won. `None` while either side is empty; saturates at `u32::MAX`.
#[must_use]
pub fn payout_bp(own: i64, other: i64, rake_bp: u16) -> Option<u32> {
    if own <= 0 || other <= 0 {
        return None;
    }
    let w = i128::from(own);
    let total = w + i128::from(other);
    let rake = rake_of(total, i128::from(other), rake_bp).ok()?;
    let value = BP * (total - rake) / w;
    Some(u32::try_from(value).unwrap_or(u32::MAX))
}

/// The estimate of RF47: what `amount` alone collects if `own` wins, with the pool now plus itself.
/// Returns `(payout, one_sided)`; with the other side empty the bet comes back whole.
///
/// # Errors
/// [`OutOfRange`] for a non-positive amount or a pool out of range.
pub fn quote(amount: i64, own: i64, other: i64, rake_bp: u16) -> Result<(i64, bool), OutOfRange> {
    if amount <= 0 || own < 0 || other < 0 {
        return Err(OutOfRange);
    }
    if other == 0 {
        return Ok((amount, true));
    }
    let w = i128::from(own) + i128::from(amount);
    let l = i128::from(other);
    let total = w + l;
    let net = total - rake_of(total, l, rake_bp)?;
    Ok((to_i64(i128::from(amount) * net / w)?, false))
}

/// `RankedProfit` of one bet (RF11). `won` says whether its side won; `losing` is `L` and
/// `losing_ranked` is `L_c`, the part of `L` with `ranked_stake`.
///
/// # Errors
/// [`OutOfRange`] for a figure out of `i64`.
pub fn ranked_profit(
    won: bool,
    payout: i64,
    stake: i64,
    losing: i64,
    losing_ranked: i64,
) -> Result<i64, OutOfRange> {
    if !won {
        return to_i64(-i128::from(stake));
    }
    let d = i128::from(payout) - i128::from(stake);
    if d <= 0 || losing == 0 {
        return to_i64(d);
    }
    to_i64(d * i128::from(losing_ranked) / i128::from(losing))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn the_examples_of_rf10() {
        // 7 and 5 on A against 9 on B with 500 bp, A wins.
        let s = split(&[7, 5], 9, 500).unwrap();
        assert_eq!(
            s,
            Split {
                rake: 1,
                residue: 1,
                payouts: vec![11, 8],
            }
        );
        assert!(check_split(&s, &[7, 5], 9));
        // 500 against 10, the 500 wins: the cap leaves the winner whole.
        let s = split(&[500], 10, 500).unwrap();
        assert_eq!(
            s,
            Split {
                rake: 10,
                residue: 0,
                payouts: vec![500],
            }
        );
        // `r = 0`: no rake, the whole pool to the winners.
        let s = split(&[10, 10], 10, 0).unwrap();
        assert_eq!(
            s,
            Split {
                rake: 0,
                residue: 0,
                payouts: vec![15, 15],
            }
        );
    }

    #[test]
    fn split_refuses_what_has_no_route() {
        assert_eq!(split(&[], 10, 0), Err(OutOfRange));
        assert_eq!(split(&[10], 0, 0), Err(OutOfRange));
        assert_eq!(split(&[10, 0], 10, 0), Err(OutOfRange));
        assert_eq!(split(&[-1], 10, 0), Err(OutOfRange));
        assert_eq!(split(&[10], 10, 10_001), Err(OutOfRange));
        // Sums beyond `i64` do not wrap: the pool is computed in `i128`, the payout must fit.
        assert_eq!(split(&[i64::MAX], i64::MAX, 0), Err(OutOfRange));
    }

    #[test]
    fn check_split_catches_a_split_that_does_not_add_up() {
        let good = split(&[7, 5], 9, 500).unwrap();
        let mut more = good.clone();
        more.payouts[0] += 1;
        assert!(!check_split(&more, &[7, 5], 9));
        let mut less = good.clone();
        less.residue -= 1;
        assert!(!check_split(&less, &[7, 5], 9));
        let mut short = good.clone();
        short.payouts.pop();
        assert!(!check_split(&short, &[7, 5], 9));
        // A winner under its stake, even if the sum holds.
        let robbed = Split {
            rake: 0,
            residue: 0,
            payouts: vec![6, 15],
        };
        assert!(!check_split(&robbed, &[7, 5], 9));
        // A rake over the losing side.
        let greedy = Split {
            rake: 10,
            residue: 0,
            payouts: vec![7, 4],
        };
        assert!(!check_split(&greedy, &[7, 5], 9));
    }

    #[test]
    fn payout_bp_needs_both_sides_and_saturates() {
        assert_eq!(payout_bp(0, 10, 0), None);
        assert_eq!(payout_bp(10, 0, 0), None);
        assert_eq!(payout_bp(10, 10, 0), Some(20_000));
        // 10 against 10 with 500 bp: `N = 19`.
        assert_eq!(payout_bp(10, 10, 500), Some(19_000));
        assert_eq!(payout_bp(1, i64::MAX, 0), Some(u32::MAX));
        assert_eq!(payout_bp(10, 10, 10_001), None);
    }

    #[test]
    fn a_quote_is_the_split_of_the_pool_with_the_bet() {
        // 10 against 10 is x1.90 by `payout_bp`, but 500 more on A collects only its stake: the
        // rake hits its cap at `L = 10`.
        assert_eq!(quote(500, 10, 10, 500), Ok((500, false)));
        assert_eq!(quote(100, 0, 0, 500), Ok((100, true)));
        assert_eq!(quote(100, 50, 0, 500), Ok((100, true)));
        assert_eq!(quote(0, 10, 10, 0), Err(OutOfRange));
        assert_eq!(
            quote(i64::from(u32::MAX), 10, 10, 0).map(|(p, _)| p >= i64::from(u32::MAX)),
            Ok(true)
        );
        // The same figure the split gives to that bet.
        let (payout, _) = quote(25, 75, 40, 500).unwrap();
        assert_eq!(split(&[75, 25], 40, 500).unwrap().payouts[1], payout);
    }

    /// RF11, test 25 (b), (d), (e) and (g) on the function.
    #[test]
    fn ranked_profit_has_the_four_branches() {
        assert_eq!(ranked_profit(false, 0, 30, 100, 100), Ok(-30));
        // 25(d): `d < 0` has no route through the settlement, only here.
        assert_eq!(ranked_profit(true, 20, 30, 100, 100), Ok(-10));
        assert_eq!(ranked_profit(true, 30, 30, 100, 0), Ok(0));
        // 25(e): `L = 0`.
        assert_eq!(ranked_profit(true, 45, 30, 0, 0), Ok(15));
        // 25(b): `L_c = L / 2` with an odd `d` rounds down.
        assert_eq!(ranked_profit(true, 45, 30, 100, 50), Ok(7));
        // 25(a): a losing side only with `false` gives 0.
        assert_eq!(ranked_profit(true, 45, 30, 100, 0), Ok(0));
        assert_eq!(ranked_profit(true, 45, 30, 100, 100), Ok(15));
    }

    fn pool() -> impl Strategy<Value = (Vec<i64>, i64, u16)> {
        (
            prop::collection::vec(1_i64..=1_000_000, 1..40),
            1_i64..=10_000_000,
            0_u16..=2000,
        )
    }

    proptest! {
        /// RF10: every coin goes somewhere, the rake stays under `L`, a winner never loses and the
        /// residue is less than one coin per winner.
        #[test]
        fn the_split_conserves_and_never_takes_from_a_winner((winning, losing, rake_bp) in pool()) {
            let s = split(&winning, losing, rake_bp).unwrap();
            prop_assert!(check_split(&s, &winning, losing));
            let total: i64 = winning.iter().sum::<i64>() + losing;
            prop_assert_eq!(s.payouts.iter().sum::<i64>() + s.rake + s.residue, total);
            prop_assert!(s.rake <= losing);
            for (payout, stake) in s.payouts.iter().zip(&winning) {
                prop_assert!(payout >= stake);
            }
            prop_assert!(s.residue >= 0);
            prop_assert!(usize::try_from(s.residue).unwrap() < winning.len());
        }

        /// RF10: the order of the bets changes nothing.
        #[test]
        fn the_order_changes_nothing((winning, losing, rake_bp) in pool(), seed in any::<u64>()) {
            let s = split(&winning, losing, rake_bp).unwrap();
            let mut order: Vec<usize> = (0..winning.len()).collect();
            let len = order.len();
            for i in 0..len {
                let j = usize::try_from(seed.rotate_left(u32::try_from(i % 64).unwrap()) % u64::try_from(len).unwrap()).unwrap();
                order.swap(i, j);
            }
            let shuffled: Vec<i64> = order.iter().map(|i| winning[*i]).collect();
            let t = split(&shuffled, losing, rake_bp).unwrap();
            prop_assert_eq!((s.rake, s.residue), (t.rake, t.residue));
            for (k, i) in order.iter().enumerate() {
                prop_assert_eq!(t.payouts[k], s.payouts[*i]);
            }
        }

        /// RF12: splitting one stake in two never pays more than one bet.
        #[test]
        fn splitting_a_stake_never_pays_more(
            (winning, losing, rake_bp) in pool(),
            cut in 1_i64..1_000_000,
        ) {
            let whole = winning[0];
            prop_assume!(cut < whole);
            let one = split(&winning, losing, rake_bp).unwrap();
            let mut two_bets = vec![cut, whole - cut];
            two_bets.extend_from_slice(&winning[1..]);
            let two = split(&two_bets, losing, rake_bp).unwrap();
            prop_assert!(two.payouts[0] + two.payouts[1] <= one.payouts[0]);
        }

        /// RF47: the quote of a bet is what the split pays it with the same pool.
        #[test]
        fn a_quote_matches_the_split(
            (winning, losing, rake_bp) in pool(),
            amount in 1_i64..1_000_000,
        ) {
            let own: i64 = winning.iter().sum();
            let (payout, one_sided) = quote(amount, own, losing, rake_bp).unwrap();
            prop_assert!(!one_sided);
            let mut with_bet = winning.clone();
            with_bet.push(amount);
            let s = split(&with_bet, losing, rake_bp).unwrap();
            prop_assert_eq!(s.payouts.last().copied(), Some(payout));
        }
    }
}
