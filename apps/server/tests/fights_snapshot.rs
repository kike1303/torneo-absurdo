//! 010 RF2: the hash of the snapshot, against a vector written by a program that knows nothing of
//! the server (plain concatenation).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use engine::{CharacterSpec, canonical_spec_json};
use server::fights::hex;
use server::fights::snapshot::snapshot_sha256;

#[test]
fn the_golden_vector() {
    let text = include_str!("golden/fight-snapshot.json");
    let vector: serde_json::Value = serde_json::from_str(text).unwrap();
    let spec_a = vector["spec_a"].as_str().unwrap();
    let spec_b = vector["spec_b"].as_str().unwrap();
    let expected = vector["snapshot_sha256"].as_str().unwrap();
    let version = u16::try_from(vector["ruleset_version"].as_u64().unwrap()).unwrap();

    // The vector carries real canonical bytes.
    for spec in [spec_a, spec_b] {
        let decoded: CharacterSpec = serde_json::from_str(spec).unwrap();
        assert_eq!(canonical_spec_json(&decoded).unwrap(), spec.as_bytes());
    }
    assert_eq!(
        hex(&snapshot_sha256(
            version,
            spec_a.as_bytes(),
            spec_b.as_bytes()
        )),
        expected
    );
    // The order of the sides is part of what is fixed, and so is the version.
    assert_ne!(
        hex(&snapshot_sha256(
            version,
            spec_b.as_bytes(),
            spec_a.as_bytes()
        )),
        expected
    );
    assert_ne!(
        hex(&snapshot_sha256(
            version + 1,
            spec_a.as_bytes(),
            spec_b.as_bytes()
        )),
        expected
    );
}
