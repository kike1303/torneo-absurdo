//! 003 RF15: the `sim` binary over the rosters of `tests/fixtures/` (exit codes, `--json`,
//! reproducibility). `balanced` is two characters with the same spec and different slugs:
//! every seed is played in both orientations and the slug does not enter the simulation, so
//! each wins exactly 50 % whatever the ruleset values. If a ruleset adjustment (plan-b step 8)
//! breaks its mirror bias or a moment kind at 400 fights, change `--seed` here and record it
//! as a deviation.
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    path.to_string_lossy().into_owned()
}

fn sim(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sim"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("run sim: {e}"))
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("sim ended by a signal"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|e| panic!("not UTF-8: {e}"))
}

fn json(args: &[&str]) -> Value {
    let output = sim(args);
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("not JSON: {e}"))
}

fn without_elapsed(mut value: Value) -> Value {
    let object = value
        .as_object_mut()
        .unwrap_or_else(|| panic!("JSON output is an object"));
    assert!(object.remove("elapsed_ms").is_some_and(|v| v.is_u64()));
    value
}

#[test]
fn a_balanced_roster_passes_the_check() {
    let output = sim(&[
        "--content",
        &fixture("balanced"),
        "--fights",
        "400",
        "--check",
    ]);
    let stdout = text(&output.stdout);
    assert_eq!(code(&output), 0, "{stdout}{}", text(&output.stderr));
    // The text report is in English and ends with the time of the RNF.
    for slug in ["twin-a", "twin-b"] {
        assert!(
            stdout
                .lines()
                .any(|l| l.starts_with(&format!("  {slug} ")) && l.contains(" 50.0%  (")),
            "{slug}\n{stdout}"
        );
    }
    for line in [
        "win rate (cross fights, a draw counts half):",
        "400 cross fights",
        "400 mirror fights",
        "RF17: all criteria pass",
        "elapsed: ",
    ] {
        assert!(stdout.contains(line), "{line}\n{stdout}");
    }
}

#[test]
fn a_lopsided_roster_fails_the_check_only_with_check() {
    let args = ["--content", &fixture("lopsided"), "--fights", "200"];
    let without = sim(&args);
    assert_eq!(code(&without), 0, "{}", text(&without.stderr));
    let with = sim(&[&args[..], &["--check"]].concat());
    assert_eq!(code(&with), 1);
    let stdout = text(&with.stdout);
    assert!(stdout.contains("win_rate: strong-one"), "{stdout}");
    assert!(stdout.contains("win_rate: weak-one"), "{stdout}");
}

#[test]
fn usage_errors_exit_with_2_and_their_code() {
    for (args, error) in [
        (vec!["--bogus"], "usage.unknown_flag"),
        (vec!["--fights", "0"], "usage.invalid_fights"),
        (vec!["--seed", "abc"], "usage.invalid_seed"),
        (vec!["--content"], "usage.missing_value"),
    ] {
        let output = sim(&args);
        assert_eq!(code(&output), 2, "{args:?}");
        let stderr = text(&output.stderr);
        assert!(stderr.contains(error), "{args:?}: {stderr}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn content_and_simulation_errors_exit_with_2_and_their_code() {
    let invalid = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../content/tests/fixtures/budget-exceeded")
        .to_string_lossy()
        .into_owned();
    for (args, error) in [
        (vec!["--content", invalid.as_str()], "content.spec"),
        (vec!["--content", "does/not/exist"], "content.io"),
    ] {
        let output = sim(&args);
        assert_eq!(code(&output), 2, "{args:?}");
        let stderr = text(&output.stderr);
        assert!(stderr.contains(error), "{args:?}: {stderr}");
    }
    let single = fixture("single");
    let output = sim(&["--content", &single]);
    assert_eq!(code(&output), 2);
    assert!(text(&output.stderr).contains("sim.roster_too_small"));
    let output = sim(&["--content", &fixture("balanced"), "--fights", "1"]);
    assert_eq!(code(&output), 2);
    assert!(text(&output.stderr).contains("sim.too_few_fights"));
}

#[test]
fn help_exits_with_0_and_warns_about_real_seeds() {
    let output = sim(&["--help"]);
    assert_eq!(code(&output), 0);
    assert!(text(&output.stdout).contains("never pass the seed of a real fight"));
}

#[test]
fn json_is_valid_and_reproducible() {
    let balanced = fixture("balanced");
    let args = ["--content", balanced.as_str(), "--fights", "200", "--json"];
    let first = json(&args);
    for field in [
        "seed_hex",
        "ruleset_version",
        "characters",
        "cross_fights",
        "mirror_fights",
        "per_character",
        "matchups",
        "mirrors",
        "endings",
        "time_up",
        "duration",
        "moments",
        "wins_after_deficit",
        "knockdown",
        "density",
    ] {
        assert!(first["report"].get(field).is_some(), "{field}");
    }
    assert!(first["failures"].is_array() && first["warnings"].is_array());
    assert_eq!(
        first["report"]["seed_hex"],
        "746f726e656f2d6162737572646f2d73696d2d64656661756c742d7631212121"
    );
    assert_eq!(first["report"]["cross_fights"], 200);

    let second = json(&args);
    assert_eq!(without_elapsed(first.clone()), without_elapsed(second));

    let other_seed = "ab".repeat(32);
    let other = json(&[&args[..], &["--seed", &other_seed]].concat());
    assert_eq!(other["report"]["seed_hex"], other_seed.as_str());
    assert_ne!(
        without_elapsed(other)["report"]["duration"],
        without_elapsed(first)["report"]["duration"]
    );
}
