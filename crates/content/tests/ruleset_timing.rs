//! 007 RF-A5: the ruleset timings the sprite guide reads (packages/renderer/src/guide).
//!
//! Lives in `crates/content`, not in the engine: it only reads the public accessors of
//! [`Ruleset`]. The golden is canonical JSON (sorted keys, 2 spaces, final newline).
use engine::Ruleset;
use serde::Serialize;

const FILE: &str = "ruleset-timing.json";

/// One ruleset version. Fields are declared in alphabetical order: that is the key order of
/// the golden, whatever `serde_json` features are enabled (`timing_fields_are_sorted`).
#[derive(Serialize)]
struct Timing {
    knockdown_ms: u16,
    ruleset_version: u16,
    windup_heavy_ms: u32,
    windup_special_ms: u32,
    windup_strike_ms: u32,
}

#[derive(Serialize)]
struct Golden {
    versions: Vec<Timing>,
}

fn timing(r: &Ruleset) -> Timing {
    Timing {
        knockdown_ms: r.knockdown_ms(),
        ruleset_version: r.version(),
        windup_heavy_ms: r.windup_heavy_ms(),
        windup_special_ms: r.windup_special_ms(),
        windup_strike_ms: r.windup_strike_ms(),
    }
}

fn golden() -> Golden {
    Golden {
        versions: (0..=u16::MAX)
            .filter_map(Ruleset::for_version)
            .map(|r| timing(&r))
            .collect(),
    }
}

fn golden_bytes() -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(&golden()).unwrap_or_else(|e| panic!("{e}"));
    bytes.push(b'\n');
    bytes
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(FILE)
}

#[test]
fn ruleset_timing_matches_its_golden() {
    let actual = golden_bytes();
    let path = golden_path();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI never regenerates goldens"
        );
        std::fs::write(&path, &actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if actual != expected {
        let at = actual
            .iter()
            .zip(&expected)
            .position(|(x, y)| x != y)
            .or(Some(actual.len().min(expected.len())));
        panic!(
            "{FILE}: bytes differ (first difference at {at:?}); a ruleset version changed or was \
             added: regenerate with UPDATE_GOLDEN=1 cargo test -p content --test ruleset_timing \
             and run just check again, which revalidates every sprite set (007 RF-A5)"
        );
    }
}

#[test]
fn timing_fields_are_sorted() {
    let r = Ruleset::for_version(1).unwrap_or_else(|| panic!("ruleset v1"));
    let bytes = serde_json::to_vec(&timing(&r)).unwrap_or_else(|e| panic!("{e}"));
    let text = String::from_utf8(bytes).unwrap_or_else(|e| panic!("{e}"));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}"));
    let serde_json::Value::Object(map) = value else {
        panic!("a timing serializes to an object");
    };
    // Keys in the order they appear in the bytes, not in the map's own order.
    let mut keys: Vec<(usize, &str)> = map
        .keys()
        .map(|k| {
            let at = text
                .find(&format!("\"{k}\":"))
                .unwrap_or_else(|| panic!("{k} in {text}"));
            (at, k.as_str())
        })
        .collect();
    keys.sort_unstable();
    let in_bytes: Vec<&str> = keys.iter().map(|(_, k)| *k).collect();
    let mut sorted = in_bytes.clone();
    sorted.sort_unstable();
    assert_eq!(
        in_bytes, sorted,
        "declare Timing fields in alphabetical order"
    );
}

#[test]
fn every_listed_version_exists() {
    let golden = golden();
    assert!(!golden.versions.is_empty(), "the golden lists no version");
    assert!(
        golden.versions.iter().any(|t| t.ruleset_version == 1),
        "ruleset v1 is missing from the golden"
    );
    for t in &golden.versions {
        assert!(Ruleset::for_version(t.ruleset_version).is_some());
    }
}
