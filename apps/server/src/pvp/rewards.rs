//! What an attack gives (021 "Números"). Pure: integers only, no database and no clock.
//!
//! This phase (TA-63) pays the coins of the attacker. The XP (TA-62) and the rating (TA-64) are
//! computed here too, with the same inputs, and stay 0 until their phases fill [`xp`] and
//! [`rating_delta`]: the transaction of the fight already writes what this module returns.

use protocol::Side;

/// Attacks of a UTC day at full pay.
pub const FULL_ATTACKS: u16 = 10;

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

/// Who the attacker fought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rival {
    /// The botarga of another user. `counts`: neither side is in probation and the pair has not
    /// used its fights of the day.
    Player { counts: bool },
    /// A character of the tournament: XP only (TA-62), never rating nor coins.
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
        Rival::Player { counts: true } if won => scale(win_coins, factor),
        Rival::Player { .. } | Rival::Tournament => 0,
    };
    Rewards {
        factor_pct: factor,
        rating_delta: rating_delta(),
        xp_attacker: xp(),
        xp_defender: xp(),
        coins_attacker,
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

/// TA-62 replaces this with the XP table of 021 (and the level up in the same transaction).
const fn xp() -> u32 {
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
        let human = Rival::Player { counts: true };
        assert_eq!(compute(1, Some(Side::A), human, 15).coins_attacker, 15);
        assert_eq!(compute(11, Some(Side::A), human, 15).coins_attacker, 11);
        assert_eq!(compute(14, Some(Side::A), human, 15).coins_attacker, 7);
        assert_eq!(compute(17, Some(Side::A), human, 15).coins_attacker, 3);
        assert_eq!(compute(21, Some(Side::A), human, 15).coins_attacker, 0);
        // A defeat, a draw, a fight that does not count and the tournament pay nothing.
        assert_eq!(compute(1, Some(Side::B), human, 15).coins_attacker, 0);
        assert_eq!(compute(1, None, human, 15).coins_attacker, 0);
        let capped = Rival::Player { counts: false };
        assert_eq!(compute(1, Some(Side::A), capped, 15).coins_attacker, 0);
        let tournament = compute(1, Some(Side::A), Rival::Tournament, 15);
        assert_eq!(tournament.coins_attacker, 0);
        assert_eq!(tournament.rating_delta, 0);
        assert_eq!(tournament.xp_defender, 0);
        // `PVP_WIN_COINS = 0` is the valve of 021.
        assert_eq!(compute(1, Some(Side::A), human, 0).coins_attacker, 0);
        assert_eq!(compute(11, Some(Side::A), human, 15).factor_pct, 75);
    }

    #[test]
    fn scale_never_overflows() {
        assert_eq!(scale(u32::MAX, 100), u32::MAX);
        assert_eq!(scale(u32::MAX, 0), 0);
        assert_eq!(scale(3, 50), 1);
    }
}
