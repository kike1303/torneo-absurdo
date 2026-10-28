//! Engine shape of a character (003 RF4, RF12, "Esquema de personaje") and its validation.
//!
//! The file `content/characters/<slug>/character.json` adds `archetype`, `ruleset_version`
//! and `rivals`; `crates/content` drops them. This shape is the one `canonical_spec_json`
//! serializes and the only one the WASM bindings accept.
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Ruleset;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterSpec {
    pub slug: String,
    pub stats: Stats,
    pub personality: Personality,
    pub abilities: Vec<Ability>,
    pub weaknesses: Vec<Element>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    pub hp: u8,
    pub attack: u8,
    pub defense: u8,
    pub speed: u8,
    pub special: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    pub aggression: u8,
    pub caution: u8,
    pub showmanship: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ability {
    pub id: String,
    pub kind: MoveKind,
    pub power: u8,
    pub cooldown: u8,
    /// Optional in the input; the canonical form always writes it (`null` if absent).
    pub element: Option<Element>,
}

/// Kind of an ability (the basic strike is not an ability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveKind {
    Heavy,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Fire,
    Water,
    Electric,
    Ground,
    Air,
    Ice,
}

/// Why a character (or, for `EventLimit`, a ruleset) cannot fight. Codes only, no text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecError {
    /// `slug` does not match `^[a-z][a-z0-9-]{1,31}$`.
    SlugInvalid,
    /// A stat is outside `stat_min..=stat_max`.
    StatOutOfRange,
    /// The stats add up to more than `stat_budget`.
    BudgetExceeded,
    /// A personality trait is above `personality_max`.
    PersonalityOutOfRange,
    /// Fewer than `abilities_min` or more than `abilities_max` abilities.
    AbilityCount,
    /// An ability `id` does not match `^[a-z][a-z0-9_]{1,31}$` or is `strike`.
    AbilityIdInvalid,
    /// Two abilities share an `id`.
    AbilityIdDuplicate,
    /// `power` outside the range of its `kind`.
    PowerOutOfRange,
    /// `cooldown` outside the range of its `kind`.
    CooldownOutOfRange,
    /// More than `weaknesses_max` weaknesses.
    WeaknessCount,
    /// A weakness appears twice.
    WeaknessDuplicate,
    /// The fight would exceed `max_events`. Unreachable with a ruleset that passes the bounds
    /// test (decision 1 of plan-a): it is a guard against a badly calibrated future ruleset,
    /// so the engine neither truncates the log nor panics.
    EventLimit,
}

impl SpecError {
    /// Every variant, once.
    pub const ALL: [Self; 12] = [
        Self::SlugInvalid,
        Self::StatOutOfRange,
        Self::BudgetExceeded,
        Self::PersonalityOutOfRange,
        Self::AbilityCount,
        Self::AbilityIdInvalid,
        Self::AbilityIdDuplicate,
        Self::PowerOutOfRange,
        Self::CooldownOutOfRange,
        Self::WeaknessCount,
        Self::WeaknessDuplicate,
        Self::EventLimit,
    ];

    /// Stable `snake_case` code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SlugInvalid => "slug_invalid",
            Self::StatOutOfRange => "stat_out_of_range",
            Self::BudgetExceeded => "budget_exceeded",
            Self::PersonalityOutOfRange => "personality_out_of_range",
            Self::AbilityCount => "ability_count",
            Self::AbilityIdInvalid => "ability_id_invalid",
            Self::AbilityIdDuplicate => "ability_id_duplicate",
            Self::PowerOutOfRange => "power_out_of_range",
            Self::CooldownOutOfRange => "cooldown_out_of_range",
            Self::WeaknessCount => "weakness_count",
            Self::WeaknessDuplicate => "weakness_duplicate",
            Self::EventLimit => "event_limit",
        }
    }
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for SpecError {}

/// The basic strike's `ability` code in the log; no ability may use it.
pub(crate) const STRIKE_ID: &str = "strike";

/// `^[a-z][a-z0-9<extra>]{1,31}$` (format rules, not game rules: they stay out of `Ruleset`).
fn is_code(value: &str, extra: u8) -> bool {
    let bytes = value.as_bytes();
    let Some((first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && (1..=31).contains(&rest.len())
        && rest
            .iter()
            .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == extra)
}

/// Whether `value` is a valid character code: `^[a-z][a-z0-9-]{1,31}$`.
///
/// A format rule, not a game rule, so it is not part of [`Ruleset`]. [`CharacterSpec::validate`]
/// applies it to `slug`, and `crates/content` to the fields that are not in the engine shape
/// (`archetype`), so the pattern has a single definition (003 RF12).
#[must_use]
pub fn is_valid_slug(value: &str) -> bool {
    is_code(value, b'-')
}

impl CharacterSpec {
    /// Checks every rule of the character against the ruleset (ranges, budget, uniqueness and
    /// the code patterns). `content` and the WASM bindings use this same function.
    ///
    /// # Errors
    /// The first rule that fails, in the order of the [`SpecError`] variants.
    pub fn validate(&self, ruleset: &Ruleset) -> Result<(), SpecError> {
        if !is_valid_slug(&self.slug) {
            return Err(SpecError::SlugInvalid);
        }
        let s = self.stats;
        let stats = [s.hp, s.attack, s.defense, s.speed, s.special];
        let range = ruleset.stat_min()..=ruleset.stat_max();
        if !stats.iter().all(|stat| range.contains(stat)) {
            return Err(SpecError::StatOutOfRange);
        }
        let total: u16 = stats.iter().map(|&stat| u16::from(stat)).sum();
        if total > ruleset.stat_budget() {
            return Err(SpecError::BudgetExceeded);
        }
        let p = self.personality;
        if [p.aggression, p.caution, p.showmanship]
            .iter()
            .any(|&trait_value| trait_value > ruleset.personality_max())
        {
            return Err(SpecError::PersonalityOutOfRange);
        }
        let count = self.abilities.len();
        if count < usize::from(ruleset.abilities_min())
            || count > usize::from(ruleset.abilities_max())
        {
            return Err(SpecError::AbilityCount);
        }
        for (i, ability) in self.abilities.iter().enumerate() {
            if !is_code(&ability.id, b'_') || ability.id == STRIKE_ID {
                return Err(SpecError::AbilityIdInvalid);
            }
            if self
                .abilities
                .iter()
                .take(i)
                .any(|other| other.id == ability.id)
            {
                return Err(SpecError::AbilityIdDuplicate);
            }
            let (power, cooldown) = match ability.kind {
                MoveKind::Heavy => (
                    ruleset.heavy_power_min()..=ruleset.heavy_power_max(),
                    ruleset.heavy_cooldown_min()..=ruleset.heavy_cooldown_max(),
                ),
                MoveKind::Special => (
                    ruleset.special_power_min()..=ruleset.special_power_max(),
                    ruleset.special_cooldown_min()..=ruleset.special_cooldown_max(),
                ),
            };
            if !power.contains(&ability.power) {
                return Err(SpecError::PowerOutOfRange);
            }
            if !cooldown.contains(&ability.cooldown) {
                return Err(SpecError::CooldownOutOfRange);
            }
        }
        if self.weaknesses.len() > usize::from(ruleset.weaknesses_max()) {
            return Err(SpecError::WeaknessCount);
        }
        for (i, weakness) in self.weaknesses.iter().enumerate() {
            if self
                .weaknesses
                .iter()
                .take(i)
                .any(|other| other == weakness)
            {
                return Err(SpecError::WeaknessDuplicate);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> CharacterSpec {
        CharacterSpec {
            slug: "rata-electrica".to_owned(),
            stats: Stats {
                hp: 55,
                attack: 60,
                defense: 45,
                speed: 85,
                special: 55,
            },
            personality: Personality {
                aggression: 70,
                caution: 20,
                showmanship: 90,
            },
            abilities: vec![Ability {
                id: "thunder_squeak".to_owned(),
                kind: MoveKind::Special,
                power: 70,
                cooldown: 3,
                element: Some(Element::Electric),
            }],
            weaknesses: vec![Element::Ground],
        }
    }

    fn check(change: impl FnOnce(&mut CharacterSpec)) -> Result<(), SpecError> {
        let mut spec = valid();
        change(&mut spec);
        spec.validate(&Ruleset::v1())
    }

    fn heavy(id: &str, power: u8, cooldown: u8) -> Ability {
        Ability {
            id: id.to_owned(),
            kind: MoveKind::Heavy,
            power,
            cooldown,
            element: None,
        }
    }

    #[test]
    fn the_spec_example_is_valid() {
        assert_eq!(check(|_| {}), Ok(()));
    }

    fn slug_cases() -> (Vec<String>, Vec<String>) {
        let ok = ["ab", "a-1", "rata-electrica"]
            .map(str::to_owned)
            .into_iter()
            .chain([format!("a{}", "b".repeat(31))])
            .collect();
        let bad = ["", "a", "1a", "-a", "Ab", "a_b", "a b", "ñu"]
            .map(str::to_owned)
            .into_iter()
            .chain([format!("a{}", "b".repeat(32))])
            .collect();
        (ok, bad)
    }

    #[test]
    fn slug_pattern() {
        let (ok, bad) = slug_cases();
        for value in ok {
            assert_eq!(check(|s| s.slug.clone_from(&value)), Ok(()), "{value}");
        }
        for value in bad {
            assert_eq!(
                check(|s| s.slug.clone_from(&value)),
                Err(SpecError::SlugInvalid),
                "{value}"
            );
        }
    }

    #[test]
    fn is_valid_slug_is_the_rule_validate_uses() {
        let (ok, bad) = slug_cases();
        for value in ok {
            assert!(is_valid_slug(&value), "{value}");
        }
        for value in bad {
            assert!(!is_valid_slug(&value), "{value}");
        }
    }

    #[test]
    fn stats_and_budget() {
        assert_eq!(check(|s| s.stats.hp = 0), Err(SpecError::StatOutOfRange));
        assert_eq!(
            check(|s| s.stats.special = 101),
            Err(SpecError::StatOutOfRange)
        );
        assert_eq!(
            check(|s| s.stats.attack = 61),
            Err(SpecError::BudgetExceeded)
        );
        assert_eq!(check(|s| s.stats.attack = 59), Ok(()));
        let all = |v| Stats {
            hp: v,
            attack: v,
            defense: v,
            speed: v,
            special: v,
        };
        assert_eq!(check(|s| s.stats = all(60)), Ok(()));
        assert_eq!(check(|s| s.stats = all(1)), Ok(()));
        assert_eq!(
            check(|s| s.stats = all(100)),
            Err(SpecError::BudgetExceeded)
        );
    }

    #[test]
    fn personality() {
        assert_eq!(check(|s| s.personality.caution = 0), Ok(()));
        assert_eq!(check(|s| s.personality.caution = 100), Ok(()));
        assert_eq!(
            check(|s| s.personality.showmanship = 101),
            Err(SpecError::PersonalityOutOfRange)
        );
    }

    #[test]
    fn abilities() {
        assert_eq!(check(|s| s.abilities.clear()), Err(SpecError::AbilityCount));
        assert_eq!(
            check(|s| s.abilities = (0..5).map(|i| heavy(&format!("h{i}"), 20, 0)).collect()),
            Err(SpecError::AbilityCount)
        );
        assert_eq!(
            check(|s| s.abilities = (0..4).map(|i| heavy(&format!("h{i}"), 20, 0)).collect()),
            Ok(())
        );
        for bad in ["strike", "a", "A_b", "a-b", "_a"] {
            assert_eq!(
                check(|s| s.abilities = vec![heavy(bad, 20, 0)]),
                Err(SpecError::AbilityIdInvalid),
                "{bad}"
            );
        }
        assert_eq!(
            check(|s| s.abilities = vec![heavy("ab", 20, 0), heavy("ab", 30, 1)]),
            Err(SpecError::AbilityIdDuplicate)
        );
    }

    #[test]
    fn power_and_cooldown_depend_on_the_kind() {
        let ranges = [
            (MoveKind::Heavy, 20, 60, 0, 5),
            (MoveKind::Special, 40, 100, 2, 8),
        ];
        for (kind, pmin, pmax, cmin, cmax) in ranges {
            let with = |power, cooldown| {
                check(|s| {
                    s.abilities = vec![Ability {
                        id: "move_one".to_owned(),
                        kind,
                        power,
                        cooldown,
                        element: None,
                    }];
                })
            };
            assert_eq!(with(pmin, cmin), Ok(()));
            assert_eq!(with(pmax, cmax), Ok(()));
            assert_eq!(with(pmin - 1, cmin), Err(SpecError::PowerOutOfRange));
            assert_eq!(with(pmax + 1, cmin), Err(SpecError::PowerOutOfRange));
            assert_eq!(with(pmin, cmax + 1), Err(SpecError::CooldownOutOfRange));
            if cmin > 0 {
                assert_eq!(with(pmin, cmin - 1), Err(SpecError::CooldownOutOfRange));
            }
        }
    }

    #[test]
    fn weaknesses() {
        assert_eq!(check(|s| s.weaknesses.clear()), Ok(()));
        assert_eq!(
            check(|s| s.weaknesses = vec![Element::Fire, Element::Ice]),
            Ok(())
        );
        assert_eq!(
            check(|s| s.weaknesses = vec![Element::Fire, Element::Ice, Element::Air]),
            Err(SpecError::WeaknessCount)
        );
        assert_eq!(
            check(|s| s.weaknesses = vec![Element::Fire, Element::Fire]),
            Err(SpecError::WeaknessDuplicate)
        );
    }

    #[test]
    fn codes_are_unique_snake_case_and_cover_every_variant() {
        let codes: Vec<&str> = SpecError::ALL.iter().map(|e| e.code()).collect();
        for (i, code) in codes.iter().enumerate() {
            assert!(
                code.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'),
                "{code}"
            );
            assert!(!codes.iter().take(i).any(|other| other == code), "{code}");
        }
        // A new variant must be added to ALL: this match fails to compile otherwise, and the
        // count below fails if it is added to the match but not to ALL.
        let index = |e: SpecError| match e {
            SpecError::SlugInvalid => 0,
            SpecError::StatOutOfRange => 1,
            SpecError::BudgetExceeded => 2,
            SpecError::PersonalityOutOfRange => 3,
            SpecError::AbilityCount => 4,
            SpecError::AbilityIdInvalid => 5,
            SpecError::AbilityIdDuplicate => 6,
            SpecError::PowerOutOfRange => 7,
            SpecError::CooldownOutOfRange => 8,
            SpecError::WeaknessCount => 9,
            SpecError::WeaknessDuplicate => 10,
            SpecError::EventLimit => 11,
        };
        let indices: Vec<usize> = SpecError::ALL.iter().map(|&e| index(e)).collect();
        assert_eq!(indices, (0..12).collect::<Vec<_>>());
        assert_eq!(SpecError::BudgetExceeded.to_string(), "budget_exceeded");
    }

    #[test]
    fn element_is_optional_in_the_input_and_null_in_the_output() {
        let json = r#"{"id":"ab","kind":"heavy","power":20,"cooldown":0}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(ability.element, None);
        assert_eq!(
            serde_json::to_string(&ability).unwrap(),
            r#"{"id":"ab","kind":"heavy","power":20,"cooldown":0,"element":null}"#
        );
    }

    #[test]
    fn unknown_fields_are_rejected_at_every_level() {
        let base = serde_json::to_value(valid()).unwrap();
        let paths: [&[&str]; 4] = [&[], &["stats"], &["personality"], &["abilities", "0"]];
        for path in paths {
            let mut value = base.clone();
            let mut target = &mut value;
            for key in path {
                target = match key.parse::<usize>() {
                    Ok(i) => target.get_mut(i).unwrap(),
                    Err(_) => target.get_mut(*key).unwrap(),
                };
            }
            target
                .as_object_mut()
                .unwrap()
                .insert("extra".to_owned(), serde_json::Value::Bool(true));
            assert!(
                serde_json::from_value::<CharacterSpec>(value).is_err(),
                "extra field at {path:?}"
            );
        }
        // The file shape (with archetype, ruleset_version, rivals) is not the engine shape.
        let mut file = base;
        file.as_object_mut().unwrap().insert(
            "archetype".to_owned(),
            serde_json::Value::String("x".to_owned()),
        );
        assert!(serde_json::from_value::<CharacterSpec>(file).is_err());
        // Closed enums.
        let bad_element = r#"{"id":"ab","kind":"heavy","power":20,"cooldown":0,"element":"wood"}"#;
        assert!(serde_json::from_str::<Ability>(bad_element).is_err());
        let bad_kind = r#"{"id":"ab","kind":"strike","power":20,"cooldown":0}"#;
        assert!(serde_json::from_str::<Ability>(bad_kind).is_err());
    }

    #[test]
    fn round_trip() {
        let spec = valid();
        let json = serde_json::to_vec(&spec).unwrap();
        assert_eq!(
            serde_json::from_slice::<CharacterSpec>(&json).unwrap(),
            spec
        );
    }
}
