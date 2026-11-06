//! Loader of `content/catalog` (016): the abilities, accessories and fighting styles a player
//! botarga can equip, with their names in every locale of [`LOCALES`].
//!
//! Layout (only these files are read; anything else in the directory is ignored):
//! - `abilities.json`: `{"abilities": [{id, kind, power, cooldown, element, tier, starter?}]}`;
//! - `accessories.json`: `{"accessories": [{id, slot, tier, deltas}]}`;
//! - `styles.json`: `{"styles": [{id, personality}]}`;
//! - `names.<locale>.json`: `{"abilities": {id: name}, "accessories": {..}, "styles": {..}}`.
//!
//! The rules of an ability and of an accessory are the engine's ([`Ability::validate`] and
//! [`Accessory::validate`]); this module adds the integrity of the catalog: unique ids, tiers in
//! range, one starter, and a name for every entry in every locale.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use engine::loadout::{Accessory, Slot, StatDeltas, TIERS};
use engine::{Ability, Element, MoveKind, Personality, Ruleset, is_valid_slug};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::{ContentError, EntryKind, LOCALES, entry_kind, json_error, read_bounded};

/// Longest name, in characters.
pub const NAME_MAX_CHARS: usize = 48;

/// An ability of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAbility {
    pub ability: Ability,
    /// The first tier in which a draw can offer it (`1..=TIERS`).
    pub tier: u8,
    /// The heavy every new botarga starts with (exactly one in the catalog).
    pub starter: bool,
    names: BTreeMap<&'static str, String>,
}

/// An accessory of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAccessory {
    pub accessory: Accessory,
    pub tier: u8,
    names: BTreeMap<&'static str, String>,
}

/// A fighting style: a personality preset the player picks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub id: String,
    pub personality: Personality,
    names: BTreeMap<&'static str, String>,
}

macro_rules! named {
    ($($t:ty),*) => {$(
        impl $t {
            /// The name in `locale` (one of [`LOCALES`]).
            #[must_use]
            pub fn name(&self, locale: &str) -> Option<&str> {
                self.names.get(locale).map(String::as_str)
            }
        }
    )*};
}
named!(CatalogAbility, CatalogAccessory, Style);

/// The whole player catalog, each list sorted by `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub abilities: Vec<CatalogAbility>,
    pub accessories: Vec<CatalogAccessory>,
    pub styles: Vec<Style>,
}

impl Catalog {
    /// The starter heavy (the loader checks there is exactly one).
    #[must_use]
    pub fn starter(&self) -> Option<&CatalogAbility> {
        self.abilities.iter().find(|a| a.starter)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AbilitiesFile {
    abilities: Vec<AbilityEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AbilityEntry {
    id: String,
    kind: MoveKind,
    power: u8,
    cooldown: u8,
    element: Option<Element>,
    tier: u8,
    #[serde(default)]
    starter: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessoriesFile {
    accessories: Vec<AccessoryEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessoryEntry {
    id: String,
    slot: Slot,
    tier: u8,
    deltas: StatDeltas,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StylesFile {
    styles: Vec<StyleEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleEntry {
    id: String,
    personality: Personality,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamesFile {
    abilities: BTreeMap<String, String>,
    accessories: BTreeMap<String, String>,
    styles: BTreeMap<String, String>,
}

const ABILITIES: &str = "abilities.json";
const ACCESSORIES: &str = "accessories.json";
const STYLES: &str = "styles.json";

/// Loads and checks the player catalog under `root` against `ruleset` (the player one, v2).
///
/// # Errors
/// The first problem found, file by file in the order of the layout above. See [`ContentError`].
pub fn load_catalog(root: &Path, ruleset: &Ruleset) -> Result<Catalog, ContentError> {
    let abilities: AbilitiesFile = read_json(root, ABILITIES)?;
    let accessories: AccessoriesFile = read_json(root, ACCESSORIES)?;
    let styles: StylesFile = read_json(root, STYLES)?;
    let mut names = Vec::new();
    for locale in LOCALES {
        let file = format!("names.{locale}.json");
        let parsed: NamesFile = read_json(root, &file)?;
        names.push((locale, PathBuf::from(file), parsed));
    }

    let mut catalog = Catalog {
        abilities: Vec::new(),
        accessories: Vec::new(),
        styles: Vec::new(),
    };
    let path = PathBuf::from(ABILITIES);
    for entry in abilities.abilities {
        let ability = Ability {
            id: entry.id,
            kind: entry.kind,
            power: entry.power,
            cooldown: entry.cooldown,
            element: entry.element,
        };
        let fail = |rule: String| catalog_error(&path, &ability.id, rule);
        ability
            .validate(ruleset)
            .map_err(|e| fail(format!("spec.{}", e.code())))?;
        check_tier(entry.tier).map_err(fail)?;
        if entry.starter
            && (ability.kind != MoveKind::Heavy || entry.tier != 1 || ability.element.is_some())
        {
            return Err(fail("starter_invalid".to_owned()));
        }
        catalog.abilities.push(CatalogAbility {
            ability,
            tier: entry.tier,
            starter: entry.starter,
            names: BTreeMap::new(),
        });
    }
    catalog
        .abilities
        .sort_by(|x, y| x.ability.id.cmp(&y.ability.id));
    check_unique(&path, catalog.abilities.iter().map(|a| &a.ability.id))?;
    if catalog.abilities.iter().filter(|a| a.starter).count() != 1 {
        return Err(catalog_error(&path, "", "starter_count".to_owned()));
    }

    let path = PathBuf::from(ACCESSORIES);
    for entry in accessories.accessories {
        let accessory = Accessory {
            id: entry.id,
            slot: entry.slot,
            deltas: entry.deltas,
        };
        let fail = |rule: String| catalog_error(&path, &accessory.id, rule);
        accessory
            .validate()
            .map_err(|e| fail(format!("loadout.{}", e.code())))?;
        check_tier(entry.tier).map_err(fail)?;
        catalog.accessories.push(CatalogAccessory {
            accessory,
            tier: entry.tier,
            names: BTreeMap::new(),
        });
    }
    catalog
        .accessories
        .sort_by(|x, y| x.accessory.id.cmp(&y.accessory.id));
    check_unique(&path, catalog.accessories.iter().map(|a| &a.accessory.id))?;

    let path = PathBuf::from(STYLES);
    for entry in styles.styles {
        let fail = |rule: &str| catalog_error(&path, &entry.id, rule.to_owned());
        if !is_valid_slug(&entry.id) {
            return Err(fail("style_id_invalid"));
        }
        let p = entry.personality;
        if [p.aggression, p.caution, p.showmanship]
            .iter()
            .any(|&t| t > ruleset.personality_max())
        {
            return Err(fail("personality_out_of_range"));
        }
        catalog.styles.push(Style {
            id: entry.id,
            personality: entry.personality,
            names: BTreeMap::new(),
        });
    }
    catalog.styles.sort_by(|x, y| x.id.cmp(&y.id));
    check_unique(&path, catalog.styles.iter().map(|s| &s.id))?;

    for (locale, path, file) in names {
        let abilities = catalog
            .abilities
            .iter_mut()
            .map(|a| (&a.ability.id, &mut a.names));
        attach(&path, locale, file.abilities, abilities)?;
        let accessories = catalog
            .accessories
            .iter_mut()
            .map(|a| (&a.accessory.id, &mut a.names));
        attach(&path, locale, file.accessories, accessories)?;
        let styles = catalog.styles.iter_mut().map(|s| (&s.id, &mut s.names));
        attach(&path, locale, file.styles, styles)?;
    }
    Ok(catalog)
}

fn read_json<T: DeserializeOwned>(root: &Path, file: &str) -> Result<T, ContentError> {
    let path = PathBuf::from(file);
    let checked = match entry_kind(root, &path)? {
        EntryKind::File(meta) => meta,
        EntryKind::Missing => return Err(ContentError::MissingCatalogFile { path }),
        EntryKind::Dir | EntryKind::Other => return Err(ContentError::UnexpectedEntry { path }),
    };
    let bytes = read_bounded(root, &path, &checked)?;
    serde_json::from_slice(&bytes).map_err(|e| json_error(&path, &e))
}

fn catalog_error(path: &Path, id: &str, rule: String) -> ContentError {
    ContentError::Catalog {
        path: path.to_path_buf(),
        id: id.to_owned(),
        rule,
    }
}

fn check_tier(tier: u8) -> Result<(), String> {
    if (1..=TIERS).contains(&tier) {
        Ok(())
    } else {
        Err("tier_out_of_range".to_owned())
    }
}

/// `ids` must be sorted.
fn check_unique<'a>(
    path: &Path,
    ids: impl Iterator<Item = &'a String>,
) -> Result<(), ContentError> {
    let mut previous: Option<&String> = None;
    for id in ids {
        if previous == Some(id) {
            return Err(catalog_error(path, id, "duplicate_id".to_owned()));
        }
        previous = Some(id);
    }
    Ok(())
}

/// Every entry gets its name in `locale`, and every name belongs to an entry.
fn attach<'a>(
    path: &Path,
    locale: &'static str,
    mut names: BTreeMap<String, String>,
    entries: impl Iterator<Item = (&'a String, &'a mut BTreeMap<&'static str, String>)>,
) -> Result<(), ContentError> {
    for (id, slot) in entries {
        let Some(name) = names.remove(id) else {
            return Err(catalog_error(path, id, "name_missing".to_owned()));
        };
        let chars = name.chars().count();
        if name.trim() != name || chars == 0 || chars > NAME_MAX_CHARS || name.contains('\n') {
            return Err(catalog_error(path, id, "name_invalid".to_owned()));
        }
        slot.insert(locale, name);
    }
    if let Some(id) = names.keys().next() {
        return Err(catalog_error(path, id, "name_unknown".to_owned()));
    }
    Ok(())
}
