//! 016: `load_catalog` over the real `content/catalog/`, and every rule over copies of it changed
//! in `CARGO_TARGET_TMPDIR` at test time.
use std::fs;
use std::path::{Path, PathBuf};

use content::{Catalog, ContentError, LOCALES, NAME_MAX_CHARS, load_catalog};
use engine::loadout::{LEVEL_MAX, Slot, TIERS, tier};
use engine::{MoveKind, Ruleset};
use serde_json::{Value, json};

/// Draws of 1 out of 3 up to `LEVEL_MAX`: every 5 levels except the ones that open a slot.
const DRAWS: usize = 17;

fn v2() -> Ruleset {
    Ruleset::for_version(2).unwrap_or_else(|| panic!("ruleset v2"))
}

fn real() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/catalog")
}

fn load_real() -> Catalog {
    let root = real();
    match load_catalog(&root, &v2()) {
        Ok(catalog) => catalog,
        Err(error) => panic!(
            "content.{} in {}: {error}",
            error.code(),
            root.join(error.path()).display()
        ),
    }
}

#[test]
fn the_real_catalog_loads_and_validates() {
    let catalog = load_real();
    let starter = catalog.starter().unwrap_or_else(|| panic!("no starter"));
    assert_eq!(starter.ability.kind, MoveKind::Heavy);

    // Every draw offers 3 abilities the player does not own: the starter plus the draws plus 2.
    assert!(
        catalog.abilities.len() > DRAWS + 2,
        "{}",
        catalog.abilities.len()
    );
    // Draws only reach the tier of the level, so each tier must cover its own draws: the draws
    // of the levels of tier t (by 5, minus the slot levels) plus 2, over the abilities of
    // tiers <= t.
    let slot_levels = engine::loadout::SLOT_LEVELS;
    for t in 1..=TIERS {
        let draws_up_to_t = (1..=LEVEL_MAX)
            .filter(|l| l % 5 == 0 && !slot_levels.contains(l))
            .filter(|&l| tier(l).is_some_and(|lt| lt <= t))
            .count();
        let available = catalog
            .abilities
            .iter()
            .filter(|a| a.tier <= t && !a.starter)
            .count();
        assert!(
            available >= draws_up_to_t + 2,
            "tier {t}: {available} abilities for {draws_up_to_t} draws"
        );
        for kind in [MoveKind::Heavy, MoveKind::Special] {
            assert!(
                catalog
                    .abilities
                    .iter()
                    .any(|a| a.tier == t && a.ability.kind == kind),
                "tier {t} without {kind:?}"
            );
        }
    }
    for slot in [Slot::Head, Slot::Hands, Slot::Torso, Slot::Legs, Slot::Feet] {
        assert!(
            catalog.accessories.iter().any(|a| a.accessory.slot == slot),
            "no accessory for {slot:?}"
        );
    }
    assert_eq!(catalog.styles.len(), 4);
    for locale in LOCALES {
        assert!(catalog.styles.iter().all(|s| s.name(locale).is_some()));
        assert!(catalog.abilities.iter().all(|a| a.name(locale).is_some()));
    }
}

/// The real tournament characters keep their own ids: no catalog id collides with them.
#[test]
fn catalog_ids_do_not_collide_with_the_tournament() {
    let catalog = load_real();
    let roster = content::load_roster(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/characters"),
        &Ruleset::for_version(1).unwrap_or_else(|| panic!("v1")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    for character in &roster {
        for ability in &character.spec().abilities {
            assert!(
                catalog.abilities.iter().all(|a| a.ability.id != ability.id),
                "{}",
                ability.id
            );
        }
    }
}

/// A copy of the real catalog in a fresh directory.
fn copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("content-catalog")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("clean {}: {e}", dir.display()));
    }
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    for entry in fs::read_dir(real()).unwrap_or_else(|e| panic!("{e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("{e}"));
        fs::copy(entry.path(), dir.join(entry.file_name())).unwrap_or_else(|e| panic!("{e}"));
    }
    dir
}

fn edit(root: &Path, file: &str, change: impl FnOnce(&mut Value)) {
    let path = root.join(file);
    let mut value: Value =
        serde_json::from_slice(&fs::read(&path).unwrap_or_else(|e| panic!("{e}")))
            .unwrap_or_else(|e| panic!("{e}"));
    change(&mut value);
    fs::write(
        &path,
        serde_json::to_vec(&value).unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
}

fn rule(error: &ContentError) -> (&str, &str, &str) {
    match error {
        ContentError::Catalog { path, id, rule } => (
            path.to_str().unwrap_or_else(|| panic!("utf8")),
            id.as_str(),
            rule.as_str(),
        ),
        other => panic!("not a catalog error: {other}"),
    }
}

fn failing(name: &str, change: impl FnOnce(&Path)) -> ContentError {
    let root = copy(name);
    change(&root);
    match load_catalog(&root, &v2()) {
        Ok(_) => panic!("{name}: loaded"),
        Err(error) => error,
    }
}

fn first<'a>(value: &'a mut Value, list: &str) -> &'a mut Value {
    value
        .get_mut(list)
        .and_then(|l| l.get_mut(0))
        .unwrap_or_else(|| panic!("{list}[0]"))
}

fn set(value: &mut Value, key: &str, new: Value) {
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_owned(), new);
    }
}

fn first_id(root: &Path, file: &str, list: &str) -> String {
    let value: Value = serde_json::from_slice(&fs::read(root.join(file)).unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}"));
    value[list][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("id"))
        .to_owned()
}

#[test]
fn every_rule_has_its_code() {
    let ability_rule = |name: &str, change: fn(&mut Value)| {
        let error = failing(name, |root| {
            edit(root, "abilities.json", |v| change(first(v, "abilities")));
        });
        rule(&error).2.to_owned()
    };
    assert_eq!(
        ability_rule("power", |a| set(a, "power", json!(99))),
        "spec.power_out_of_range"
    );
    assert_eq!(
        ability_rule("strike", |a| set(a, "id", json!("strike"))),
        "spec.ability_id_invalid"
    );
    assert_eq!(
        ability_rule("tier", |a| set(a, "tier", json!(6))),
        "tier_out_of_range"
    );
    assert_eq!(
        ability_rule("starter-special", |a| {
            set(a, "kind", json!("special"));
            set(a, "power", json!(50));
            set(a, "cooldown", json!(2));
        }),
        "starter_invalid"
    );

    // Two starters, or none.
    let error = failing("two-starters", |root| {
        edit(root, "abilities.json", |v| {
            if let Some(list) = v["abilities"].as_array_mut() {
                for a in list
                    .iter_mut()
                    .filter(|a| a["kind"] == "heavy" && a["tier"] == 1)
                {
                    set(a, "starter", json!(true));
                    set(a, "element", Value::Null);
                }
            }
        });
    });
    assert_eq!(rule(&error).2, "starter_count");

    let error = failing("duplicate", |root| {
        let id = first_id(root, "abilities.json", "abilities");
        edit(root, "abilities.json", |v| {
            if let Some(second) = v["abilities"].as_array_mut().and_then(|l| l.get_mut(1)) {
                set(second, "id", json!(id));
            }
        });
    });
    assert_eq!(rule(&error).2, "duplicate_id");

    let error = failing("accessory-adds", |root| {
        edit(root, "accessories.json", |v| {
            set(first(v, "accessories"), "deltas", json!({"attack": 5}));
        });
    });
    assert_eq!(
        rule(&error),
        (
            "accessories.json",
            rule(&error).1,
            "loadout.accessory_deltas"
        )
    );

    let error = failing("style", |root| {
        edit(root, "styles.json", |v| {
            set(
                first(v, "styles"),
                "personality",
                json!({"aggression": 101, "caution": 0, "showmanship": 0}),
            );
        });
    });
    assert_eq!(rule(&error).2, "personality_out_of_range");

    // Names: missing, unknown, too long, padded.
    let error = failing("name-missing", |root| {
        let id = first_id(root, "abilities.json", "abilities");
        edit(root, "names.en.json", |v| {
            if let Some(map) = v["abilities"].as_object_mut() {
                map.remove(&id);
            }
        });
    });
    assert_eq!(rule(&error).0, "names.en.json");
    assert_eq!(rule(&error).2, "name_missing");

    let error = failing("name-unknown", |root| {
        edit(root, "names.es.json", |v| {
            set(&mut v["styles"], "ghost", json!("Fantasma"));
        });
    });
    assert_eq!(rule(&error), ("names.es.json", "ghost", "name_unknown"));

    for (case, name) in [
        ("name-long", "x".repeat(NAME_MAX_CHARS + 1)),
        ("name-padded", " Padded".to_owned()),
        ("name-empty", String::new()),
    ] {
        let error = failing(case, |root| {
            let id = first_id(root, "styles.json", "styles");
            edit(root, "names.es.json", |v| {
                set(&mut v["styles"], &id, json!(name))
            });
        });
        assert_eq!(rule(&error).2, "name_invalid", "{case}");
    }
}

#[test]
fn files_and_shapes() {
    let error = failing("missing", |root| {
        fs::remove_file(root.join("styles.json")).unwrap_or_else(|e| panic!("{e}"));
    });
    assert_eq!(
        error,
        ContentError::MissingCatalogFile {
            path: PathBuf::from("styles.json")
        }
    );
    let error = failing("extra-field", |root| {
        edit(root, "accessories.json", |v| {
            set(first(v, "accessories"), "price", json!(10));
        });
    });
    assert_eq!(error.code(), "invalid_shape");
    let error = failing("bad-slot", |root| {
        edit(root, "accessories.json", |v| {
            set(first(v, "accessories"), "slot", json!("tail"));
        });
    });
    assert_eq!(error.code(), "invalid_shape");

    // Other files in the directory are ignored.
    let root = copy("extra-file");
    fs::write(root.join("README.md"), "notes").unwrap_or_else(|e| panic!("{e}"));
    assert!(load_catalog(&root, &v2()).is_ok());
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_is_rejected() {
    let error = failing("symlink", |root| {
        let target = root.join("styles.real.json");
        fs::rename(root.join("styles.json"), &target).unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&target, root.join("styles.json"))
            .unwrap_or_else(|e| panic!("{e}"));
    });
    assert_eq!(error.code(), "unexpected_entry");
}
