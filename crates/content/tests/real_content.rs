//! 003 RF13: the real roster of `content/characters/` loads and validates against ruleset v1.
//!
//! A content change that breaks a character fails here, in `just check`, with the code and the
//! path of the problem.
use std::path::PathBuf;

use content::load_roster;
use engine::Ruleset;

/// RF13: the roster of engine v0 has from 2 to 4 characters.
const ROSTER_SIZE: std::ops::RangeInclusive<usize> = 2..=4;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/characters")
}

#[test]
fn the_real_roster_loads_and_validates() {
    let ruleset = Ruleset::for_version(1).unwrap_or_else(|| panic!("ruleset v1"));
    let root = root();
    let roster = match load_roster(&root, &ruleset) {
        Ok(roster) => roster,
        Err(error) => panic!(
            "content.{} in {}: {error}",
            error.code(),
            root.join(error.path()).display()
        ),
    };
    assert!(
        ROSTER_SIZE.contains(&roster.len()),
        "{} character(s) in {}, expected {ROSTER_SIZE:?}",
        roster.len(),
        root.display()
    );
}
