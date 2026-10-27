//! 003 RF17: the balance criteria over the real roster of `content/characters/`, with the
//! default fights and root seed of `just sim`. A content change that unbalances the roster
//! fails here, in `just check`, with the whole text report as the message.
use std::path::PathBuf;

use engine::{CharacterSpec, Ruleset};
use sim::args::DEFAULT_FIGHTS;
use sim::{Config, Criteria, DEFAULT_SEED, RULESET_VERSION};

#[test]
fn the_real_roster_passes_rf17() {
    let ruleset = Ruleset::for_version(RULESET_VERSION).unwrap_or_else(|| panic!("ruleset v1"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/characters");
    let roster: Vec<CharacterSpec> = match content::load_roster(&root, &ruleset) {
        Ok(roster) => roster
            .into_iter()
            .map(content::Character::into_spec)
            .collect(),
        Err(error) => panic!(
            "content.{} in {}: {error}",
            error.code(),
            root.join(error.path()).display()
        ),
    };
    let config = Config {
        fights: DEFAULT_FIGHTS,
        seed: DEFAULT_SEED,
    };
    let report = sim::run(&config, &roster, &ruleset).unwrap_or_else(|e| panic!("{e}"));
    let criteria = Criteria::RF17;
    assert!(
        report.failures(&criteria).is_empty(),
        "{}",
        report.render_text(&criteria)
    );
}
