//! 010 RF6: a ruleset version is tied to its behavior. The table is empty on purpose: RF10 of 003
//! still lets version 1 change. Its first entry is a prerequisite of the first production fight,
//! and changing or removing an entry blocks the review (`REVIEW.md`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use engine::{CharacterSpec, Ruleset, Seed, canonical_json, canonical_spec_json, simulate};
use sha2::{Digest, Sha256};

/// `(version, sha256)`.
const FROZEN: &[(u16, &str)] = &[];

const SPECS: [&str; 4] = [
    include_str!("../../../crates/engine/tests/golden/spec-striker.json"),
    include_str!("../../../crates/engine/tests/golden/spec-tank.json"),
    include_str!("../../../crates/engine/tests/golden/spec-trickster.json"),
    include_str!("../../../crates/engine/tests/golden/spec-turtle.json"),
];

/// The hash of everything a version fixes: the `Debug` of its ruleset, the bytes of the specs, and
/// the bytes of the logs of a fixed set of fights. `None` if the version does not exist.
fn fingerprint(version: u16) -> Option<String> {
    let ruleset = Ruleset::for_version(version)?;
    let mut hasher = Sha256::new();
    hasher.update(format!("{ruleset:?}").as_bytes());
    let specs: Vec<CharacterSpec> = SPECS
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    for spec in &specs {
        hasher.update(canonical_spec_json(spec).unwrap());
    }
    for (i, a) in specs.iter().enumerate() {
        for (j, b) in specs.iter().enumerate() {
            for seed in 0..4_u8 {
                let log = simulate(
                    &Seed::from_bytes([seed.wrapping_mul(37).wrapping_add(1); 32]),
                    a,
                    b,
                    &ruleset,
                )
                .unwrap_or_else(|e| panic!("{i} vs {j}: {e}"));
                hasher.update(canonical_json(&log).unwrap());
            }
        }
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn check(table: &[(u16, &str)], fingerprint: impl Fn(u16) -> Option<String>) -> Result<(), String> {
    for (version, expected) in table {
        match fingerprint(*version) {
            Some(actual) if actual == *expected => {}
            Some(actual) => return Err(format!("version {version} changed: {actual}")),
            None => return Err(format!("version {version} is gone")),
        }
    }
    Ok(())
}

#[test]
fn every_frozen_version_keeps_its_hash() {
    check(FROZEN, fingerprint).unwrap();
}

#[test]
fn the_mechanism_detects_a_change_and_a_missing_version() {
    let actual = fingerprint(1).unwrap();
    assert_eq!(actual, fingerprint(1).unwrap());
    assert_eq!(actual.len(), 64);
    check(&[(1, actual.as_str())], fingerprint).unwrap();
    assert!(check(&[(1, "0000")], fingerprint).is_err());
    assert_eq!(fingerprint(9999), None);
    assert!(check(&[(9999, "0000")], fingerprint).is_err());
}
