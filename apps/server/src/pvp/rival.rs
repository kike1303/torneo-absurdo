//! The rival of last resort (021 "Sin rival"): a character of the tournament scaled to the level of
//! the attacker. Pure: the specs come from the roster, and the pick is the caller's.
//!
//! Scaling keeps the shape of the character and the rules of a botarga of that level: every stat
//! starts at the minimum of the player ruleset and the rest of the budget of the level is shared in
//! the proportions of the tournament stats (rounded down, capped by the ruleset, so the sum never
//! passes the budget); the abilities are the first ones of the character that fit the open slots
//! and the per-kind limits of the player ruleset. The personality and the weaknesses stay.

use engine::loadout::{budget, slots};
use engine::{CharacterSpec, MoveKind, Ruleset, Stats};

/// `spec` as a rival of a botarga of `level`, valid for `ruleset`, or `None` if it cannot be one.
#[must_use]
pub fn scaled(spec: &CharacterSpec, level: u8, ruleset: &Ruleset) -> Option<CharacterSpec> {
    let budget = u32::from(budget(level)?);
    let s = spec.stats;
    let weights = [s.hp, s.attack, s.defense, s.speed, s.special].map(u32::from);
    let min = u32::from(ruleset.stat_min());
    let max = u32::from(ruleset.stat_max());
    let extra = budget.checked_sub(min * 5)?;
    let total: u32 = weights.iter().sum();
    let mut stats = [0_u8; 5];
    for (out, weight) in stats.iter_mut().zip(weights) {
        let share = extra.checked_mul(weight)?.checked_div(total).unwrap_or(0);
        *out = u8::try_from((min + share).min(max)).ok()?;
    }
    let [hp, attack, defense, speed, special] = stats;
    let open = slots(level);
    let (mut heavies, mut specials) = (0_u8, 0_u8);
    let mut abilities = Vec::new();
    for ability in &spec.abilities {
        if abilities.len() >= open {
            break;
        }
        let (used, limit) = match ability.kind {
            MoveKind::Heavy => (&mut heavies, ruleset.heavies_max()),
            MoveKind::Special => (&mut specials, ruleset.specials_max()),
        };
        if *used < limit {
            *used += 1;
            abilities.push(ability.clone());
        }
    }
    let rival = CharacterSpec {
        slug: spec.slug.clone(),
        stats: Stats {
            hp,
            attack,
            defense,
            speed,
            special,
        },
        personality: spec.personality,
        abilities,
        weaknesses: spec.weaknesses.clone(),
    };
    rival.validate(ruleset).ok()?;
    Some(rival)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use engine::{Ability, Personality};

    use super::*;

    fn v2() -> Ruleset {
        Ruleset::for_version(2).unwrap()
    }

    fn ability(id: &str, kind: MoveKind) -> Ability {
        let (power, cooldown) = match kind {
            MoveKind::Heavy => (40, 1),
            MoveKind::Special => (70, 4),
        };
        Ability {
            id: id.to_owned(),
            kind,
            power,
            cooldown,
            element: None,
        }
    }

    fn character(stats: [u8; 5], abilities: Vec<Ability>) -> CharacterSpec {
        let [hp, attack, defense, speed, special] = stats;
        CharacterSpec {
            slug: "rata-electrica".to_owned(),
            stats: Stats {
                hp,
                attack,
                defense,
                speed,
                special,
            },
            personality: Personality {
                aggression: 70,
                caution: 20,
                showmanship: 50,
            },
            abilities,
            weaknesses: Vec::new(),
        }
    }

    fn sum(spec: &CharacterSpec) -> u16 {
        let s = spec.stats;
        [s.hp, s.attack, s.defense, s.speed, s.special]
            .iter()
            .map(|&v| u16::from(v))
            .sum()
    }

    #[test]
    fn a_rival_stays_inside_the_budget_and_the_slots_of_the_level() {
        let heavy = |id| ability(id, MoveKind::Heavy);
        let special = |id| ability(id, MoveKind::Special);
        let spec = character(
            [100, 100, 60, 30, 10],
            vec![
                heavy("one"),
                heavy("two"),
                heavy("three"),
                special("four"),
                special("five"),
            ],
        );
        for level in [1, 4, 5, 15, 30, 60, 100] {
            let rival = scaled(&spec, level, &v2()).unwrap();
            assert!(sum(&rival) <= budget(level).unwrap(), "{level}");
            assert!(rival.abilities.len() <= slots(level), "{level}");
            let s = rival.stats;
            assert!(
                [s.hp, s.attack, s.defense, s.speed, s.special]
                    .iter()
                    .all(|&v| v >= 20)
            );
            // The order of the stats of the character holds.
            assert!(s.hp >= s.defense && s.defense >= s.speed && s.speed >= s.special);
            assert_eq!(rival.slug, "rata-electrica");
            assert_eq!(rival.personality, spec.personality);
        }
        let ids = |level| -> Vec<String> {
            scaled(&spec, level, &v2())
                .unwrap()
                .abilities
                .into_iter()
                .map(|a| a.id)
                .collect()
        };
        assert_eq!(ids(1), ["one"]);
        // A third heavy does not fit the player ruleset: the next special takes its slot.
        assert_eq!(ids(15), ["one", "two", "four"]);
        assert_eq!(ids(30), ["one", "two", "four", "five"]);
    }

    #[test]
    fn a_character_that_cannot_be_a_rival_is_none() {
        // A level outside the range has no budget.
        let spec = character([50, 50, 50, 50, 50], vec![ability("one", MoveKind::Heavy)]);
        assert!(scaled(&spec, 0, &v2()).is_none());
        assert!(scaled(&spec, 101, &v2()).is_none());
        assert!(scaled(&spec, 1, &v2()).is_some());
    }

    #[test]
    fn the_real_roster_gives_a_rival_at_every_level() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/characters");
        let roster = crate::fights::roster::rival_specs(&dir).unwrap();
        assert!(!roster.is_empty());
        for spec in &roster {
            for level in 1..=100 {
                assert!(
                    scaled(spec, level, &v2()).is_some(),
                    "{} at {level}",
                    spec.slug
                );
            }
        }
    }
}
