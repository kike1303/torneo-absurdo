//! Golden tests (003 RF16), the same files natively and in WASI (RF18, `just _check-wasm`).
//!
//! - (a) fixed `(seed, a, b) -> log` vectors against `golden/log-*.json` (also the fixtures of
//!   002's adapter, decision 6 of plan-a);
//! - (b) an FNV-1a 64 digest of the canonical JSON of 1000 fights against `golden/digest-1000.txt`;
//! - (c) `canonical_spec_json` of every fixture against `golden/spec-*.json`, and its round trip.
//!
//! Every golden file is exactly the canonical bytes plus a final `\n`. Regenerate them with
//! `UPDATE_GOLDEN=1 cargo test -p engine --test golden`, natively only (in WASI the environment
//! is empty and the writing code does not exist); CI never regenerates.
mod common;

use common::{
    FIXTURES, assert_invariants, assert_only_codes, derive_seed, fixture, fixtures, hex, v1,
};
use engine::{
    CharacterSpec, EndReason, FightEvent, FightLog, MomentKind, Seed, Side, canonical_json,
    canonical_spec_json, simulate,
};

type Covers = fn(&FightLog) -> bool;

/// A fixed vector: fixtures `a` and `b`, seed `derive_seed(seed)`, and what it must cover.
struct Vector {
    file: &'static str,
    a: &'static str,
    b: &'static str,
    seed: u32,
    expected: &'static [u8],
    covers: Covers,
}

fn has(log: &FightLog, wanted: fn(&FightEvent) -> bool) -> bool {
    log.events.iter().any(wanted)
}

fn is_ko(log: &FightLog) -> bool {
    matches!(log.outcome(), Some((_, EndReason::Ko)))
        && has(log, |e| matches!(e, FightEvent::LowHp { .. }))
}

fn ko_decisive_crit(log: &FightLog) -> bool {
    is_ko(log)
        && has(log, |e| {
            matches!(
                e,
                FightEvent::Moment {
                    kind: MomentKind::DecisiveCrit,
                    ..
                }
            )
        })
}

fn ko_knockdown(log: &FightLog) -> bool {
    is_ko(log) && has(log, |e| matches!(e, FightEvent::Knockdown { .. }))
}

fn ko_comeback(log: &FightLog) -> bool {
    is_ko(log)
        && has(log, |e| {
            matches!(
                e,
                FightEvent::Moment {
                    kind: MomentKind::Comeback,
                    ..
                }
            )
        })
}

fn ko_clutch(log: &FightLog) -> bool {
    is_ko(log)
        && has(log, |e| {
            matches!(
                e,
                FightEvent::Moment {
                    kind: MomentKind::ClutchKo,
                    ..
                }
            )
        })
}

fn decision_a(log: &FightLog) -> bool {
    log.outcome() == Some((Some(Side::A), EndReason::Decision))
}

fn decision_b(log: &FightLog) -> bool {
    log.outcome() == Some((Some(Side::B), EndReason::Decision))
}

fn draw(log: &FightLog) -> bool {
    log.outcome() == Some((None, EndReason::Draw))
}

/// What each vector must show; the seeds come from `search_golden_seeds`.
const TARGETS: [(&str, Covers); 7] = [
    ("log-ko-decisive-crit.json", ko_decisive_crit),
    ("log-ko-knockdown.json", ko_knockdown),
    ("log-ko-comeback.json", ko_comeback),
    ("log-ko-clutch.json", ko_clutch),
    ("log-decision-a.json", decision_a),
    ("log-decision-b.json", decision_b),
    ("log-draw.json", draw),
];

/// Seeds chosen with `search_golden_seeds`: varied pairs, and no vector is another one with the
/// sides swapped (same seed, mirrored fight). The draw is a turtle mirror.
const VECTORS: [Vector; 7] = [
    Vector {
        file: "log-ko-decisive-crit.json",
        a: "striker",
        b: "trickster",
        seed: 1,
        expected: include_bytes!("golden/log-ko-decisive-crit.json"),
        covers: ko_decisive_crit,
    },
    Vector {
        file: "log-ko-knockdown.json",
        a: "trickster",
        b: "tank",
        seed: 0,
        expected: include_bytes!("golden/log-ko-knockdown.json"),
        covers: ko_knockdown,
    },
    Vector {
        file: "log-ko-comeback.json",
        a: "striker",
        b: "tank",
        seed: 4,
        expected: include_bytes!("golden/log-ko-comeback.json"),
        covers: ko_comeback,
    },
    Vector {
        file: "log-ko-clutch.json",
        a: "trickster",
        b: "trickster",
        seed: 1,
        expected: include_bytes!("golden/log-ko-clutch.json"),
        covers: ko_clutch,
    },
    Vector {
        file: "log-decision-a.json",
        a: "tank",
        b: "turtle",
        seed: 0,
        expected: include_bytes!("golden/log-decision-a.json"),
        covers: decision_a,
    },
    Vector {
        file: "log-decision-b.json",
        a: "turtle",
        b: "tank",
        seed: 1,
        expected: include_bytes!("golden/log-decision-b.json"),
        covers: decision_b,
    },
    Vector {
        file: "log-draw.json",
        a: "turtle",
        b: "turtle",
        seed: 53,
        expected: include_bytes!("golden/log-draw.json"),
        covers: draw,
    },
];

fn simulate_vector(a: &CharacterSpec, b: &CharacterSpec, seed: u32) -> FightLog {
    simulate(&Seed::from_bytes(derive_seed(seed)), a, b, &v1())
        .unwrap_or_else(|e| panic!("simulate: {e}"))
}

fn golden_bytes(canonical: Vec<u8>) -> Vec<u8> {
    let mut bytes = canonical;
    bytes.push(b'\n');
    bytes
}

/// Compares with the golden file or, with `UPDATE_GOLDEN=1` (native only), rewrites it.
fn check_golden(file: &str, actual: &[u8], expected: &[u8]) {
    #[cfg(not(target_family = "wasm"))]
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI never regenerates goldens"
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(file);
        std::fs::write(&path, actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    if actual != expected {
        let at = actual.iter().zip(expected).position(|(x, y)| x != y);
        panic!(
            "{file}: bytes differ (first difference at {at:?}, lengths {} and {}); \
             if the change is intended, regenerate with UPDATE_GOLDEN=1 (RF10)",
            actual.len(),
            expected.len()
        );
    }
}

/// (a) Fixed vectors.
#[test]
fn vectors_match_their_golden_logs() {
    let r = v1();
    for v in &VECTORS {
        let (a, b) = (fixture(v.a), fixture(v.b));
        let log = simulate_vector(&a, &b, v.seed);
        assert!(
            (v.covers)(&log),
            "{}: does not cover what it is for",
            v.file
        );
        assert_invariants(&log, &r);
        assert_only_codes(&log, &a, &b);
        let bytes = golden_bytes(canonical_json(&log).unwrap_or_else(|e| panic!("{e}")));
        // RF20: the log never contains the seed.
        let text = String::from_utf8(bytes.clone()).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !text.contains(&hex(&derive_seed(v.seed))),
            "{}: contains the seed",
            v.file
        );
        check_golden(v.file, &bytes, v.expected);
    }
}

/// Between them, the vectors show every event variant and every moment (fixtures of 002).
#[test]
fn vectors_cover_every_event_and_moment() {
    let logs: Vec<FightLog> = VECTORS
        .iter()
        .map(|v| simulate_vector(&fixture(v.a), &fixture(v.b), v.seed))
        .collect();
    let any =
        |wanted: &dyn Fn(&FightEvent) -> bool| logs.iter().any(|l| l.events.iter().any(wanted));
    assert!(any(&|e| matches!(e, FightEvent::Intro { .. })));
    assert!(any(&|e| matches!(e, FightEvent::Attack { .. })));
    assert!(any(&|e| matches!(e, FightEvent::Hit { crit: true, .. })));
    assert!(any(&|e| matches!(e, FightEvent::Hit { weak: true, .. })));
    assert!(any(&|e| matches!(e, FightEvent::Block { .. })));
    assert!(any(&|e| matches!(e, FightEvent::Dodge { .. })));
    assert!(any(&|e| matches!(e, FightEvent::Knockdown { .. })));
    assert!(any(&|e| matches!(e, FightEvent::LowHp { .. })));
    assert!(any(&|e| matches!(
        e,
        FightEvent::Moment {
            kind: MomentKind::Comeback,
            ..
        }
    )));
    assert!(any(&|e| matches!(
        e,
        FightEvent::Moment {
            kind: MomentKind::DecisiveCrit,
            ..
        }
    )));
    assert!(any(&|e| matches!(
        e,
        FightEvent::Moment {
            kind: MomentKind::ClutchKo,
            ..
        }
    )));
    assert!(any(&|e| matches!(e, FightEvent::Ko { .. })));
    assert!(any(&|e| matches!(e, FightEvent::TimeUp { .. })));
    assert!(any(&|e| matches!(
        e,
        FightEvent::End {
            reason: EndReason::Decision,
            ..
        }
    )));
}

/// FNV-1a, 64 bits.
fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// (b) Digest of 1000 fights. Fight `i` (0..1000) uses seed `derive_seed(i)` and the ordered
/// pair `p = i mod 16` of `FIXTURES` (striker, trickster, tank, turtle): `a = FIXTURES[p / 4]`,
/// `b = FIXTURES[p % 4]`, mirrors included. Changing this mapping regenerates the digest.
#[test]
fn digest_of_1000_fights() {
    let all = fixtures();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for i in 0..1000_u32 {
        let p = (i % 16) as usize;
        let (a, b) = match (all.get(p / 4), all.get(p % 4)) {
            (Some((_, a)), Some((_, b))) => (a, b),
            _ => panic!("pair {p}"),
        };
        let log = simulate_vector(a, b, i);
        hash = fnv1a(
            hash,
            &canonical_json(&log).unwrap_or_else(|e| panic!("{e}")),
        );
    }
    let actual = format!("{hash:016x}\n");
    check_golden(
        "digest-1000.txt",
        actual.as_bytes(),
        include_bytes!("golden/digest-1000.txt"),
    );
}

/// (c) Canonical specs of the fixtures, and their round trip.
#[test]
fn specs_match_their_golden_files() {
    let expected: [(&str, &[u8]); 4] = [
        ("striker", include_bytes!("golden/spec-striker.json")),
        ("trickster", include_bytes!("golden/spec-trickster.json")),
        ("tank", include_bytes!("golden/spec-tank.json")),
        ("turtle", include_bytes!("golden/spec-turtle.json")),
    ];
    assert_eq!(expected.map(|(n, _)| n), FIXTURES.map(|(n, _)| n));
    for (name, golden) in expected {
        let spec = fixture(name);
        let canonical = canonical_spec_json(&spec).unwrap_or_else(|e| panic!("{e}"));
        let back: CharacterSpec =
            serde_json::from_slice(&canonical).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back, spec, "{name}: round trip");
        check_golden(
            &format!("spec-{name}.json"),
            &golden_bytes(canonical),
            golden,
        );
    }
}

/// Finds the seeds of `VECTORS`: `cargo test -p engine --test golden -- --ignored --nocapture`.
#[test]
#[ignore = "tool to choose the golden seeds"]
#[allow(clippy::print_stderr)]
fn search_golden_seeds() {
    let all = fixtures();
    for (file, covers) in TARGETS {
        let mut found = Vec::new();
        'search: for seed in 0..20_000_u32 {
            for (an, a) in &all {
                for (bn, b) in &all {
                    if covers(&simulate_vector(a, b, seed)) {
                        found.push(format!("{an} vs {bn} seed {seed}"));
                        if found.len() == 6 {
                            break 'search;
                        }
                    }
                }
            }
        }
        eprintln!("{file}: {found:?}");
    }
}
