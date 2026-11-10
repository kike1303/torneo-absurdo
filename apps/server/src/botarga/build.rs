//! The rules of a build a client sends (021 "Build"): the sizes, what the user owns, the catalog and
//! `engine::loadout::resolve` with the player ruleset. Pure: no database, no clock.

use engine::loadout::{self, LoadoutError};
use engine::{SpecError, Stats};
use protocol::{BotargaBuild, BotargaErrorCode, BotargaUnlocks};

use super::{BotargaCatalog, MAX_ACCESSORIES, MAX_ID_BYTES};

/// Whether the body has the sizes of a build: ids of [`MAX_ID_BYTES`] at most, and no more
/// abilities than the ruleset allows nor more accessories than slots. Checked before any query.
#[must_use]
pub fn well_formed(catalog: &BotargaCatalog, build: &BotargaBuild) -> bool {
    let id_fits = |id: &String| !id.is_empty() && id.len() <= MAX_ID_BYTES;
    id_fits(&build.style_id)
        && build.abilities.len() <= usize::from(catalog.ruleset().abilities_max())
        && build.accessories.len() <= MAX_ACCESSORIES
        && build.abilities.iter().all(id_fits)
        && build.accessories.iter().all(id_fits)
}

/// The code of a loadout the engine refused. Never its text.
const fn refusal(error: LoadoutError) -> BotargaErrorCode {
    match error {
        LoadoutError::BudgetExceeded | LoadoutError::Spec(SpecError::BudgetExceeded) => {
            BotargaErrorCode::BudgetExceeded
        }
        LoadoutError::SlotsLocked => BotargaErrorCode::SlotsLocked,
        LoadoutError::AccessorySlotDuplicate => BotargaErrorCode::AccessorySlotDuplicate,
        LoadoutError::AccessoryStatTotal => BotargaErrorCode::AccessoryStatTotal,
        LoadoutError::Spec(SpecError::StatOutOfRange) => BotargaErrorCode::StatOutOfRange,
        LoadoutError::Spec(SpecError::AbilityCount) => BotargaErrorCode::AbilityCount,
        LoadoutError::Spec(SpecError::AbilityKindCount) => BotargaErrorCode::AbilityKindCount,
        LoadoutError::Spec(SpecError::AbilityIdDuplicate) => BotargaErrorCode::AbilityDuplicate,
        LoadoutError::Spec(SpecError::WeaknessCount) => BotargaErrorCode::WeaknessCount,
        // The level is the server's and the rest is of the catalog, checked at startup.
        LoadoutError::LevelOutOfRange
        | LoadoutError::AccessoryIdInvalid
        | LoadoutError::AccessoryDeltas
        | LoadoutError::Spec(_) => BotargaErrorCode::BuildInvalid,
    }
}

/// Whether a botarga of `level` that owns `unlocks` may equip `build`.
///
/// # Errors
/// The first rule that fails: the style, what the user owns (an id the catalog lacks answers the
/// same), and then the rules of the loadout in the order of `resolve`.
pub fn check(
    catalog: &BotargaCatalog,
    level: u8,
    unlocks: &BotargaUnlocks,
    build: &BotargaBuild,
) -> Result<(), BotargaErrorCode> {
    if catalog.style(&build.style_id).is_none() {
        return Err(BotargaErrorCode::StyleUnknown);
    }
    let owned = |ids: &[String], unlocked: &[String]| ids.iter().all(|id| unlocked.contains(id));
    let cosmetics = [
        build.cosmetics.idle.map(protocol::IdleClip::as_str),
        build.cosmetics.victory.map(protocol::VictoryClip::as_str),
    ];
    if !owned(&build.abilities, &unlocks.abilities)
        || !owned(&build.accessories, &unlocks.accessories)
        || !cosmetics
            .into_iter()
            .flatten()
            .all(|code| unlocks.cosmetics.iter().any(|owned| owned == code))
    {
        return Err(BotargaErrorCode::NotUnlocked);
    }
    let s = build.stats;
    let stats = Stats {
        hp: s.hp,
        attack: s.attack,
        defense: s.defense,
        speed: s.speed,
        special: s.special,
    };
    // Owned but gone from the catalog: the same answer as not owned.
    let loadout = catalog
        .loadout(
            level,
            stats,
            &build.style_id,
            &build.abilities,
            &build.accessories,
        )
        .ok_or(BotargaErrorCode::NotUnlocked)?;
    loadout::resolve(&loadout, catalog.ruleset())
        .map(|_| ())
        .map_err(refusal)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use protocol::{BotargaStats, FighterCosmetics, IdleClip, VictoryClip};

    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    fn stats(each: u8) -> BotargaStats {
        BotargaStats {
            hp: each,
            attack: each,
            defense: each,
            speed: each,
            special: each,
        }
    }

    fn starter() -> BotargaBuild {
        BotargaBuild {
            stats: stats(40),
            style_id: "balanced".to_owned(),
            abilities: ids(&["foam_headbutt"]),
            accessories: Vec::new(),
            cosmetics: FighterCosmetics::default(),
        }
    }

    /// Everything of the catalog that the tests equip.
    fn everything() -> BotargaUnlocks {
        BotargaUnlocks {
            abilities: ids(&[
                "foam_headbutt",
                "spicy_slap",
                "puddle_punt",
                "wet_noodle_whip",
                "confetti_cannon",
                "joy_buzzer_hug",
                "dramatic_pause",
            ]),
            accessories: ids(&[
                "propeller_beanie",
                "tinfoil_crown",
                "spandex_bike_shorts",
                "grandma_roller_skates",
            ]),
            cosmetics: ids(&["idle_2", "win_3"]),
        }
    }

    fn check_at(
        level: u8,
        unlocks: &BotargaUnlocks,
        change: impl FnOnce(&mut BotargaBuild),
    ) -> Result<(), BotargaErrorCode> {
        let catalog = BotargaCatalog::embedded().unwrap();
        let mut build = starter();
        change(&mut build);
        check(&catalog, level, unlocks, &build)
    }

    fn starter_unlocks() -> BotargaUnlocks {
        BotargaUnlocks {
            abilities: ids(&["foam_headbutt"]),
            ..BotargaUnlocks::default()
        }
    }

    #[test]
    fn the_test_ids_are_of_the_catalog() {
        let catalog = BotargaCatalog::embedded().unwrap();
        let all = everything();
        for id in &all.abilities {
            assert!(catalog.ability(id).is_some(), "{id}");
        }
        for id in &all.accessories {
            assert!(catalog.accessory(id).is_some(), "{id}");
        }
    }

    #[test]
    fn the_starter_build_passes_and_stats_move_freely_inside_the_budget() {
        let own = starter_unlocks();
        assert_eq!(check_at(1, &own, |_| {}), Ok(()));
        assert_eq!(
            check_at(1, &own, |b| {
                b.stats.hp = 80;
                b.stats.special = 20;
                b.stats.speed = 20;
            }),
            Ok(())
        );
        // Under the budget is fine: the points stay free.
        assert_eq!(check_at(1, &own, |b| b.stats.hp = 30), Ok(()));
        assert_eq!(
            check_at(1, &own, |b| b.stats.hp = 41),
            Err(BotargaErrorCode::BudgetExceeded)
        );
        // Level 2 has one point more.
        assert_eq!(check_at(2, &own, |b| b.stats.hp = 41), Ok(()));
        assert_eq!(
            check_at(1, &own, |b| {
                b.stats.hp = 61;
                b.stats.special = 19;
            }),
            Err(BotargaErrorCode::StatOutOfRange)
        );
        for style in ["aggressive", "defensive", "showman"] {
            assert_eq!(check_at(1, &own, |b| b.style_id = style.to_owned()), Ok(()));
        }
        assert_eq!(
            check_at(1, &own, |b| b.style_id = "berserk".to_owned()),
            Err(BotargaErrorCode::StyleUnknown)
        );
    }

    #[test]
    fn only_what_the_user_owns_and_the_catalog_has() {
        let own = starter_unlocks();
        // Of the catalog, not owned.
        assert_eq!(
            check_at(1, &own, |b| b.abilities = ids(&["spicy_slap"])),
            Err(BotargaErrorCode::NotUnlocked)
        );
        // Not of the catalog: the same code.
        assert_eq!(
            check_at(1, &own, |b| b.abilities = ids(&["nuke"])),
            Err(BotargaErrorCode::NotUnlocked)
        );
        assert_eq!(
            check_at(1, &own, |b| b.accessories = ids(&["propeller_beanie"])),
            Err(BotargaErrorCode::NotUnlocked)
        );
        assert_eq!(
            check_at(1, &own, |b| b.cosmetics.idle = Some(IdleClip::Idle2)),
            Err(BotargaErrorCode::NotUnlocked)
        );
        assert_eq!(
            check_at(1, &own, |b| b.cosmetics.victory = Some(VictoryClip::Win1)),
            Err(BotargaErrorCode::NotUnlocked)
        );
        // Owned, but the catalog no longer has it.
        let ghost = BotargaUnlocks {
            abilities: ids(&["foam_headbutt", "retired_move"]),
            accessories: ids(&["retired_hat"]),
            cosmetics: Vec::new(),
        };
        assert_eq!(
            check_at(1, &ghost, |b| b.abilities = ids(&["retired_move"])),
            Err(BotargaErrorCode::NotUnlocked)
        );
        assert_eq!(
            check_at(1, &ghost, |b| b.accessories = ids(&["retired_hat"])),
            Err(BotargaErrorCode::NotUnlocked)
        );
        let all = everything();
        assert_eq!(
            check_at(1, &all, |b| {
                b.cosmetics = FighterCosmetics {
                    idle: Some(IdleClip::Idle2),
                    victory: Some(VictoryClip::Win3),
                };
            }),
            Ok(())
        );
        assert_eq!(
            check_at(1, &all, |b| b.cosmetics.victory = Some(VictoryClip::Win1)),
            Err(BotargaErrorCode::NotUnlocked)
        );
    }

    #[test]
    fn the_rules_of_the_loadout_answer_with_their_code() {
        let all = everything();
        assert_eq!(
            check_at(1, &all, |b| b.abilities =
                ids(&["foam_headbutt", "spicy_slap"])),
            Err(BotargaErrorCode::SlotsLocked)
        );
        assert_eq!(
            check_at(5, &all, |b| b.abilities =
                ids(&["foam_headbutt", "spicy_slap"])),
            Ok(())
        );
        assert_eq!(
            check_at(1, &all, |b| b.abilities.clear()),
            Err(BotargaErrorCode::AbilityCount)
        );
        // Three specials, with the three slots of level 15.
        assert_eq!(
            check_at(15, &all, |b| {
                b.abilities = ids(&["confetti_cannon", "joy_buzzer_hug", "dramatic_pause"]);
            }),
            Err(BotargaErrorCode::AbilityKindCount)
        );
        assert_eq!(
            check_at(15, &all, |b| {
                b.abilities = ids(&["foam_headbutt", "spicy_slap", "wet_noodle_whip"]);
            }),
            Err(BotargaErrorCode::AbilityKindCount)
        );
        // Fire, water and electric bring three weaknesses.
        assert_eq!(
            check_at(15, &all, |b| {
                b.abilities = ids(&["spicy_slap", "puddle_punt", "joy_buzzer_hug"]);
            }),
            Err(BotargaErrorCode::WeaknessCount)
        );
        // Three pieces that add 26 of speed between them.
        assert_eq!(
            check_at(1, &all, |b| {
                b.accessories = ids(&[
                    "propeller_beanie",
                    "spandex_bike_shorts",
                    "grandma_roller_skates",
                ]);
            }),
            Err(BotargaErrorCode::AccessoryStatTotal)
        );
        assert_eq!(
            check_at(5, &all, |b| b.abilities =
                ids(&["foam_headbutt", "foam_headbutt"])),
            Err(BotargaErrorCode::AbilityDuplicate)
        );
        // Two head pieces.
        assert_eq!(
            check_at(1, &all, |b| b.accessories =
                ids(&["propeller_beanie", "tinfoil_crown"])),
            Err(BotargaErrorCode::AccessorySlotDuplicate)
        );
        // +8 speed and -8 defense: 32 of defense is still in range, 27 would not be.
        assert_eq!(
            check_at(1, &all, |b| b.accessories = ids(&["propeller_beanie"])),
            Ok(())
        );
        assert_eq!(
            check_at(1, &all, |b| {
                b.accessories = ids(&["propeller_beanie"]);
                b.stats.defense = 27;
            }),
            Err(BotargaErrorCode::StatOutOfRange)
        );
    }

    #[test]
    fn the_sizes_of_a_body() {
        let catalog = BotargaCatalog::embedded().unwrap();
        let ok = |change: &dyn Fn(&mut BotargaBuild)| {
            let mut build = starter();
            change(&mut build);
            well_formed(&catalog, &build)
        };
        assert!(ok(&|_| {}));
        assert!(ok(&|b| b.abilities.clear()));
        assert!(ok(&|b| b.abilities = vec!["a".repeat(64); 4]));
        assert!(!ok(&|b| b.abilities = vec!["a".repeat(65)]));
        assert!(!ok(&|b| b.abilities = vec!["a".to_owned(); 5]));
        assert!(ok(&|b| b.accessories = vec!["a".to_owned(); 5]));
        assert!(!ok(&|b| b.accessories = vec!["a".to_owned(); 6]));
        assert!(!ok(&|b| b.accessories = vec![String::new()]));
        assert!(!ok(&|b| b.style_id = "s".repeat(65)));
        assert!(!ok(&|b| b.style_id = String::new()));
        // Bytes, not characters.
        assert!(!ok(&|b| b.style_id = "ñ".repeat(33)));
    }
}
