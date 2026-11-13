//! The player botarga (feature 021, phase 1; 016 phase 5a): one per user, created with the user,
//! and the build the user arms with what they own.
//!
//! The catalog of `content/catalog` is embedded in the binary, like the alias lists of
//! `content/names`, and checked once at startup against the player ruleset (v2): a server never
//! runs with a catalog that does not validate. Everything a client sends is an id of that catalog;
//! [`build::check`] is the only way a build reaches the table.

pub mod api;
pub mod build;
pub mod levels;
pub mod store;

use content::{Catalog, CatalogSource, Style, parse_catalog};
use engine::loadout::{self, Accessory, LEVEL_MAX, Loadout, tier};
use engine::{Ability, Ruleset, Stats};

/// The ruleset of the player fights (016): the tournament stays in version 1.
pub const RULESET_VERSION: u16 = 2;

/// Longest id of a build, in bytes (021 "Build").
pub const MAX_ID_BYTES: usize = 64;

/// Most accessories of a build: one per slot.
pub const MAX_ACCESSORIES: usize = 5;

/// The slug of the spec a build is checked with. The fights of a later phase pick their own.
const SPEC_SLUG: &str = "botarga";

/// The style of a new botarga.
const STARTER_STYLE: &str = "balanced";

/// Each stat of a new botarga: the budget of level 1 in five equal parts.
const STARTER_STAT: u8 = 40;

/// Why the embedded catalog cannot be used, by code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the botarga catalog is not valid")]
pub struct CatalogError {
    pub kind: &'static str,
}

/// What a new botarga starts with (021 "Alta").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Starter {
    pub stats: Stats,
    pub style_id: String,
    pub ability_id: String,
}

/// The catalog, the ruleset it was checked with and the starter build.
#[derive(Debug, Clone)]
pub struct BotargaCatalog {
    catalog: Catalog,
    ruleset: Ruleset,
    starter: Starter,
}

impl BotargaCatalog {
    /// The catalog of `content/catalog` as it was when the binary was built.
    ///
    /// # Errors
    /// [`CatalogError`] with a code of `content`, of the engine or of [`Self::new`].
    pub fn embedded() -> Result<Self, CatalogError> {
        let ruleset = Ruleset::for_version(RULESET_VERSION).ok_or(CatalogError {
            kind: "version_missing",
        })?;
        // In the order of `content::LOCALES`.
        let source = CatalogSource {
            abilities: include_bytes!("../../../../content/catalog/abilities.json"),
            accessories: include_bytes!("../../../../content/catalog/accessories.json"),
            styles: include_bytes!("../../../../content/catalog/styles.json"),
            names: [
                include_bytes!("../../../../content/catalog/names.en.json"),
                include_bytes!("../../../../content/catalog/names.es.json"),
            ],
        };
        let catalog = parse_catalog(&source, &ruleset)
            .map_err(|error| CatalogError { kind: error.code() })?;
        Self::new(catalog, ruleset)
    }

    /// Checks that a new botarga can exist with `catalog`: the starter style, the starter heavy,
    /// and a starter build that resolves at level 1.
    ///
    /// # Errors
    /// `starter_style_missing`, `starter_missing` or `starter_invalid`.
    pub fn new(catalog: Catalog, ruleset: Ruleset) -> Result<Self, CatalogError> {
        let invalid = |kind: &'static str| CatalogError { kind };
        if !catalog.styles.iter().any(|style| style.id == STARTER_STYLE) {
            return Err(invalid("starter_style_missing"));
        }
        let ability = catalog.starter().ok_or(invalid("starter_missing"))?;
        let starter = Starter {
            stats: Stats {
                hp: STARTER_STAT,
                attack: STARTER_STAT,
                defense: STARTER_STAT,
                speed: STARTER_STAT,
                special: STARTER_STAT,
            },
            style_id: STARTER_STYLE.to_owned(),
            ability_id: ability.ability.id.clone(),
        };
        let this = Self {
            catalog,
            ruleset,
            starter,
        };
        let starter = &this.starter;
        this.loadout(
            1,
            starter.stats,
            &starter.style_id,
            std::slice::from_ref(&starter.ability_id),
            &[],
        )
        .and_then(|loadout| loadout::resolve(&loadout, &this.ruleset).ok())
        .ok_or(invalid("starter_invalid"))?;
        Ok(this)
    }

    #[must_use]
    pub const fn starter(&self) -> &Starter {
        &self.starter
    }

    #[must_use]
    pub const fn ruleset(&self) -> &Ruleset {
        &self.ruleset
    }

    // The lists of the catalog are sorted by id.
    #[must_use]
    pub fn ability(&self, id: &str) -> Option<&Ability> {
        let abilities = &self.catalog.abilities;
        abilities
            .binary_search_by(|entry| entry.ability.id.as_str().cmp(id))
            .ok()
            .and_then(|at| abilities.get(at))
            .map(|entry| &entry.ability)
    }

    #[must_use]
    pub fn accessory(&self, id: &str) -> Option<&Accessory> {
        let accessories = &self.catalog.accessories;
        accessories
            .binary_search_by(|entry| entry.accessory.id.as_str().cmp(id))
            .ok()
            .and_then(|at| accessories.get(at))
            .map(|entry| &entry.accessory)
    }

    /// The abilities a draw of `level` may offer: the ones of its tier or a lower one (016). The
    /// caller leaves out what the user owns.
    #[must_use]
    pub fn drawable_abilities(&self, level: u8) -> Vec<&str> {
        let Some(top) = tier(level) else {
            return Vec::new();
        };
        self.catalog
            .abilities
            .iter()
            .filter(|entry| entry.tier <= top)
            .map(|entry| entry.ability.id.as_str())
            .collect()
    }

    #[must_use]
    pub fn style(&self, id: &str) -> Option<&Style> {
        let styles = &self.catalog.styles;
        styles
            .binary_search_by(|style| style.id.as_str().cmp(id))
            .ok()
            .and_then(|at| styles.get(at))
    }

    /// The loadout of those ids, or `None` if the catalog lacks one of them.
    fn loadout(
        &self,
        level: u8,
        stats: Stats,
        style_id: &str,
        abilities: &[String],
        accessories: &[String],
    ) -> Option<Loadout> {
        Some(Loadout {
            slug: SPEC_SLUG.to_owned(),
            level,
            stats,
            personality: self.style(style_id)?.personality,
            abilities: abilities
                .iter()
                .map(|id| self.ability(id).cloned())
                .collect::<Option<_>>()?,
            accessories: accessories
                .iter()
                .map(|id| self.accessory(id).cloned())
                .collect::<Option<_>>()?,
        })
    }
}

/// The XP from `level` to the next one (021 "XP por nivel"): `100 + 20 · level`, and 0 at the last
/// level.
#[must_use]
pub fn xp_to_next(level: u8) -> u32 {
    if level >= LEVEL_MAX {
        0
    } else {
        100 + 20 * u32::from(level)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use engine::loadout::budget;

    use super::*;

    #[test]
    fn the_embedded_catalog_is_valid_and_gives_the_starter_of_the_feature() {
        let catalog = BotargaCatalog::embedded().unwrap();
        let starter = catalog.starter();
        let s = starter.stats;
        let spent: u16 = [s.hp, s.attack, s.defense, s.speed, s.special]
            .iter()
            .map(|&stat| u16::from(stat))
            .sum();
        // 200 points, all of the budget of level 1, in equal parts.
        assert_eq!(Some(spent), budget(1));
        assert_eq!(starter.style_id, "balanced");
        assert_eq!(starter.ability_id, "foam_headbutt");
        assert_eq!(catalog.ruleset().version(), 2);
        assert!(catalog.ability(&starter.ability_id).is_some());
        assert!(catalog.ability("strike").is_none());
        assert!(catalog.accessory("propeller_beanie").is_some());
        assert!(catalog.accessory("foam_headbutt").is_none());
        assert!(catalog.style("showman").is_some());
        assert!(catalog.style("").is_none());
    }

    #[test]
    fn a_catalog_without_the_starter_style_is_refused() {
        let good = BotargaCatalog::embedded().unwrap();
        let mut catalog = good.catalog.clone();
        catalog.styles.retain(|style| style.id != STARTER_STYLE);
        assert_eq!(
            BotargaCatalog::new(catalog, good.ruleset.clone()).map(|_| ()),
            Err(CatalogError {
                kind: "starter_style_missing"
            })
        );
        let mut catalog = good.catalog.clone();
        catalog.abilities.retain(|ability| !ability.starter);
        assert_eq!(
            BotargaCatalog::new(catalog, good.ruleset.clone()).map(|_| ()),
            Err(CatalogError {
                kind: "starter_missing"
            })
        );
    }

    #[test]
    fn xp_to_next_follows_the_formula() {
        assert_eq!(xp_to_next(1), 120);
        assert_eq!(xp_to_next(50), 1100);
        assert_eq!(xp_to_next(99), 2080);
        assert_eq!(xp_to_next(100), 0);
    }
}
