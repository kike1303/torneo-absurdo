//! What an attack gives (021 "Números"). Pure: integers only, no database and no clock.
//!
//! The coins of the attacker (TA-63) and the XP of both sides (TA-62). The rating (TA-64) is
//! computed here too, with the same inputs, and stays 0 until its phase fills [`rating_delta`]: the
//! transaction of the fight already writes what this module returns.

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
        /// The rating of the defender minus the attacker's, for the bonus of a victory.
        rating_gap: i32,
    },
    /// A character of the tournament: XP only, never rating nor coins.
    Tournament,
}

/// What [`compute`] decides for one fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rewards {
    pub factor_pct: u8,
    /// The change of the attacker; the defender moves the opposite.
    pub rating_delta: i16,
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
    Rewards {
        factor_pct: factor,
        rating_delta: rating_delta(),
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

/// TA-64 replaces this with the integer Elo of 021, scaled by the step and 0 when the fight does
/// not count. The column and the constraint (`-40..=40`) already exist.
const fn rating_delta() -> i16 {
    0
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
    fn scale_never_overflows() {
        assert_eq!(scale(u32::MAX, 100), u32::MAX);
        assert_eq!(scale(u32::MAX, 0), 0);
        assert_eq!(scale(3, 50), 1);
    }
}
