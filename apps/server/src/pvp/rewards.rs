//! What an attack gives (021 "Números"). Pure: integers only, no database and no clock.
//!
//! The coins of the attacker (TA-63), the XP of both sides (TA-62) and the integer Elo (TA-64): the
//! transaction of the fight writes what this module returns.

use protocol::Side;

/// Attacks of a UTC day at full pay.
pub const FULL_ATTACKS: u16 = 10;

/// Attacks of a UTC day at all (owner, 2026-11-11): each one stores its log for good, so the volume
/// of one account has a ceiling. Above the last paid step (20), so the steps do not change.
pub const DAILY_ATTACKS_MAX: u16 = 50;

/// Fights of one pair in one UTC day that count: from the fourth on, no rating, no coins and no XP
/// of defense (021 "Emparejamiento").
pub const PAIR_FIGHTS_PER_DAY: i64 = 3;

/// Attacks as attacker, and days of age, that a user needs to leave the PvP probation (021
/// "Carencia PvP").
pub const PROBATION_ATTACKS: i64 = 10;
pub const PROBATION_DAYS: i32 = 7;

/// The step of the day of the `daily_index`-th attack (1-based): 100 % up to 10, then 75, 50 and
/// 25 %, and 0 from the 21st on.
#[must_use]
pub const fn factor_pct(daily_index: u16) -> u8 {
    match daily_index {
        0..=FULL_ATTACKS => 100,
        11..=13 => 75,
        14..=16 => 50,
        17..=20 => 25,
        _ => 0,
    }
}

/// XP of an attack against a user, by result (021 "XP por pelea"): victory, and defeat or draw.
pub const XP_WIN: u32 = 100;
pub const XP_LOSS: u32 = 30;

/// XP of a defense, by result: half of the attack's.
pub const XP_DEFENSE_WIN: u32 = 50;
pub const XP_DEFENSE_LOSS: u32 = 15;

/// XP of an attack against a character of the tournament, by result.
pub const XP_TOURNAMENT_WIN: u32 = 70;
pub const XP_TOURNAMENT_LOSS: u32 = 20;

/// The rating gap the bonus of a victory reads, both ways, and its divisor: from −50 to +50 XP.
pub const XP_BONUS_GAP: i32 = 200;
pub const XP_BONUS_DIVISOR: i32 = 4;

/// The bounds of a rating, as `botargas_rating_ck`, and where every botarga starts.
pub const RATING_MIN: i32 = 0;
pub const RATING_MAX: i32 = 4000;
pub const RATING_START: i32 = 1000;

/// The K of the Elo of each side: 40 while it has fewer than [`K_NEW_FIGHTS`] rated fights, then 20
/// (owner, 2026-11-14: each side its own K, so a new alt never moves a settled account by 40).
pub const K_NEW: i32 = 40;
pub const K_SETTLED: i32 = 20;
pub const K_NEW_FIGHTS: u32 = 30;

/// The points of rating of one step of [`EXPECTED_MILLI`].
const EXPECTED_STEP: u32 = 25;

/// The expected score, in thousandths, of a side whose rival has `25 · i` more points (021 "Elo en
/// enteros"); the last entry holds for any gap of 800 or more. A table and not `10^(d/400)`, so
/// nothing the server persists comes out of `f64`.
const EXPECTED_MILLI: [i32; 33] = [
    500, 464, 429, 394, 360, 327, 297, 268, 240, 215, 192, 170, 151, 133, 118, 104, 91, 80, 70, 61,
    53, 46, 40, 35, 31, 27, 23, 20, 17, 15, 13, 11, 10,
];

/// Who the attacker fought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rival {
    /// The botarga of another user.
    Player {
        /// Neither side is in probation and the pair has not used its fights of the day: the
        /// fight moves rating and pays coins.
        counts: bool,
        /// The pair has not used its fights of the day: the defender earns XP.
        pair_open: bool,
        /// The rating of the defender minus the attacker's: the bonus of a victory and the Elo.
        rating_gap: i32,
        /// The rating of the attacker, for the bounds of its change.
        attacker_rating: i32,
        /// The rated fights of each side, which choose its K.
        attacker_rated_fights: u32,
        defender_rated_fights: u32,
    },
    /// A character of the tournament: XP only, never rating nor coins.
    Tournament,
}

/// What [`compute`] decides for one fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rewards {
    pub factor_pct: u8,
    /// The fight moves the rating: it counts and its step is not 0. A rated draw of two equal
    /// ratings still counts as rated with a change of 0.
    pub rated: bool,
    /// The change of the attacker's rating, as applied: within the bounds of 021.
    pub rating_delta: i16,
    /// The change of the defender's rating, with its own K and as applied. Opposite in sign to
    /// the attacker's (or 0); the same size when both K are equal.
    pub defender_rating_delta: i16,
    pub xp_attacker: u32,
    pub xp_defender: u32,
    pub coins_attacker: u32,
}

/// The rewards of an attack: `win_coins` (`PVP_WIN_COINS`) scaled by the step of the day, only for
/// a victory of the attacker against a user, and only when the fight counts.
#[must_use]
pub fn compute(daily_index: u16, winner: Option<Side>, rival: Rival, win_coins: u32) -> Rewards {
    let factor = factor_pct(daily_index);
    let won = winner == Some(Side::A);
    let coins_attacker = match rival {
        Rival::Player { counts: true, .. } if won => scale(win_coins, factor),
        Rival::Player { .. } | Rival::Tournament => 0,
    };
    let (xp_attacker, xp_defender) = xp(winner, rival);
    let (rated, rating_delta, defender_rating_delta) = match rival {
        Rival::Player {
            counts: true,
            rating_gap,
            attacker_rating,
            attacker_rated_fights,
            defender_rated_fights,
            ..
        } if factor > 0 => {
            let flipped = winner.map(|side| match side {
                Side::A => Side::B,
                Side::B => Side::A,
            });
            let a = elo_delta(rating_gap, k_of(attacker_rated_fights), winner, factor);
            let d = elo_delta(-rating_gap, k_of(defender_rated_fights), flipped, factor);
            let defender_rating = attacker_rating.saturating_add(rating_gap);
            (
                true,
                applied(attacker_rating, a),
                applied(defender_rating, d),
            )
        }
        Rival::Player { .. } | Rival::Tournament => (false, 0, 0),
    };
    Rewards {
        factor_pct: factor,
        rated,
        rating_delta,
        defender_rating_delta,
        xp_attacker: scale(xp_attacker, factor),
        xp_defender: scale(xp_defender, factor),
        coins_attacker,
    }
}

/// The XP of the attacker and of the defender before the step of the day (021 "XP por pelea"). A
/// draw pays as a defeat to both. The probation stops the rating and the coins, never the XP (owner,
/// 2026-11-09 and 2026-11-13), so the bonus of a victory reads the gap in probation too; over the
/// limit of the pair it does not, so an alt cannot feed it.
fn xp(winner: Option<Side>, rival: Rival) -> (u32, u32) {
    match rival {
        Rival::Player {
            pair_open,
            rating_gap,
            ..
        } => {
            let attacker = if winner == Some(Side::A) {
                let bonus = if pair_open {
                    rating_gap.clamp(-XP_BONUS_GAP, XP_BONUS_GAP) / XP_BONUS_DIVISOR
                } else {
                    0
                };
                XP_WIN.saturating_add_signed(bonus)
            } else {
                XP_LOSS
            };
            let defender = match (pair_open, winner) {
                (false, _) => 0,
                (true, Some(Side::B)) => XP_DEFENSE_WIN,
                (true, _) => XP_DEFENSE_LOSS,
            };
            (attacker, defender)
        }
        Rival::Tournament if winner == Some(Side::A) => (XP_TOURNAMENT_WIN, 0),
        Rival::Tournament => (XP_TOURNAMENT_LOSS, 0),
    }
}

/// `value · factor / 100`, rounded down.
#[must_use]
pub fn scale(value: u32, factor_pct: u8) -> u32 {
    let scaled = u64::from(value) * u64::from(factor_pct) / 100;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

/// What `delta` really moves a `rating`, within [`RATING_MIN`] and [`RATING_MAX`]: what the fight
/// stores, so a close reads back exactly what was applied.
fn applied(rating: i32, delta: i16) -> i16 {
    let moved = bounded(rating, i32::from(delta)) - rating.clamp(RATING_MIN, RATING_MAX);
    i16::try_from(moved).unwrap_or(0)
}

/// The K of a side with `rated_fights` rated fights.
#[must_use]
pub const fn k_of(rated_fights: u32) -> i32 {
    if rated_fights < K_NEW_FIGHTS {
        K_NEW
    } else {
        K_SETTLED
    }
}

/// The expected score of a side, in thousandths, against a rival with `gap` more points (negative
/// when the rival has fewer). Symmetric: `expected(d) + expected(-d) = 1000`.
#[must_use]
pub fn expected_milli(gap: i32) -> i32 {
    let step = usize::try_from(gap.unsigned_abs() / EXPECTED_STEP).unwrap_or(usize::MAX);
    let e = EXPECTED_MILLI[step.min(EXPECTED_MILLI.len() - 1)];
    if gap >= 0 { e } else { 1000 - e }
}

/// `K · (score − E) / 1000`, rounded half away from zero, with the score in thousandths.
const fn elo_raw(k: i32, score: i32, expected: i32) -> i32 {
    let diff = score - expected;
    if diff >= 0 {
        (k * diff + 500) / 1000
    } else {
        -((k * -diff + 500) / 1000)
    }
}

/// The change of the attacker's rating (021 "Elo en enteros"): a victory scores 1000, a defeat 0
/// and a draw 500; then the step of the day, truncating toward zero. A victory or a defeat that is
/// rated moves at least 1; a draw between equals moves 0. Within `±K`.
#[must_use]
pub fn elo_delta(rating_gap: i32, k: i32, winner: Option<Side>, factor_pct: u8) -> i16 {
    let expected = expected_milli(rating_gap);
    let score = match winner {
        Some(Side::A) => 1000,
        Some(Side::B) => 0,
        None => 500,
    };
    let scaled = elo_raw(k, score, expected) * i32::from(factor_pct) / 100;
    let delta = match winner {
        Some(Side::A) if factor_pct > 0 => scaled.max(1),
        Some(Side::B) if factor_pct > 0 => scaled.min(-1),
        _ => scaled,
    };
    i16::try_from(delta.clamp(-k, k)).unwrap_or(0)
}

/// `rating + delta`, within [`RATING_MIN`] and [`RATING_MAX`].
#[must_use]
pub const fn bounded(rating: i32, delta: i32) -> i32 {
    let moved = rating.saturating_add(delta);
    if moved < RATING_MIN {
        RATING_MIN
    } else if moved > RATING_MAX {
        RATING_MAX
    } else {
        moved
    }
}

/// The soft reset of the close of a season (021 "Reinicio suave"): half the distance to 1000, with
/// the division truncating toward zero.
#[must_use]
pub const fn soft_reset(rating: i32) -> i32 {
    RATING_START + (rating - RATING_START) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_of_the_day_follow_the_table() {
        let expected = [
            (1, 100),
            (10, 100),
            (11, 75),
            (13, 75),
            (14, 50),
            (16, 50),
            (17, 25),
            (20, 25),
            (21, 0),
            (500, 0),
            (u16::MAX, 0),
        ];
        for (index, pct) in expected {
            assert_eq!(factor_pct(index), pct, "{index}");
        }
    }

    #[test]
    fn only_a_victory_against_a_user_that_counts_pays() {
        let human = Rival::Player {
            counts: true,
            pair_open: true,
            rating_gap: 0,
            attacker_rating: 1000,
            attacker_rated_fights: 0,
            defender_rated_fights: 0,
        };
        assert_eq!(compute(1, Some(Side::A), human, 15).coins_attacker, 15);
        assert_eq!(compute(11, Some(Side::A), human, 15).coins_attacker, 11);
        assert_eq!(compute(14, Some(Side::A), human, 15).coins_attacker, 7);
        assert_eq!(compute(17, Some(Side::A), human, 15).coins_attacker, 3);
        assert_eq!(compute(21, Some(Side::A), human, 15).coins_attacker, 0);
        // A defeat, a draw, a fight that does not count and the tournament pay nothing.
        assert_eq!(compute(1, Some(Side::B), human, 15).coins_attacker, 0);
        assert_eq!(compute(1, None, human, 15).coins_attacker, 0);
        let capped = Rival::Player {
            counts: false,
            pair_open: false,
            rating_gap: 0,
            attacker_rating: 1000,
            attacker_rated_fights: 0,
            defender_rated_fights: 0,
        };
        assert_eq!(compute(1, Some(Side::A), capped, 15).coins_attacker, 0);
        let tournament = compute(1, Some(Side::A), Rival::Tournament, 15);
        assert_eq!(tournament.coins_attacker, 0);
        assert_eq!(tournament.rating_delta, 0);
        assert_eq!(tournament.xp_defender, 0);
        // `PVP_WIN_COINS = 0` is the valve of 021.
        assert_eq!(compute(1, Some(Side::A), human, 0).coins_attacker, 0);
        assert_eq!(compute(11, Some(Side::A), human, 15).factor_pct, 75);
    }

    fn player(counts: bool, pair_open: bool, rating_gap: i32) -> Rival {
        Rival::Player {
            counts,
            pair_open,
            rating_gap,
            attacker_rating: 1000,
            attacker_rated_fights: 0,
            defender_rated_fights: 0,
        }
    }

    #[test]
    fn the_xp_follows_the_table_and_the_step() {
        let even = player(true, true, 0);
        let won = compute(1, Some(Side::A), even, 15);
        assert_eq!((won.xp_attacker, won.xp_defender), (100, 15));
        let lost = compute(1, Some(Side::B), even, 15);
        assert_eq!((lost.xp_attacker, lost.xp_defender), (30, 50));
        let drawn = compute(1, None, even, 15);
        assert_eq!((drawn.xp_attacker, drawn.xp_defender), (30, 15));
        // The steps scale both sides: 75 % of 100 and of 15 (11, rounded down).
        let eleventh = compute(11, Some(Side::A), even, 15);
        assert_eq!((eleventh.xp_attacker, eleventh.xp_defender), (75, 11));
        let late = compute(21, Some(Side::A), even, 15);
        assert_eq!((late.xp_attacker, late.xp_defender), (0, 0));
        // The tournament: 70 / 20, and no defender.
        let t = compute(1, Some(Side::A), Rival::Tournament, 15);
        assert_eq!((t.xp_attacker, t.xp_defender), (70, 0));
        let t = compute(17, Some(Side::B), Rival::Tournament, 15);
        assert_eq!((t.xp_attacker, t.xp_defender), (5, 0));
    }

    #[test]
    fn the_rating_bonus_pays_victories_under_the_pair_limit() {
        let up = |gap| compute(1, Some(Side::A), player(true, true, gap), 15).xp_attacker;
        assert_eq!(up(300), 150);
        assert_eq!(up(200), 150);
        assert_eq!(up(103), 125);
        assert_eq!(up(-103), 75);
        assert_eq!(up(-1000), 50);
        // A defeat ignores the gap, and so does a fight over the limit of the pair.
        let lost = compute(1, Some(Side::B), player(true, true, 400), 15);
        assert_eq!(lost.xp_attacker, 30);
        let over = compute(1, Some(Side::A), player(false, false, 400), 15);
        assert_eq!(over.xp_attacker, 100);
        // Probation keeps the bonus: it stops the rating and the coins, not the XP.
        let probation = compute(1, Some(Side::A), player(false, true, 400), 15);
        assert_eq!(probation.xp_attacker, 150);
        // Probation leaves the XP of the defender; the pair limit takes it.
        assert_eq!(probation.xp_defender, 15);
        let capped = compute(1, Some(Side::B), player(false, false, 0), 15);
        assert_eq!((capped.xp_attacker, capped.xp_defender), (30, 0));
    }

    #[test]
    fn the_table_of_expectations_is_symmetric() {
        assert_eq!(expected_milli(0), 500);
        for gap in -1000..=1000 {
            assert_eq!(expected_milli(gap) + expected_milli(-gap), 1000, "{gap}");
            assert!(expected_milli(gap) >= expected_milli(gap + 1), "{gap}");
        }
        assert_eq!(expected_milli(24), 500);
        assert_eq!(expected_milli(25), 464);
        assert_eq!(expected_milli(800), 10);
        assert_eq!(expected_milli(5000), 10);
        assert_eq!(expected_milli(i32::MAX), 10);
        assert_eq!(expected_milli(i32::MIN), 990);
    }

    #[test]
    fn the_example_of_021_and_the_k() {
        // 1000 against 1300, K = 40: +34 and −6.
        assert_eq!(elo_delta(300, K_NEW, Some(Side::A), 100), 34);
        assert_eq!(elo_delta(300, K_NEW, Some(Side::B), 100), -6);
        // Equal ratings: ±20 with K = 40, ±10 with K = 20; a draw between equals moves nothing.
        assert_eq!(elo_delta(0, K_NEW, Some(Side::A), 100), 20);
        assert_eq!(elo_delta(0, K_SETTLED, Some(Side::B), 100), -10);
        assert_eq!(elo_delta(0, K_NEW, None, 100), 0);
        assert_eq!(elo_delta(300, K_NEW, None, 100), 14);
        assert_eq!(elo_delta(-300, K_NEW, None, 100), -14);
        assert_eq!(k_of(0), 40);
        assert_eq!(k_of(29), 40);
        assert_eq!(k_of(30), 20);
        assert_eq!(k_of(u32::MAX), 20);
    }

    #[test]
    fn a_rated_fight_never_moves_zero_and_never_past_k() {
        for gap in (-2000..=2000).step_by(7) {
            for k in [K_NEW, K_SETTLED] {
                for factor in [100, 75, 50, 25] {
                    let won = elo_delta(gap, k, Some(Side::A), factor);
                    let lost = elo_delta(gap, k, Some(Side::B), factor);
                    assert!((1..=k).contains(&i32::from(won)), "{gap} {k} {factor}");
                    assert!((-k..=-1).contains(&i32::from(lost)), "{gap} {k} {factor}");
                }
            }
        }
        // The steps truncate toward zero: 34 → 25, 17, 8; −6 → −4, −3, −1.
        assert_eq!(elo_delta(300, K_NEW, Some(Side::A), 75), 25);
        assert_eq!(elo_delta(300, K_NEW, Some(Side::A), 50), 17);
        assert_eq!(elo_delta(300, K_NEW, Some(Side::A), 25), 8);
        assert_eq!(elo_delta(300, K_NEW, Some(Side::B), 75), -4);
        assert_eq!(elo_delta(300, K_NEW, Some(Side::B), 50), -3);
        assert_eq!(elo_delta(300, K_NEW, Some(Side::B), 25), -1);
        // A very strong attacker that wins still moves 1.
        assert_eq!(elo_delta(-2000, K_SETTLED, Some(Side::A), 25), 1);
    }

    #[test]
    fn only_a_fight_that_counts_on_a_paid_step_is_rated() {
        let even = player(true, true, 0);
        let won = compute(1, Some(Side::A), even, 15);
        assert!(won.rated);
        assert_eq!(won.rating_delta, 20);
        assert_eq!(compute(11, Some(Side::A), even, 15).rating_delta, 15);
        let lost = compute(17, Some(Side::B), even, 15);
        assert!(lost.rated);
        assert_eq!(lost.rating_delta, -5);
        let drawn = compute(1, None, even, 15);
        assert!(drawn.rated);
        assert_eq!(drawn.rating_delta, 0);
        // The 21st, a pair over its limit, probation and the tournament: no rating.
        for (index, rival) in [
            (21, even),
            (1, player(false, false, 0)),
            (1, player(false, true, 0)),
            (1, Rival::Tournament),
        ] {
            let r = compute(index, Some(Side::A), rival, 15);
            assert!(!r.rated, "{index} {rival:?}");
            assert_eq!(r.rating_delta, 0, "{index} {rival:?}");
        }
        assert_eq!(won.defender_rating_delta, -20);
        assert_eq!(drawn.defender_rating_delta, 0);
        // Each side its own K: a settled attacker against a new defender, and the other way.
        let with = |attacker_rated_fights, defender_rated_fights| Rival::Player {
            counts: true,
            pair_open: true,
            rating_gap: 0,
            attacker_rating: 1000,
            attacker_rated_fights,
            defender_rated_fights,
        };
        let r = compute(1, Some(Side::A), with(30, 0), 15);
        assert_eq!((r.rating_delta, r.defender_rating_delta), (10, -20));
        let r = compute(1, Some(Side::B), with(0, 30), 15);
        assert_eq!((r.rating_delta, r.defender_rating_delta), (-20, 10));
        // The example of 021 from both sides: 1000 against 1300, both new.
        let gap = Rival::Player {
            counts: true,
            pair_open: true,
            rating_gap: 300,
            attacker_rating: 1000,
            attacker_rated_fights: 0,
            defender_rated_fights: 0,
        };
        let r = compute(1, Some(Side::A), gap, 15);
        assert_eq!((r.rating_delta, r.defender_rating_delta), (34, -34));
        let r = compute(1, Some(Side::B), gap, 15);
        assert_eq!((r.rating_delta, r.defender_rating_delta), (-6, 6));
        // At the bounds, the change is what was applied: a defender at 0 loses nothing more.
        let floor = Rival::Player {
            counts: true,
            pair_open: true,
            rating_gap: -995,
            attacker_rating: 995,
            attacker_rated_fights: 0,
            defender_rated_fights: 0,
        };
        let r = compute(1, Some(Side::A), floor, 15);
        assert!(r.rated);
        assert_eq!((r.rating_delta, r.defender_rating_delta), (1, 0));
        let ceiling = Rival::Player {
            counts: true,
            pair_open: true,
            rating_gap: 0,
            attacker_rating: 3990,
            attacker_rated_fights: 0,
            defender_rated_fights: 0,
        };
        let r = compute(1, Some(Side::A), ceiling, 15);
        assert_eq!((r.rating_delta, r.defender_rating_delta), (10, -20));
    }

    #[test]
    fn the_bounds_and_the_soft_reset() {
        assert_eq!(bounded(3990, 40), 4000);
        assert_eq!(bounded(5, -40), 0);
        assert_eq!(bounded(1000, 20), 1020);
        assert_eq!(bounded(i32::MAX, 1), 4000);
        assert_eq!(soft_reset(1600), 1300);
        assert_eq!(soft_reset(1000), 1000);
        assert_eq!(soft_reset(1001), 1000);
        assert_eq!(soft_reset(999), 1000);
        assert_eq!(soft_reset(997), 999);
        assert_eq!(soft_reset(0), 500);
        assert_eq!(soft_reset(4000), 2500);
    }

    #[test]
    fn scale_never_overflows() {
        assert_eq!(scale(u32::MAX, 100), u32::MAX);
        assert_eq!(scale(u32::MAX, 0), 0);
        assert_eq!(scale(3, 50), 1);
    }
}
