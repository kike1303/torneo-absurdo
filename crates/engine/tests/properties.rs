//! Property tests without extra crates (003 "Estrategia de pruebas"): determinism and the RF2
//! invariants over derived seeds, the totality of RF21, the public API of RF4 and the
//! breather of 006 RF7.
mod common;

use common::{
    assert_invariants, assert_only_codes, derive_seed, fixture, fixtures, resolution_times, v1,
};
use engine::{
    Ability, CharacterSpec, Element, MoveKind, Personality, Seed, SpecError, Stats,
    TIMELINE_CONTRACT, canonical_json, canonical_spec_json, simulate,
};

const SEEDS_PER_PAIR: u32 = 1000;

/// RF2 and determinism: every unordered pair of fixtures plus the mirrors (10 pairs), 1000
/// seeds each, simulated twice. Unordered instead of ordered keeps the debug run under ~15 s
/// (plan-a, Riesgos and Desviaciones); both sides attack in every fight anyway.
#[test]
fn every_log_is_deterministic_and_respects_rf2() {
    let r = v1();
    let all = fixtures();
    for (i, (_, a)) in all.iter().enumerate() {
        for (j, (_, b)) in all.iter().enumerate().skip(i) {
            for k in 0..SEEDS_PER_PAIR {
                let n = u32::try_from(i * 4 + j).unwrap() * SEEDS_PER_PAIR + k;
                let first = simulate(&Seed::from_bytes(derive_seed(n)), a, b, &r).unwrap();
                let again = simulate(&Seed::from_bytes(derive_seed(n)), a, b, &r).unwrap();
                assert_eq!(
                    canonical_json(&first).unwrap(),
                    canonical_json(&again).unwrap()
                );
                assert_invariants(&first, &r);
                assert_only_codes(&first, a, b);
            }
        }
    }
}

/// 006 RF7: share (‰) of the fights with more than `breather_after_resolutions` resolutions that
/// must show a separation of at least `breather_gap_min()`.
const BREATHER_SEEN_MIN_PERMILLE: u64 = 900;

/// 006 RF7, over the same 10 000 logs (simulated once): the shortest separation between
/// resolutions is at least `strike_gap_min()`, and breathers do happen.
#[test]
fn breathers_happen_and_keep_the_contract() {
    let r = v1();
    assert!(r.strike_gap_min() >= TIMELINE_CONTRACT.strike_gap_ms);
    let all = fixtures();
    let mut min_gap = u32::MAX;
    let (mut eligible, mut paused) = (0_u64, 0_u64);
    for (i, (_, a)) in all.iter().enumerate() {
        for (j, (_, b)) in all.iter().enumerate().skip(i) {
            for k in 0..SEEDS_PER_PAIR {
                let n = u32::try_from(i * 4 + j).unwrap() * SEEDS_PER_PAIR + k;
                let log = simulate(&Seed::from_bytes(derive_seed(n)), a, b, &r).unwrap();
                let times = resolution_times(&log);
                let gaps: Vec<u32> = times
                    .windows(2)
                    .filter_map(|w| match w {
                        [x, y] => Some(y - x),
                        _ => None,
                    })
                    .collect();
                min_gap = gaps.iter().copied().fold(min_gap, u32::min);
                if times.len() > usize::try_from(r.breather_after_resolutions()).unwrap() {
                    eligible += 1;
                    if gaps.iter().any(|g| *g >= r.breather_gap_min()) {
                        paused += 1;
                    }
                }
            }
        }
    }
    assert!(
        min_gap >= r.strike_gap_min(),
        "shortest separation {min_gap}"
    );
    assert!(eligible > 0);
    assert!(
        paused * 1000 >= BREATHER_SEEN_MIN_PERMILLE * eligible,
        "{paused} of {eligible} fights with a pause"
    );
}

fn spec(
    stats: [u8; 5],
    personality: [u8; 3],
    abilities: Vec<Ability>,
    weaknesses: Vec<Element>,
) -> CharacterSpec {
    let [hp, attack, defense, speed, special] = stats;
    let [aggression, caution, showmanship] = personality;
    CharacterSpec {
        slug: "edge-case".to_owned(),
        stats: Stats {
            hp,
            attack,
            defense,
            speed,
            special,
        },
        personality: Personality {
            aggression,
            caution,
            showmanship,
        },
        abilities,
        weaknesses,
    }
}

fn ability(id: &str, kind: MoveKind, power: u8, cooldown: u8, element: Option<Element>) -> Ability {
    Ability {
        id: id.to_owned(),
        kind,
        power,
        cooldown,
        element,
    }
}

fn one_heavy() -> Vec<Ability> {
    vec![ability("bump", MoveKind::Heavy, 20, 0, None)]
}

/// RF21: valid extremes simulate and respect RF2.
#[test]
fn valid_extremes_respect_rf2() {
    let r = v1();
    let specials = (0..4)
        .map(|i| {
            ability(
                &format!("nova_{i}"),
                MoveKind::Special,
                100,
                2,
                Some(Element::Fire),
            )
        })
        .collect::<Vec<_>>();
    let extremes = [
        spec([1, 1, 1, 1, 1], [0, 0, 0], one_heavy(), vec![]),
        spec([100, 98, 100, 1, 1], [100, 100, 100], one_heavy(), vec![]),
        spec([100, 100, 98, 1, 1], [0, 0, 0], one_heavy(), vec![]),
        spec(
            [1, 100, 1, 100, 98],
            [100, 0, 100],
            specials.clone(),
            vec![Element::Fire, Element::Ice],
        ),
        spec(
            [60, 60, 60, 60, 60],
            [50, 50, 50],
            specials,
            vec![Element::Fire, Element::Water],
        ),
        spec([1, 1, 100, 100, 98], [0, 100, 0], one_heavy(), vec![]),
    ];
    for a in &extremes {
        assert_eq!(a.validate(&r), Ok(()), "{a:?}");
        for b in extremes
            .iter()
            .chain([&fixture("turtle"), &fixture("striker")])
        {
            for k in 0..50 {
                let log = simulate(&Seed::from_bytes(derive_seed(k)), a, b, &r).unwrap();
                assert_invariants(&log, &r);
            }
        }
    }
}

/// RF21: the worst case for the number of events stays within the analytic bound (418).
#[test]
fn worst_case_events_stay_within_the_bound() {
    let r = v1();
    // speed 100 (most actions), defense 100 and as much hp as the budget allows (longest).
    let wall = spec([98, 1, 100, 100, 1], [0, 0, 0], one_heavy(), vec![]);
    let mut longest = 0;
    for k in 0..SEEDS_PER_PAIR {
        let log = simulate(&Seed::from_bytes(derive_seed(k)), &wall, &wall, &r).unwrap();
        assert_invariants(&log, &r);
        longest = longest.max(log.events.len());
    }
    assert!(u32::try_from(longest).unwrap() <= r.max_events_bound());
    assert!(r.max_events_bound() <= 418);
}

/// RF21 and RF4: invalid characters are rejected with their code, on either side.
#[test]
fn invalid_specs_return_their_error() {
    let r = v1();
    let seed = Seed::from_bytes(derive_seed(0));
    let good = fixture("striker");
    let cases: Vec<(SpecError, CharacterSpec)> = vec![
        (
            SpecError::SlugInvalid,
            CharacterSpec {
                slug: "Bad".to_owned(),
                ..good.clone()
            },
        ),
        (
            SpecError::StatOutOfRange,
            spec([0, 1, 1, 1, 1], [0, 0, 0], one_heavy(), vec![]),
        ),
        (
            SpecError::StatOutOfRange,
            spec([101, 1, 1, 1, 1], [0, 0, 0], one_heavy(), vec![]),
        ),
        (
            SpecError::BudgetExceeded,
            spec([100; 5], [0, 0, 0], one_heavy(), vec![]),
        ),
        (
            SpecError::PersonalityOutOfRange,
            spec([1; 5], [0, 101, 0], one_heavy(), vec![]),
        ),
        (
            SpecError::AbilityCount,
            spec([1; 5], [0, 0, 0], vec![], vec![]),
        ),
        (
            SpecError::AbilityIdInvalid,
            spec(
                [1; 5],
                [0, 0, 0],
                vec![ability("strike", MoveKind::Heavy, 20, 0, None)],
                vec![],
            ),
        ),
        (
            SpecError::AbilityIdDuplicate,
            spec(
                [1; 5],
                [0, 0, 0],
                vec![
                    ability("bump", MoveKind::Heavy, 20, 0, None),
                    ability("bump", MoveKind::Heavy, 20, 0, None),
                ],
                vec![],
            ),
        ),
        (
            SpecError::PowerOutOfRange,
            spec(
                [1; 5],
                [0, 0, 0],
                vec![ability("bump", MoveKind::Special, 39, 2, None)],
                vec![],
            ),
        ),
        (
            SpecError::CooldownOutOfRange,
            spec(
                [1; 5],
                [0, 0, 0],
                vec![ability("bump", MoveKind::Heavy, 20, 6, None)],
                vec![],
            ),
        ),
        (
            SpecError::WeaknessCount,
            spec(
                [1; 5],
                [0, 0, 0],
                one_heavy(),
                vec![Element::Fire, Element::Air, Element::Ice],
            ),
        ),
        (
            SpecError::WeaknessDuplicate,
            spec(
                [1; 5],
                [0, 0, 0],
                one_heavy(),
                vec![Element::Air, Element::Air],
            ),
        ),
    ];
    let mut covered: Vec<SpecError> = Vec::new();
    for (error, bad) in &cases {
        assert_eq!(simulate(&seed, bad, &good, &r), Err(*error), "{bad:?} as a");
        assert_eq!(simulate(&seed, &good, bad, &r), Err(*error), "{bad:?} as b");
        covered.push(*error);
    }
    // Every code except the EventLimit guard (covered by the unit tests of sim.rs).
    for error in SpecError::ALL {
        assert!(
            covered.contains(&error) || error == SpecError::EventLimit,
            "{error:?}"
        );
    }
}

/// RF4: canonical spec round trip and `deny_unknown_fields`.
#[test]
fn character_spec_round_trip() {
    for (_, spec) in fixtures() {
        let bytes = canonical_spec_json(&spec).unwrap();
        assert_eq!(
            serde_json::from_slice::<CharacterSpec>(&bytes).unwrap(),
            spec
        );
    }
    let mut value = serde_json::to_value(fixture("tank")).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("rivals".to_owned(), serde_json::Value::Array(vec![]));
    assert!(serde_json::from_value::<CharacterSpec>(value).is_err());
}

/// Whether `$t` implements `$trait`, without extra crates (inherent associated consts win over
/// trait ones only when the bound holds).
macro_rules! implements {
    ($t:ty: $($bound:tt)+) => {{
        #[allow(dead_code)] // unused when the bound holds
        trait DoesNot {
            const IMPLS: bool = false;
        }
        impl<T: ?Sized> DoesNot for T {}
        struct Probe<T: ?Sized>(std::marker::PhantomData<T>);
        #[allow(dead_code)]
        impl<T: ?Sized + $($bound)+> Probe<T> {
            const IMPLS: bool = true;
        }
        <Probe<$t>>::IMPLS
    }};
}

/// RF4, RF20: the seed is opaque.
#[test]
fn seed_is_opaque() {
    assert_eq!(
        format!("{:?}", Seed::from_bytes([7; 32])),
        "Seed(<redacted>)"
    );
    assert!(implements!(Seed: std::fmt::Debug));
    assert!(!implements!(Seed: Clone));
    assert!(!implements!(Seed: serde::Serialize));
    assert!(!implements!(Seed: std::fmt::Display));
    assert!(!implements!(Seed: std::fmt::LowerHex));
    assert!(!implements!(Seed: std::fmt::UpperHex));
    // The probe itself works.
    assert!(implements!(CharacterSpec: Clone));
}

/// RF4: only version 1 exists.
#[test]
fn ruleset_versions() {
    assert!(engine::Ruleset::for_version(1).is_some());
    assert!(engine::Ruleset::for_version(0).is_none());
    assert!(engine::Ruleset::for_version(2).is_none());
}

/// RF11: compact JSON (no spaces or newlines outside strings), fields in declaration order.
#[test]
fn canonical_json_is_compact() {
    let r = v1();
    let log = simulate(
        &Seed::from_bytes(derive_seed(1)),
        &fixture("striker"),
        &fixture("tank"),
        &r,
    )
    .unwrap();
    let text = String::from_utf8(canonical_json(&log).unwrap()).unwrap();
    assert!(!text.contains(' ') && !text.contains('\n'));
    assert!(text.starts_with(r#"{"ruleset_version":1,"a":{"slug":"striker","max_hp":700},"b":{"slug":"tank","max_hp":860},"events":[{"type":"intro","t":0},"#));
    let spec = String::from_utf8(canonical_spec_json(&fixture("turtle")).unwrap()).unwrap();
    assert_eq!(
        spec,
        concat!(
            r#"{"slug":"turtle","stats":{"hp":100,"attack":1,"defense":100,"speed":40,"special":1},"#,
            r#""personality":{"aggression":0,"caution":100,"showmanship":0},"#,
            r#""abilities":[{"id":"shell_bump","kind":"heavy","power":20,"cooldown":5,"element":null}],"#,
            r#""weaknesses":[]}"#
        )
    );
}
