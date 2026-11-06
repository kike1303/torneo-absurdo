//! 016 phase 3: the open builds criteria over the real player catalog of `content/catalog/` at
//! the top level, with the default root seed. Smaller than the command line (`just sim --builds
//! 2000 --fights 50000`) so it fits in `just check`; a catalog change that breaks the balance
//! fails here with the whole text report as the message.
use std::path::PathBuf;

use engine::Ruleset;
use engine::loadout::LEVEL_MAX;
use sim::DEFAULT_SEED;
use sim::builds::{BUILDS_RULESET_VERSION, BuildsConfig, BuildsCriteria, run_builds};

#[test]
fn the_real_catalog_passes_the_builds_criteria() {
    let ruleset =
        Ruleset::for_version(BUILDS_RULESET_VERSION).unwrap_or_else(|| panic!("ruleset v2"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/catalog");
    let catalog = content::load_catalog(&root, &ruleset).unwrap_or_else(|error| {
        panic!(
            "content.{} in {}: {error}",
            error.code(),
            root.join(error.path()).display()
        )
    });
    let config = BuildsConfig {
        builds: 1000,
        fights: 20_000,
        level: LEVEL_MAX,
        seed: DEFAULT_SEED,
        climbs: 20,
        climb_steps: 15,
        gap_fights: 0,
    };
    let report = run_builds(&config, &catalog, &ruleset).unwrap_or_else(|e| panic!("{e}"));
    let criteria = BuildsCriteria::FEATURE_016;
    assert!(
        report.failures(&criteria).is_empty(),
        "{}",
        report.render_text(&criteria)
    );
}
