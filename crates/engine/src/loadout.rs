//! Player botarga loadouts (016): from what a player equips to the [`CharacterSpec`] the engine
//! fights with.
//!
//! A [`Loadout`] is the base stats the player spent, a fighting style, up to four abilities of the
//! catalog and one accessory per slot. [`resolve`] checks the progression rules (level, stat budget
//! of the level, unlocked ability slots, zero-sum accessories) and builds the spec: stats plus the
//! accessory deltas, and the weaknesses that the elemental abilities bring. The fight only needs
//! the resolved spec, so these rules are not part of a [`Ruleset`] version: changing them never
//! changes the result of a fight already played.
use serde::{Deserialize, Serialize};

use crate::spec::is_code;
use crate::{Ability, CharacterSpec, Element, Personality, Ruleset, SpecError, Stats};

/// Highest player level.
pub const LEVEL_MAX: u8 = 100;

/// Stat budget at level 1; it grows linearly up to `BUDGET_AT_LEVEL_MAX`.
pub const BUDGET_AT_LEVEL_1: u16 = 200;

/// Stat budget at [`LEVEL_MAX`], the same as the tournament characters.
pub const BUDGET_AT_LEVEL_MAX: u16 = 300;

/// Level at which each ability slot opens (the first one from the start).
pub const SLOT_LEVELS: [u8; 4] = [1, 5, 15, 30];

/// A level that is a multiple of this and does not open a slot draws a new ability (016).
pub const DRAW_EVERY: u8 = 5;

/// Catalog tiers: tier `t` covers levels `(t - 1) * LEVELS_PER_TIER` up to the next tier, and the
/// last one reaches [`LEVEL_MAX`].
pub const TIERS: u8 = 5;
pub const LEVELS_PER_TIER: u8 = 20;

/// Largest change one accessory makes to one stat, up or down.
pub const ACCESSORY_DELTA_MAX: u8 = 10;

/// Largest change all the equipped accessories together make to one stat, up or down.
pub const ACCESSORY_STAT_TOTAL_MAX: u8 = 20;

/// The stat budget of a level: [`BUDGET_AT_LEVEL_1`] at level 1 and [`BUDGET_AT_LEVEL_MAX`] at
/// [`LEVEL_MAX`], rounding down in between. `None` outside `1..=LEVEL_MAX`.
#[must_use]
pub const fn budget(level: u8) -> Option<u16> {
    if level == 0 || level > LEVEL_MAX {
        return None;
    }
    let span = BUDGET_AT_LEVEL_MAX - BUDGET_AT_LEVEL_1;
    let steps = (LEVEL_MAX - 1) as u16;
    Some(BUDGET_AT_LEVEL_1 + (level as u16 - 1) * span / steps)
}

/// How many ability slots are open at a level (0 outside `1..=LEVEL_MAX`).
#[must_use]
pub fn slots(level: u8) -> usize {
    if level == 0 || level > LEVEL_MAX {
        return 0;
    }
    SLOT_LEVELS.iter().filter(|&&from| from <= level).count()
}

/// How many ability draws a player has had up to `level` (0 outside `1..=LEVEL_MAX`): one at
/// every multiple of [`DRAW_EVERY`] that does not open a slot.
#[must_use]
pub fn draws(level: u8) -> usize {
    if level == 0 || level > LEVEL_MAX {
        return 0;
    }
    (1..=level)
        .filter(|l| l % DRAW_EVERY == 0 && !SLOT_LEVELS.contains(l))
        .count()
}

/// The highest catalog tier a level reaches (`None` outside `1..=LEVEL_MAX`).
#[must_use]
pub const fn tier(level: u8) -> Option<u8> {
    if level == 0 || level > LEVEL_MAX {
        return None;
    }
    let t = level / LEVELS_PER_TIER + 1;
    Some(if t > TIERS { TIERS } else { t })
}

/// The weakness an equipped ability of `element` brings to its owner (016): every element is weak
/// to the next one of the cycle fire, water, electric, ground, air, ice.
#[must_use]
pub const fn weakness_of(element: Element) -> Element {
    match element {
        Element::Fire => Element::Water,
        Element::Water => Element::Electric,
        Element::Electric => Element::Ground,
        Element::Ground => Element::Air,
        Element::Air => Element::Ice,
        Element::Ice => Element::Fire,
    }
}

/// Clothing slot of an accessory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Head,
    Hands,
    Torso,
    Legs,
    Feet,
}

/// What an accessory adds to (or takes from) each stat. Absent fields are 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StatDeltas {
    pub hp: i8,
    pub attack: i8,
    pub defense: i8,
    pub speed: i8,
    pub special: i8,
}

impl StatDeltas {
    const fn all(self) -> [i8; 5] {
        [self.hp, self.attack, self.defense, self.speed, self.special]
    }
}

/// An accessory of the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accessory {
    pub id: String,
    pub slot: Slot,
    pub deltas: StatDeltas,
}

impl Accessory {
    /// An accessory moves points between stats and never adds any (016): its deltas add up to 0,
    /// at least one is not 0, and none goes beyond [`ACCESSORY_DELTA_MAX`].
    ///
    /// # Errors
    /// `AccessoryIdInvalid` or `AccessoryDeltas`.
    pub fn validate(&self) -> Result<(), LoadoutError> {
        if !is_code(&self.id, b'_') {
            return Err(LoadoutError::AccessoryIdInvalid);
        }
        let deltas = self.deltas.all();
        let sum: i16 = deltas.iter().map(|&d| i16::from(d)).sum();
        let in_range = deltas
            .iter()
            .all(|d| d.unsigned_abs() <= ACCESSORY_DELTA_MAX);
        if sum != 0 || !in_range || deltas.iter().all(|&d| d == 0) {
            return Err(LoadoutError::AccessoryDeltas);
        }
        Ok(())
    }
}

/// What a player equips. `stats` are the points they spent, before the accessories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loadout {
    pub slug: String,
    pub level: u8,
    pub stats: Stats,
    pub personality: Personality,
    pub abilities: Vec<Ability>,
    pub accessories: Vec<Accessory>,
}

/// Why a loadout cannot fight. Codes only, no text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadoutError {
    /// `level` outside `1..=LEVEL_MAX`.
    LevelOutOfRange,
    /// The spent stats add up to more than the budget of the level.
    BudgetExceeded,
    /// More abilities than the slots open at the level.
    SlotsLocked,
    /// An accessory `id` does not match `^[a-z][a-z0-9_]{1,31}$`.
    AccessoryIdInvalid,
    /// An accessory that adds points, changes nothing or moves a stat beyond the cap.
    AccessoryDeltas,
    /// Two accessories in the same slot.
    AccessorySlotDuplicate,
    /// The accessories together move a stat beyond [`ACCESSORY_STAT_TOTAL_MAX`].
    AccessoryStatTotal,
    /// The resolved spec breaks a rule of the ruleset.
    Spec(SpecError),
}

impl LoadoutError {
    /// Stable `snake_case` code; a spec error is `spec.<code>`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::LevelOutOfRange => "level_out_of_range",
            Self::BudgetExceeded => "budget_exceeded",
            Self::SlotsLocked => "slots_locked",
            Self::AccessoryIdInvalid => "accessory_id_invalid",
            Self::AccessoryDeltas => "accessory_deltas",
            Self::AccessorySlotDuplicate => "accessory_slot_duplicate",
            Self::AccessoryStatTotal => "accessory_stat_total",
            Self::Spec(_) => "spec",
        }
    }
}

impl std::fmt::Display for LoadoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spec(error) => write!(f, "spec.{}", error.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for LoadoutError {}

/// The spec a loadout fights with, validated against `ruleset` (the player one is version 2).
///
/// # Errors
/// The first rule that fails: level, open slots, each accessory, slots of the accessories, their
/// totals, the budget of the level, and then the rules of the ruleset (`Spec`).
pub fn resolve(loadout: &Loadout, ruleset: &Ruleset) -> Result<CharacterSpec, LoadoutError> {
    let budget = budget(loadout.level).ok_or(LoadoutError::LevelOutOfRange)?;
    if loadout.abilities.len() > slots(loadout.level) {
        return Err(LoadoutError::SlotsLocked);
    }
    let mut totals = [0_i16; 5];
    for (i, accessory) in loadout.accessories.iter().enumerate() {
        accessory.validate()?;
        if loadout
            .accessories
            .iter()
            .take(i)
            .any(|other| other.slot == accessory.slot)
        {
            return Err(LoadoutError::AccessorySlotDuplicate);
        }
        for (total, delta) in totals.iter_mut().zip(accessory.deltas.all()) {
            *total += i16::from(delta);
        }
    }
    if totals
        .iter()
        .any(|t| t.unsigned_abs() > u16::from(ACCESSORY_STAT_TOTAL_MAX))
    {
        return Err(LoadoutError::AccessoryStatTotal);
    }
    let s = loadout.stats;
    let base = [s.hp, s.attack, s.defense, s.speed, s.special];
    if base.iter().map(|&stat| u16::from(stat)).sum::<u16>() > budget {
        return Err(LoadoutError::BudgetExceeded);
    }
    let mut stats = [0_u8; 5];
    for ((out, stat), total) in stats.iter_mut().zip(base).zip(totals) {
        // Out of `u8` means far out of the ruleset range.
        *out = u8::try_from(i16::from(stat) + total)
            .map_err(|_| LoadoutError::Spec(SpecError::StatOutOfRange))?;
    }
    let [hp, attack, defense, speed, special] = stats;
    let mut weaknesses: Vec<Element> = Vec::new();
    for element in loadout.abilities.iter().filter_map(|a| a.element) {
        let weakness = weakness_of(element);
        if !weaknesses.contains(&weakness) {
            weaknesses.push(weakness);
        }
    }
    let spec = CharacterSpec {
        slug: loadout.slug.clone(),
        stats: Stats {
            hp,
            attack,
            defense,
            speed,
            special,
        },
        personality: loadout.personality,
        abilities: loadout.abilities.clone(),
        weaknesses,
    };
    spec.validate(ruleset).map_err(LoadoutError::Spec)?;
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MoveKind;

    fn ability(id: &str, kind: MoveKind, element: Option<Element>) -> Ability {
        let (power, cooldown) = match kind {
            MoveKind::Heavy => (40, 1),
            MoveKind::Special => (70, 4),
        };
        Ability {
            id: id.to_owned(),
            kind,
            power,
            cooldown,
            element,
        }
    }

    fn accessory(id: &str, slot: Slot, deltas: StatDeltas) -> Accessory {
        Accessory {
            id: id.to_owned(),
            slot,
            deltas,
        }
    }

    fn swap(attack: i8, speed: i8) -> StatDeltas {
        StatDeltas {
            attack,
            speed,
            ..StatDeltas::default()
        }
    }

    fn loadout() -> Loadout {
        Loadout {
            slug: "player-1".to_owned(),
            level: 30,
            stats: Stats {
                hp: 50,
                attack: 50,
                defense: 50,
                speed: 50,
                special: 29,
            },
            personality: Personality {
                aggression: 50,
                caution: 50,
                showmanship: 50,
            },
            abilities: vec![
                ability("foam_headbutt", MoveKind::Heavy, None),
                ability("kettle_blast", MoveKind::Special, Some(Element::Fire)),
            ],
            accessories: vec![accessory("boxing_mitts", Slot::Hands, swap(8, -8))],
        }
    }

    fn check(change: impl FnOnce(&mut Loadout)) -> Result<CharacterSpec, LoadoutError> {
        let mut l = loadout();
        change(&mut l);
        resolve(&l, &Ruleset::v2())
    }

    #[test]
    fn progression_curves() {
        assert_eq!(budget(0), None);
        assert_eq!(budget(1), Some(200));
        assert_eq!(budget(2), Some(201));
        assert_eq!(budget(50), Some(249));
        assert_eq!(budget(99), Some(298));
        assert_eq!(budget(100), Some(300));
        assert_eq!(budget(101), None);
        assert_eq!(BUDGET_AT_LEVEL_MAX, Ruleset::v2().stat_budget());

        let open: Vec<usize> = [0, 1, 4, 5, 14, 15, 29, 30, 100, 101]
            .iter()
            .map(|&l| slots(l))
            .collect();
        assert_eq!(open, vec![0, 1, 1, 2, 2, 3, 3, 4, 4, 0]);
        assert_eq!(slots(LEVEL_MAX), usize::from(Ruleset::v2().abilities_max()));

        let drawn: Vec<usize> = [0, 1, 5, 10, 15, 20, 30, 100, 101]
            .iter()
            .map(|&l| draws(l))
            .collect();
        assert_eq!(drawn, vec![0, 0, 0, 1, 1, 2, 3, 17, 0]);

        let tiers: Vec<Option<u8>> = [0, 1, 19, 20, 39, 40, 79, 80, 99, 100]
            .iter()
            .map(|&l| tier(l))
            .collect();
        assert_eq!(
            tiers,
            vec![
                None,
                Some(1),
                Some(1),
                Some(2),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(5),
                Some(5)
            ]
        );
    }

    #[test]
    fn the_weakness_cycle_visits_every_element_once() {
        let mut element = Element::Fire;
        let mut seen = Vec::new();
        for _ in 0..6 {
            assert!(!seen.contains(&element));
            seen.push(element);
            element = weakness_of(element);
        }
        assert_eq!(element, Element::Fire);
        assert_eq!(weakness_of(Element::Electric), Element::Ground);
    }

    #[test]
    fn a_valid_loadout_resolves_to_its_spec() {
        let spec = check(|_| {}).unwrap();
        assert_eq!(spec.slug, "player-1");
        assert_eq!(spec.stats.attack, 58);
        assert_eq!(spec.stats.speed, 42);
        assert_eq!(spec.stats.hp, 50);
        // The fire special brings the water weakness.
        assert_eq!(spec.weaknesses, vec![Element::Water]);
        assert_eq!(spec.abilities.len(), 2);
    }

    #[test]
    fn weaknesses_come_from_the_elements_in_file_order_without_repeats() {
        let spec = check(|l| {
            l.abilities = vec![
                ability("ice_a", MoveKind::Heavy, Some(Element::Ice)),
                ability("plain", MoveKind::Heavy, None),
                ability("ice_b", MoveKind::Special, Some(Element::Ice)),
                ability("zap", MoveKind::Special, Some(Element::Electric)),
            ];
        })
        .unwrap();
        assert_eq!(spec.weaknesses, vec![Element::Fire, Element::Ground]);
        // A third element brings a third weakness: not allowed.
        assert_eq!(
            check(|l| {
                l.abilities = vec![
                    ability("ice_a", MoveKind::Heavy, Some(Element::Ice)),
                    ability("burn", MoveKind::Heavy, Some(Element::Fire)),
                    ability("zap", MoveKind::Special, Some(Element::Electric)),
                ];
            }),
            Err(LoadoutError::Spec(SpecError::WeaknessCount))
        );
    }

    #[test]
    fn level_budget_and_slots() {
        assert_eq!(check(|l| l.level = 0), Err(LoadoutError::LevelOutOfRange));
        assert_eq!(check(|l| l.level = 101), Err(LoadoutError::LevelOutOfRange));
        // 229 points: level 30 has 229.
        assert_eq!(budget(30), Some(229));
        assert_eq!(
            check(|l| l.stats.special = 30),
            Err(LoadoutError::BudgetExceeded)
        );
        assert_eq!(check(|l| l.level = 4), Err(LoadoutError::SlotsLocked));
        assert_eq!(
            check(|l| {
                l.level = 5;
                l.stats.hp = 20;
            })
            .map(|s| s.abilities.len()),
            Ok(2)
        );
    }

    #[test]
    fn accessories_are_zero_sum_and_capped() {
        let bad = [
            swap(8, -7),
            swap(11, -11),
            StatDeltas::default(),
            StatDeltas {
                hp: 5,
                attack: 5,
                speed: -10,
                ..StatDeltas::default()
            },
        ];
        for (i, deltas) in bad.into_iter().enumerate() {
            let expected = if i == 3 {
                Ok(())
            } else {
                Err(LoadoutError::AccessoryDeltas)
            };
            assert_eq!(
                accessory("glove", Slot::Hands, deltas).validate(),
                expected,
                "{deltas:?}"
            );
        }
        assert_eq!(
            accessory("Glove", Slot::Hands, swap(1, -1)).validate(),
            Err(LoadoutError::AccessoryIdInvalid)
        );
        assert_eq!(
            check(|l| l
                .accessories
                .push(accessory("mitts", Slot::Hands, swap(-2, 2)))),
            Err(LoadoutError::AccessorySlotDuplicate)
        );
        // 3 accessories of +8 attack: 24 > 20.
        assert_eq!(
            check(|l| {
                l.accessories
                    .push(accessory("helmet", Slot::Head, swap(8, -8)));
                l.accessories.push(accessory(
                    "boots",
                    Slot::Feet,
                    StatDeltas {
                        attack: 8,
                        defense: -8,
                        ..StatDeltas::default()
                    },
                ));
            }),
            Err(LoadoutError::AccessoryStatTotal)
        );
    }

    #[test]
    fn the_ruleset_still_rules_the_resolved_spec() {
        // An accessory can push a stat below the v2 minimum of 20.
        assert_eq!(
            check(|l| {
                l.stats.speed = 25;
                l.stats.hp = 75;
            }),
            Err(LoadoutError::Spec(SpecError::StatOutOfRange))
        );
        assert_eq!(
            check(|l| l.stats.speed = 5),
            Err(LoadoutError::Spec(SpecError::StatOutOfRange))
        );
        assert_eq!(
            check(|l| l.personality.caution = 101),
            Err(LoadoutError::Spec(SpecError::PersonalityOutOfRange))
        );
        assert_eq!(
            check(|l| l.abilities.clear()),
            Err(LoadoutError::Spec(SpecError::AbilityCount))
        );
    }

    #[test]
    fn codes_are_snake_case() {
        let all = [
            LoadoutError::LevelOutOfRange,
            LoadoutError::BudgetExceeded,
            LoadoutError::SlotsLocked,
            LoadoutError::AccessoryIdInvalid,
            LoadoutError::AccessoryDeltas,
            LoadoutError::AccessorySlotDuplicate,
            LoadoutError::AccessoryStatTotal,
            LoadoutError::Spec(SpecError::WeaknessCount),
        ];
        for error in all {
            assert!(
                error
                    .code()
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c == b'_')
            );
        }
        assert_eq!(
            LoadoutError::Spec(SpecError::WeaknessCount).to_string(),
            "spec.weakness_count"
        );
    }

    #[test]
    fn accessory_json_shape() {
        let json = r#"{"id":"boxing_mitts","slot":"hands","deltas":{"attack":8,"speed":-8}}"#;
        let a: Accessory = serde_json::from_str(json).unwrap();
        assert_eq!(a.deltas, swap(8, -8));
        assert!(
            serde_json::from_str::<Accessory>(r#"{"id":"x_y","slot":"tail","deltas":{}}"#).is_err()
        );
        assert!(
            serde_json::from_str::<Accessory>(r#"{"id":"x_y","slot":"head","deltas":{"luck":1}}"#)
                .is_err()
        );
    }
}
