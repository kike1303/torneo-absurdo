//! The bets (feature 012): the hook of the settlement, the betting API, the daily grant, the bailout
//! and the pools.

pub mod audit;
pub mod claims;
pub mod facts;
mod http;
pub mod parimutuel;
pub mod probation;
pub mod settle;
pub mod wallet;

pub use settle::BetsHook;

use crate::ledger::Coins;

/// What the environment configures for the bets (012 RF40). `B` is `DAILY_GRANT`, `R = ⌊B / 2⌋`
/// (ADR-0008 D8). The rake is in [`crate::fights::FightsConfig`], because the announcement writes
/// it (plan-b D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BetsConfig {
    /// `B`: the daily grant.
    pub grant: Coins,
    /// `m`: the smallest bet.
    pub min_bet: Coins,
    /// `m_cap`: the cap of one fight outside probation, in grants.
    pub cap_multiple: i64,
}

impl BetsConfig {
    pub const DEFAULT_MIN_BET: i64 = 25;
    pub const DEFAULT_RAKE_BP: i16 = 0;
    pub const DEFAULT_CAP_MULTIPLE: i64 = 5;
    pub const MAX_RAKE_BP: i16 = 2000;
    pub const CAP_MULTIPLE_RANGE: std::ops::RangeInclusive<i64> = 1..=20;

    /// `R`: the bailout.
    #[must_use]
    pub const fn bailout(self) -> Coins {
        Coins(self.grant.0 / 2)
    }

    /// The most one user may stake on one fight (012 RF27): `1 B` in probation, `m_cap · B` outside.
    #[must_use]
    pub const fn fight_cap(self, in_probation: bool) -> Coins {
        if in_probation {
            self.grant
        } else {
            Coins(self.grant.0.saturating_mul(self.cap_multiple))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_derived_values_of_rf40() {
        let config = BetsConfig {
            grant: Coins(500),
            min_bet: Coins(25),
            cap_multiple: 5,
        };
        assert_eq!(config.bailout(), Coins(250));
        assert_eq!(config.fight_cap(true), Coins(500));
        assert_eq!(config.fight_cap(false), Coins(2500));
        let odd = BetsConfig {
            grant: Coins(51),
            ..config
        };
        assert_eq!(odd.bailout(), Coins(25));
    }
}
