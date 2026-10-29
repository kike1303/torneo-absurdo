//! Every game constant of the engine, by version (003 RF4, RF10, "Modelo de simulación").
//!
//! Values are fixed per `ruleset_version`. Before the first production fight, version 1 may
//! change if the goldens are regenerated in the same PR; afterwards every change is a new
//! version and the old one stays here so old fights can be re-simulated (ADR-0001).

macro_rules! ruleset {
    ($( $(#[$doc:meta])* $field:ident: $ty:ty, )*) => {
        /// The rules of a version. Read-only: built only by [`Ruleset::for_version`].
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Ruleset {
            $( $field: $ty, )*
        }

        impl Ruleset {
            $(
                $(#[$doc])*
                #[must_use]
                pub const fn $field(&self) -> $ty {
                    self.$field
                }
            )*
        }
    };
}

ruleset! {
    /// `ruleset_version` written in every log.
    version: u16,

    // Character validation (decision 8 of plan-a).
    /// Minimum of each stat.
    stat_min: u8,
    /// Maximum of each stat.
    stat_max: u8,
    /// `hp + attack + defense + speed + special <= stat_budget`.
    stat_budget: u16,
    /// Maximum of each personality trait (the minimum is 0).
    personality_max: u8,
    abilities_min: u8,
    abilities_max: u8,
    weaknesses_max: u8,
    heavy_power_min: u8,
    heavy_power_max: u8,
    heavy_cooldown_min: u8,
    heavy_cooldown_max: u8,
    special_power_min: u8,
    special_power_max: u8,
    special_cooldown_min: u8,
    special_cooldown_max: u8,

    // Simulation.
    /// `max_hp = hp_base + hp_per_point * hp`.
    hp_base: u32,
    hp_per_point: u32,
    /// `interval = interval_base_ms - interval_per_speed_ms * speed`.
    interval_base_ms: u32,
    interval_per_speed_ms: u32,
    /// Jitter added to every next turn: `roll(jitter_ms + 1)`.
    jitter_ms: u32,
    /// First turn: `next_at = intro_ms + interval / 2`.
    intro_ms: u32,
    windup_strike_ms: u32,
    windup_heavy_ms: u32,
    windup_special_ms: u32,
    /// After any resolution: `next_at_def = max(next_at_def, t_res + recovery_ms)`.
    recovery_ms: u32,
    /// A critical hit that leaves the target alive and applies
    /// `damage * 1000 >= knockdown_min_permille * max_hp` knocks it down for `knockdown_ms`.
    knockdown_min_permille: u32,
    knockdown_ms: u16,
    /// Breather (006 RF1, RF2): resolutions without KO nor knockdown, since the start or the
    /// last breather or knockdown, before a breather can be rolled.
    breather_after_resolutions: u32,
    /// A breather starts when `roll(1000) < breather_chance_permille` (step 6 of RF7).
    breather_chance_permille: u32,
    /// Both clocks move by `breather_min_ms + roll(breather_span_ms + 1)` (step 7 of RF7).
    breather_min_ms: u32,
    breather_span_ms: u32,
    /// Action weights: strike `w_strike_base + aggression`, each heavy
    /// `w_heavy_base + aggression`, each special `w_special_base + w_special_per_show *
    /// showmanship`, times `w_special_desperation_mul` when desperate.
    w_strike_base: u32,
    w_heavy_base: u32,
    w_special_base: u32,
    w_special_per_show: u32,
    w_special_desperation_mul: u32,
    /// `dodge = dodge_base + dodge_per_speed * speed_def` (‰).
    dodge_base: u32,
    dodge_per_speed: u32,
    /// `block = block_base + defense_def / block_defense_div + caution_def` (‰).
    block_base: u32,
    block_defense_div: u32,
    /// Power of the basic strike.
    strike_power: u32,
    /// `base = power * (damage_offset + off) / (damage_offset + def)`.
    damage_offset: u32,
    /// Variance: `d = base * (variance_min + roll(variance_span)) / 100`.
    variance_min: u32,
    variance_span: u32,
    /// Desperate when `hp * 1000 / max_hp <= desperation_permille`.
    desperation_permille: u32,
    desperation_crit_bonus: u32,
    desperation_dmg_num: u32,
    desperation_dmg_den: u32,
    /// Critical when `roll(1000) < crit_base + crit_per_special * special_atk (+ bonus)`.
    crit_base: u32,
    crit_per_special: u32,
    crit_num: u32,
    crit_den: u32,
    weak_num: u32,
    weak_den: u32,
    block_chip_num: u32,
    block_chip_den: u32,
    /// No attack starts if `next_at + windup_special_ms > time_limit_ms`.
    time_limit_ms: u32,
    /// From `ko` or `time_up` to `end`.
    end_ms: u32,
    low_hp_permille: u32,
    comeback_deficit_permille: u32,
    clutch_permille: u32,
    /// Guard: `simulate` returns `SpecError::EventLimit` instead of exceeding it.
    max_events: u16,
}

impl Ruleset {
    /// The ruleset of a version, if it exists (v0 of the engine: only `1`).
    #[must_use]
    pub const fn for_version(version: u16) -> Option<Self> {
        match version {
            1 => Some(Self::v1()),
            _ => None,
        }
    }

    pub(crate) const fn v1() -> Self {
        Self {
            version: 1,
            stat_min: 1,
            stat_max: 100,
            stat_budget: 300,
            personality_max: 100,
            abilities_min: 1,
            abilities_max: 4,
            weaknesses_max: 2,
            heavy_power_min: 20,
            heavy_power_max: 60,
            heavy_cooldown_min: 0,
            heavy_cooldown_max: 5,
            special_power_min: 40,
            special_power_max: 100,
            special_cooldown_min: 2,
            special_cooldown_max: 8,
            hp_base: 500,
            hp_per_point: 4,
            interval_base_ms: 1800,
            interval_per_speed_ms: 10,
            jitter_ms: 200,
            intro_ms: 3000,
            windup_strike_ms: 400,
            windup_heavy_ms: 700,
            windup_special_ms: 1100,
            recovery_ms: 300,
            knockdown_min_permille: 100,
            knockdown_ms: 1500,
            breather_after_resolutions: 9,
            breather_chance_permille: 200,
            breather_min_ms: 1800,
            breather_span_ms: 800,
            w_strike_base: 100,
            w_heavy_base: 50,
            w_special_base: 30,
            w_special_per_show: 2,
            w_special_desperation_mul: 2,
            dodge_base: 40,
            dodge_per_speed: 2,
            block_base: 60,
            block_defense_div: 2,
            strike_power: 20,
            damage_offset: 100,
            variance_min: 85,
            variance_span: 31,
            desperation_permille: 250,
            desperation_crit_bonus: 150,
            desperation_dmg_num: 5,
            desperation_dmg_den: 4,
            crit_base: 40,
            crit_per_special: 1,
            crit_num: 3,
            crit_den: 2,
            weak_num: 5,
            weak_den: 4,
            block_chip_num: 3,
            block_chip_den: 10,
            time_limit_ms: 86_000,
            end_ms: 4000,
            low_hp_permille: 250,
            comeback_deficit_permille: 250,
            clutch_permille: 60,
            max_events: 512,
        }
    }

    /// Only for the `EventLimit` guard tests.
    #[cfg(test)]
    pub(crate) const fn with_max_events(mut self, max_events: u16) -> Self {
        self.max_events = max_events;
        self
    }

    /// Shortest interval between two actions of the same fighter (speed at `stat_max`, no jitter).
    #[must_use]
    pub const fn interval_min(&self) -> u32 {
        self.interval_base_ms - self.interval_per_speed_ms * self.stat_max as u32
    }

    /// Shortest structural separation between two consecutive resolutions (006 RF1): the
    /// defender acts after at least `recovery_ms`, the attacker repeats after at least
    /// `interval_min`, and either one resolves a strike windup later.
    #[must_use]
    pub const fn strike_gap_min(&self) -> u32 {
        let wait = if self.recovery_ms < self.interval_min() {
            self.recovery_ms
        } else {
            self.interval_min()
        };
        wait + self.windup_strike_ms
    }

    /// Shortest separation between the resolution that opens a breather and the next one
    /// (006 RF1, RF4 b).
    #[must_use]
    pub const fn breather_gap_min(&self) -> u32 {
        self.breather_min_ms + self.strike_gap_min()
    }

    /// Upper bound of the events of one fight (decision 2 of plan-a): every side makes at most
    /// `A` actions, each with at most 3 events (attack, resolution, knockdown), plus the unique
    /// ones (intro, 2 low_hp, 2 comeback, decisive_crit, clutch_ko, ko, time_up, end).
    #[must_use]
    pub const fn max_events_bound(&self) -> u32 {
        let first = self.intro_ms + self.interval_min() / 2;
        let last = self.time_limit_ms - self.windup_special_ms;
        let actions = (last - first) / (self.interval_min() + self.windup_strike_ms) + 1;
        1 + 2 * actions * EVENTS_PER_ACTION_MAX + EVENTS_TAIL_MAX
    }
}

/// `attack`, the resolution and `knockdown`.
pub(crate) const EVENTS_PER_ACTION_MAX: u32 = 3;
/// 2 `low_hp`, 2 `comeback`, `decisive_crit`, `clutch_ko`, `ko`, `time_up` and `end`.
pub(crate) const EVENTS_TAIL_MAX: u32 = 9;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TIMELINE_CONTRACT;

    #[test]
    fn only_version_one_exists() {
        assert_eq!(Ruleset::for_version(1), Some(Ruleset::v1()));
        assert_eq!(Ruleset::v1().version(), 1);
        assert_eq!(Ruleset::for_version(0), None);
        assert_eq!(Ruleset::for_version(2), None);
    }

    /// The pacing bounds of RF2 (data from 002) and the totality bounds of RF21.
    #[test]
    fn v1_respects_the_bounds() {
        let r = Ruleset::v1();
        // Pacing: the `Timeline` contract (spec 003 "Modelo de simulación", 006 RF6).
        let c = TIMELINE_CONTRACT;
        assert!(r.end_ms() >= c.win_gap_ms + c.win_pose_ms);
        assert!(r.intro_ms() + r.windup_strike_ms() >= c.intro_gap_ms);
        assert!((c.knockdown_min_ms..=c.knockdown_max_ms).contains(&u32::from(r.knockdown_ms())));
        assert!(r.strike_gap_min() >= c.strike_gap_ms);

        // Breather (006 RF1, RF6).
        assert!(r.breather_after_resolutions() >= 1);
        assert!(r.breather_chance_permille() <= 1000);
        assert!(r.breather_span_ms().checked_add(1).is_some());
        assert_eq!(r.strike_gap_min(), 700);
        assert_eq!(r.breather_gap_min(), 2500);

        // Events (decision 2): A = 68 actions per side, 418 events, below max_events.
        assert!(r.interval_base_ms() > r.interval_per_speed_ms() * u32::from(r.stat_max()));
        assert!(r.time_limit_ms() > r.windup_special_ms());
        assert!(r.time_limit_ms() - r.windup_special_ms() >= r.intro_ms() + r.interval_min() / 2);
        assert_eq!(r.max_events_bound(), 418);
        assert!(r.max_events_bound() < u32::from(r.max_events()));

        // Every action moves the attacker's clock forward, so the loop ends.
        assert!(r.interval_min() + r.windup_strike_ms() > 0);

        // Divisors and roll bounds are nonzero.
        for divisor in [
            r.block_defense_div(),
            r.desperation_dmg_den(),
            r.crit_den(),
            r.weak_den(),
            r.block_chip_den(),
            r.damage_offset(),
            r.variance_span(),
            r.w_strike_base(),
        ] {
            assert!(divisor > 0);
        }
        assert!(r.stat_min() >= 1);
        assert!(r.stat_min() <= r.stat_max());
        assert!(r.hp_base() > 0);

        // max_hp and the largest possible damage fit in u16.
        let max_hp = r.hp_base() + r.hp_per_point() * u32::from(r.stat_max());
        assert!(max_hp <= u32::from(u16::MAX));
        let power = u32::from(r.special_power_max()).max(r.strike_power());
        let base = power * (r.damage_offset() + u32::from(r.stat_max()))
            / (r.damage_offset() + u32::from(r.stat_min()));
        let mut d = base * (r.variance_min() + r.variance_span() - 1) / 100;
        d = d * r.desperation_dmg_num() / r.desperation_dmg_den();
        d = d * r.crit_num() / r.crit_den();
        d = d * r.weak_num() / r.weak_den();
        assert!(d <= u32::from(u16::MAX));

        // Probabilities stay within 1000 ‰, and times fit in u32.
        let dodge = r.dodge_base() + r.dodge_per_speed() * u32::from(r.stat_max());
        let block = r.block_base()
            + u32::from(r.stat_max()) / r.block_defense_div()
            + u32::from(r.personality_max());
        assert!(dodge + block <= 1000);
        assert!(r.time_limit_ms().checked_add(r.end_ms()).is_some());
        let latest = [
            r.interval_base_ms(),
            r.jitter_ms(),
            r.breather_min_ms(),
            r.breather_span_ms(),
            u32::from(r.knockdown_ms()),
            r.recovery_ms(),
        ]
        .into_iter()
        .try_fold(r.time_limit_ms(), u32::checked_add);
        assert!(latest.is_some());
    }
}
