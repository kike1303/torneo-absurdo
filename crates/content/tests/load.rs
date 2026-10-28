//! 003 RF12: `load_roster` over the fixtures of `tests/fixtures/<case>/`, plus the cases that
//! need a symlink or a large file, built in `CARGO_TARGET_TMPDIR` at test time.
use std::fs;
use std::path::{Path, PathBuf};

use content::{ContentError, LOCALES, MAX_FILE_BYTES, load_roster};
use engine::{
    Ability, CharacterSpec, Element, MoveKind, Personality, Ruleset, SpecError, Stats,
    canonical_spec_json,
};

fn v1() -> Ruleset {
    Ruleset::for_version(1).unwrap_or_else(|| panic!("ruleset v1"))
}

fn fixture(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(case)
}

fn load(case: &str) -> Result<Vec<content::Character>, ContentError> {
    load_roster(&fixture(case), &v1())
}

fn error(case: &str) -> ContentError {
    match load(case) {
        Ok(roster) => panic!("{case}: loaded {} character(s)", roster.len()),
        Err(error) => error,
    }
}

fn p(path: &str) -> PathBuf {
    PathBuf::from(path)
}

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("content-load")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("clean {}: {e}", dir.display()));
    }
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

/// Copies the one character of `tests/fixtures/<case>/solo-one` into `root/solo-one`.
fn copy_solo(case: &str, root: &Path) -> PathBuf {
    let from = fixture(case).join("solo-one");
    let to = root.join("solo-one");
    fs::create_dir_all(&to).unwrap_or_else(|e| panic!("create {}: {e}", to.display()));
    let entries = fs::read_dir(&from).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("{}: {e}", from.display()));
        fs::copy(entry.path(), to.join(entry.file_name()))
            .unwrap_or_else(|e| panic!("copy {}: {e}", entry.path().display()));
    }
    to
}

fn stats(hp: u8, attack: u8, defense: u8, speed: u8, special: u8) -> Stats {
    Stats {
        hp,
        attack,
        defense,
        speed,
        special,
    }
}

fn base_spec(slug: &str) -> CharacterSpec {
    CharacterSpec {
        slug: slug.to_owned(),
        stats: stats(60, 60, 60, 60, 60),
        personality: Personality {
            aggression: 50,
            caution: 30,
            showmanship: 40,
        },
        abilities: vec![
            Ability {
                id: "big_swing".to_owned(),
                kind: MoveKind::Heavy,
                power: 40,
                cooldown: 2,
                element: None,
            },
            Ability {
                id: "flare".to_owned(),
                kind: MoveKind::Special,
                power: 70,
                cooldown: 4,
                element: Some(Element::Fire),
            },
        ],
        weaknesses: vec![Element::Water],
    }
}

#[test]
fn valid_roster_is_sorted_by_slug_and_ignores_files_and_extra_dirs() {
    let roster = load("valid").expect("valid roster");
    let slugs: Vec<&str> = roster.iter().map(|c| c.spec().slug.as_str()).collect();
    assert_eq!(slugs, ["alpha-ox", "mid-cat", "zeta-fox"]);

    let mut alpha = base_spec("alpha-ox");
    alpha.stats = stats(80, 70, 70, 30, 50);
    alpha.weaknesses = vec![];
    let mut cat = base_spec("mid-cat");
    cat.abilities = vec![Ability {
        id: "pounce".to_owned(),
        kind: MoveKind::Heavy,
        power: 30,
        cooldown: 0,
        element: None,
    }];
    let fox = base_spec("zeta-fox");
    let specs: Vec<&CharacterSpec> = roster.iter().map(content::Character::spec).collect();
    assert_eq!(specs, [&alpha, &cat, &fox]);

    let [ox, cat, fox] = roster.as_slice() else {
        panic!("three characters");
    };
    assert_eq!(ox.archetype(), "stubborn-ox");
    assert_eq!(ox.rivals(), ["zeta-fox", "mid-cat"]);
    assert_eq!(cat.archetype(), "test-archetype");
    assert!(cat.rivals().is_empty());
    assert_eq!(fox.rivals(), ["alpha-ox"]);
    assert_eq!(fox.clone().into_spec(), base_spec("zeta-fox"));
}

#[test]
fn the_canonical_spec_is_the_engine_shape() {
    let roster = load("valid").expect("valid roster");
    for character in &roster {
        let json = canonical_spec_json(character.spec()).expect("encode");
        let text = String::from_utf8(json.clone()).expect("utf8");
        for field in ["archetype", "ruleset_version", "rivals"] {
            assert!(!text.contains(field), "{field} in {text}");
        }
        let expected = canonical_spec_json(&character.clone().into_spec()).expect("encode");
        assert_eq!(json, expected);
    }
    let fox = roster.last().expect("fox");
    assert_eq!(
        canonical_spec_json(fox.spec()).expect("encode"),
        canonical_spec_json(&base_spec("zeta-fox")).expect("encode")
    );
}

#[test]
fn an_empty_root_is_an_empty_roster() {
    assert_eq!(load("empty").expect("empty"), vec![]);
}

#[test]
fn a_missing_root_is_an_io_error_with_its_kind_and_the_root() {
    let error = error("does-not-exist");
    assert_eq!(
        error,
        ContentError::Io {
            path: fixture("does-not-exist"),
            kind: std::io::ErrorKind::NotFound,
        }
    );
    // The display names the root, not an empty path (QA of phase B).
    assert!(
        error
            .to_string()
            .ends_with(&format!("at {}", fixture("does-not-exist").display())),
        "{error}"
    );
}

#[test]
fn unknown_or_mistyped_fields_are_invalid_shape_with_a_position() {
    for case in ["unknown-field-top", "unknown-field-nested", "wrong-type"] {
        match error(case) {
            ContentError::InvalidShape { path, line, column } => {
                assert_eq!(path, p("solo-one/character.json"), "{case}");
                assert!(line > 1 && column > 0, "{case}: {line}:{column}");
            }
            other => panic!("{case}: {other:?}"),
        }
    }
}

#[test]
fn broken_json_is_invalid_json_with_a_position() {
    match error("broken-json") {
        ContentError::InvalidJson { path, line, column } => {
            assert_eq!(path, p("solo-one/character.json"));
            assert!(line >= 3, "{line}:{column}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn no_error_shows_the_text_of_serde_json() {
    for case in [
        "unknown-field-top",
        "unknown-field-nested",
        "wrong-type",
        "broken-json",
    ] {
        let shown = error(case).to_string();
        for serde_text in [
            "expected",
            "unknown field",
            "invalid type",
            "EOF",
            "while parsing",
            "luck",
            "extra",
        ] {
            assert!(!shown.contains(serde_text), "{case}: {shown}");
        }
    }
}

#[test]
fn slug_must_match_the_directory() {
    assert_eq!(
        error("slug-mismatch"),
        ContentError::SlugMismatch {
            path: p("solo-one/character.json"),
        }
    );
}

#[test]
fn archetype_uses_the_slug_pattern_of_the_engine() {
    assert_eq!(
        error("archetype-invalid"),
        ContentError::ArchetypeInvalid {
            path: p("solo-one/character.json"),
            slug: "solo-one".to_owned(),
        }
    );
}

#[test]
fn ruleset_version_must_be_the_loading_version() {
    assert_eq!(
        error("ruleset-version"),
        ContentError::RulesetVersionMismatch {
            path: p("solo-one/character.json"),
            slug: "solo-one".to_owned(),
            found: 2,
            expected: 1,
        }
    );
}

#[test]
fn every_locale_needs_its_lore() {
    assert_eq!(LOCALES, ["en", "es"]);
    for locale in LOCALES {
        assert_eq!(
            error(&format!("missing-lore-{locale}")),
            ContentError::MissingLore {
                path: p(&format!("solo-one/lore.{locale}.md")),
                slug: "solo-one".to_owned(),
                locale,
            }
        );
    }
}

#[test]
fn the_engine_validates_the_spec() {
    assert_eq!(
        error("budget-exceeded"),
        ContentError::Spec {
            path: p("solo-one/character.json"),
            slug: "solo-one".to_owned(),
            source: SpecError::BudgetExceeded,
        }
    );
}

#[test]
fn a_character_dir_needs_character_json() {
    assert_eq!(
        error("missing-character-file"),
        ContentError::MissingCharacterFile {
            path: p("solo-one/character.json"),
        }
    );
}

#[test]
fn rivals_must_be_other_known_characters_listed_once() {
    let path = p("solo-one/character.json");
    let slug = "solo-one".to_owned();
    assert_eq!(
        error("rival-unknown"),
        ContentError::RivalUnknown {
            path: path.clone(),
            slug: slug.clone(),
            rival: "nobody-here".to_owned(),
        }
    );
    assert_eq!(
        error("rival-self"),
        ContentError::RivalSelf {
            path: path.clone(),
            slug: slug.clone(),
        }
    );
    assert_eq!(
        error("rival-duplicate"),
        ContentError::RivalDuplicate {
            path,
            slug,
            rival: "solo-two".to_owned(),
        }
    );
}

#[test]
fn character_json_is_read_with_a_64_kib_cap() {
    assert_eq!(MAX_FILE_BYTES, 65_536);
    let root = scratch("file-size");
    let dir = copy_solo("valid-solo-base", &root);
    let file = dir.join("character.json");
    let json = fs::read_to_string(&file).expect("read");
    let cap = usize::try_from(MAX_FILE_BYTES).expect("fits");

    // Exactly the cap (trailing whitespace is valid JSON): it loads.
    let padded = format!("{json}{}", " ".repeat(cap - json.len()));
    assert_eq!(padded.len(), cap);
    fs::write(&file, &padded).expect("write");
    assert_eq!(load_roster(&root, &v1()).expect("at the cap").len(), 1);

    // One byte more, and 65 KiB: too large, before parsing.
    for size in [cap + 1, 65 * 1024] {
        fs::write(&file, format!("{json}{}", " ".repeat(size - json.len()))).expect("write");
        assert_eq!(
            load_roster(&root, &v1()),
            Err(ContentError::FileTooLarge {
                path: p("solo-one/character.json"),
            }),
            "{size}"
        );
    }
}

#[cfg(unix)]
mod links {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn a_symlinked_character_dir_is_unexpected() {
        let root = scratch("link-dir");
        let real = scratch("link-dir-target");
        copy_solo("valid-solo-base", &real);
        symlink(real.join("solo-one"), root.join("solo-one")).expect("symlink");
        assert_eq!(
            load_roster(&root, &v1()),
            Err(ContentError::UnexpectedEntry {
                path: p("solo-one")
            })
        );
    }

    #[test]
    fn a_symlinked_file_at_the_root_is_unexpected() {
        let root = scratch("link-root-file");
        copy_solo("valid-solo-base", &root);
        symlink(root.join("solo-one/lore.en.md"), root.join("README.md")).expect("symlink");
        assert_eq!(
            load_roster(&root, &v1()),
            Err(ContentError::UnexpectedEntry {
                path: p("README.md")
            })
        );
    }

    #[test]
    fn a_symlinked_character_json_is_unexpected() {
        let root = scratch("link-json");
        let dir = copy_solo("valid-solo-base", &root);
        let outside = scratch("link-json-target").join("character.json");
        fs::rename(dir.join("character.json"), &outside).expect("move");
        symlink(&outside, dir.join("character.json")).expect("symlink");
        assert_eq!(
            load_roster(&root, &v1()),
            Err(ContentError::UnexpectedEntry {
                path: p("solo-one/character.json"),
            })
        );
    }

    #[test]
    fn a_symlinked_lore_is_unexpected() {
        let root = scratch("link-lore");
        let dir = copy_solo("valid-solo-base", &root);
        fs::remove_file(dir.join("lore.es.md")).expect("remove");
        symlink(dir.join("lore.en.md"), dir.join("lore.es.md")).expect("symlink");
        assert_eq!(
            load_roster(&root, &v1()),
            Err(ContentError::UnexpectedEntry {
                path: p("solo-one/lore.es.md"),
            })
        );
    }
}

#[test]
fn a_directory_named_character_json_is_unexpected() {
    let root = scratch("json-dir");
    let dir = copy_solo("valid-solo-base", &root);
    fs::remove_file(dir.join("character.json")).expect("remove");
    fs::create_dir(dir.join("character.json")).expect("mkdir");
    assert_eq!(
        load_roster(&root, &v1()),
        Err(ContentError::UnexpectedEntry {
            path: p("solo-one/character.json"),
        })
    );
}

/// macOS (APFS) rejects names that are not UTF-8, so this only runs on Linux.
#[cfg(target_os = "linux")]
#[test]
fn a_name_that_is_not_utf8_is_rejected() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let root = scratch("not-utf8");
    let name = OsStr::from_bytes(b"bad-\xff");
    fs::create_dir(root.join(name)).expect("mkdir");
    assert_eq!(
        load_roster(&root, &v1()),
        Err(ContentError::NameNotUtf8 {
            path: PathBuf::from(name),
        })
    );
}

#[test]
fn errors_are_reported_in_name_order() {
    // Two broken characters: the first by name is the one reported, on every file system.
    let root = scratch("order");
    copy_solo("budget-exceeded", &root);
    fs::rename(root.join("solo-one"), root.join("b-second")).expect("rename");
    copy_solo("missing-character-file", &root);
    fs::rename(root.join("solo-one"), root.join("a-first")).expect("rename");
    assert_eq!(
        load_roster(&root, &v1()),
        Err(ContentError::MissingCharacterFile {
            path: p("a-first/character.json"),
        })
    );
}
