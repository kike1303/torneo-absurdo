//! Probation (012 RF27, ADR-0008 D8 and its note of 2026-11-02): an account is in probation while it
//! is younger than 7 days **or** has fewer than 10 fights with a bet won or lost. Derived, never
//! stored; both facts only grow, so an account never goes back in. Pure.

use protocol::Probation;

/// The age that ends probation.
pub const PROBATION_MS: i64 = 7 * DAY_MS;
/// The settled fights that end probation: refunds do not count.
pub const PROBATION_FIGHTS: i64 = 10;
const DAY_MS: i64 = 86_400_000;

/// What is left of probation, or `None` outside it. `age_ms` is the age of the account and
/// `fights` its distinct fights with a bet won or lost.
#[must_use]
pub fn of(age_ms: i64, fights: i64) -> Option<Probation> {
    // A clock of the database that went back makes a new account look younger than new: it is new.
    let ms_left = PROBATION_MS.saturating_sub(age_ms.max(0)).max(0);
    let fights_left = PROBATION_FIGHTS.saturating_sub(fights).max(0);
    if ms_left == 0 && fights_left == 0 {
        return None;
    }
    // Whole days, rounded up: an account 6.5 days old has 1 day left.
    let days_left = ms_left.div_euclid(DAY_MS) + i64::from(ms_left.rem_euclid(DAY_MS) > 0);
    Some(Probation {
        days_left: u16::try_from(days_left).unwrap_or(u16::MAX),
        fights_left: u16::try_from(fights_left).unwrap_or(u16::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    /// The two axes, each on its own (RF27).
    #[test]
    fn either_axis_keeps_an_account_in_probation() {
        // New, and without fights.
        assert_eq!(
            of(0, 0),
            Some(Probation {
                days_left: 7,
                fights_left: 10
            })
        );
        // Old enough, but 9 fights: test 24.
        assert_eq!(
            of(PROBATION_MS, 9),
            Some(Probation {
                days_left: 0,
                fights_left: 1
            })
        );
        // Ten fights, but 6.5 days old.
        assert_eq!(
            of(6 * DAY_MS + 12 * HOUR, 10),
            Some(Probation {
                days_left: 1,
                fights_left: 0
            })
        );
        // Both done: out, and more of either changes nothing.
        assert_eq!(of(PROBATION_MS, 10), None);
        assert_eq!(of(100 * DAY_MS, 500), None);
    }

    #[test]
    fn odd_inputs_do_not_wrap() {
        assert_eq!(
            of(-5, -3),
            Some(Probation {
                days_left: 7,
                fights_left: 13
            })
        );
        assert_eq!(of(i64::MAX, i64::MAX), None);
        assert_eq!(of(i64::MIN, 0).map(|p| p.days_left), Some(7));
    }

    /// A clock that steps back between the account's creation and the read gives a negative age:
    /// still the 7 days of a new account, never 8.
    #[test]
    fn a_negative_age_is_a_new_account() {
        assert_eq!(of(-DAY_MS / 2, 0).map(|p| p.days_left), Some(7));
    }
}
