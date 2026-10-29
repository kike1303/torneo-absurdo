//! Shared by `golden.rs` and `properties.rs`: fixtures, derived seeds and the RF2 invariants.
//! Fixtures are embedded (`include_str!`) so the same tests run in WASI without a file system.
#![allow(dead_code)] // each test crate uses a subset

use engine::{
    CharacterSpec, EndReason, FightEvent, FightLog, MomentKind, Ruleset, Side, TIMELINE_CONTRACT,
};

/// Fixture name and its JSON, in this fixed order (the digest of `golden.rs` depends on it).
pub const FIXTURES: [(&str, &str); 4] = [
    ("striker", include_str!("../fixtures/striker.json")),
    ("trickster", include_str!("../fixtures/trickster.json")),
    ("tank", include_str!("../fixtures/tank.json")),
    ("turtle", include_str!("../fixtures/turtle.json")),
];

pub fn fixture(name: &str) -> CharacterSpec {
    let json = FIXTURES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, json)| *json)
        .unwrap_or_else(|| panic!("unknown fixture {name}"));
    serde_json::from_str(json).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

pub fn fixtures() -> Vec<(&'static str, CharacterSpec)> {
    FIXTURES
        .iter()
        .map(|(name, _)| (*name, fixture(name)))
        .collect()
}

pub fn v1() -> Ruleset {
    Ruleset::for_version(1).unwrap_or_else(|| panic!("ruleset 1 exists"))
}

/// Fixed root of every test seed (exactly 32 bytes).
pub const ROOT_SEED: [u8; 32] = *b"torneo-absurdo-engine-golden-v1!";

/// Seed `i`: the root with its first 4 bytes XOR `i` (little endian).
pub fn derive_seed(i: u32) -> [u8; 32] {
    let mut seed = ROOT_SEED;
    for (byte, x) in seed.iter_mut().zip(i.to_le_bytes()) {
        *byte ^= x;
    }
    seed
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_resolution(event: &FightEvent) -> bool {
    matches!(
        event,
        FightEvent::Hit { .. } | FightEvent::Block { .. } | FightEvent::Dodge { .. }
    )
}

/// Times of the resolutions (`hit`, `block`, `dodge`), in log order.
pub fn resolution_times(log: &FightLog) -> Vec<u32> {
    log.events
        .iter()
        .filter(|e| is_resolution(e))
        .map(FightEvent::t)
        .collect()
}

fn index(side: Side) -> usize {
    match side {
        Side::A => 0,
        Side::B => 1,
    }
}

/// Every invariant of RF2, plus the structure the renderer relies on (each `attack` is followed
/// by its resolution, unique events appear at most once per side).
pub fn assert_invariants(log: &FightLog, r: &Ruleset) {
    let events = &log.events;
    let ctx = || format!("{} vs {}", log.a.slug, log.b.slug);
    assert_eq!(log.ruleset_version, r.version(), "{}", ctx());

    // Order and structure.
    assert_eq!(
        events.first(),
        Some(&FightEvent::Intro { t: 0 }),
        "{}",
        ctx()
    );
    for pair in events.windows(2) {
        if let [prev, next] = pair {
            assert!(prev.t() <= next.t(), "{}: t decreases at {next:?}", ctx());
        }
    }
    let ends: Vec<&FightEvent> = events
        .iter()
        .filter(|e| matches!(e, FightEvent::End { .. }))
        .collect();
    assert_eq!(ends.len(), 1, "{}: exactly one end", ctx());
    let Some(FightEvent::End {
        t: end_t,
        winner,
        reason,
    }) = events.last()
    else {
        panic!("{}: end is not the last event", ctx());
    };
    assert!(*end_t <= r.time_limit_ms() + r.end_ms(), "{}", ctx());
    assert!(events.len() <= usize::from(r.max_events()), "{}", ctx());
    assert!(events.len() <= 512, "{}", ctx());

    // HP chain, per side.
    let max_hp = [log.a.max_hp, log.b.max_hp];
    let mut hp = max_hp;
    let mut pending_ko: Option<Side> = None;
    let mut kos = Vec::new();
    let mut time_ups = 0;
    let mut low_hp = [0; 2];
    let mut moments: Vec<(MomentKind, Side)> = Vec::new();
    let mut last_attack: Option<(Side, String)> = None;
    for event in events {
        if let Some(side) = pending_ko {
            assert!(
                matches!(event, FightEvent::Moment { .. } | FightEvent::Ko { .. }),
                "{}: hp 0 of {side:?} not followed by its ko: {event:?}",
                ctx()
            );
        }
        match event {
            FightEvent::Attack {
                attacker, ability, ..
            } => {
                assert!(
                    last_attack.is_none(),
                    "{}: attack without resolution",
                    ctx()
                );
                last_attack = Some((*attacker, ability.clone()));
            }
            FightEvent::Hit {
                attacker,
                ability,
                damage,
                hp: after,
                ..
            }
            | FightEvent::Block {
                attacker,
                ability,
                damage,
                hp: after,
                ..
            } => {
                assert_eq!(
                    last_attack.take(),
                    Some((*attacker, ability.clone())),
                    "{}",
                    ctx()
                );
                let target = index(attacker.opponent());
                let before = hp.get(target).copied().unwrap_or_default();
                assert!(*damage <= before, "{}: damage above the previous hp", ctx());
                assert_eq!(*after, before - damage, "{}: hp chain", ctx());
                if let Some(slot) = hp.get_mut(target) {
                    *slot = *after;
                }
                if *after == 0 {
                    pending_ko = Some(attacker.opponent());
                }
            }
            FightEvent::Dodge {
                attacker, ability, ..
            } => {
                assert_eq!(
                    last_attack.take(),
                    Some((*attacker, ability.clone())),
                    "{}",
                    ctx()
                );
            }
            FightEvent::LowHp {
                side, hp: value, ..
            } => {
                let i = index(*side);
                assert_eq!(
                    Some(value),
                    hp.get(i),
                    "{}: low_hp with the current hp",
                    ctx()
                );
                assert!(*value > 0, "{}", ctx());
                let max = u32::from(max_hp.get(i).copied().unwrap_or(1));
                assert!(
                    u32::from(*value) * 1000 / max <= r.low_hp_permille(),
                    "{}",
                    ctx()
                );
                if let Some(count) = low_hp.get_mut(i) {
                    *count += 1;
                }
            }
            FightEvent::Moment { kind, side, .. } => {
                assert!(
                    !moments.contains(&(*kind, *side)),
                    "{}: repeated moment",
                    ctx()
                );
                moments.push((*kind, *side));
            }
            FightEvent::Ko { t, side } => {
                assert_eq!(pending_ko.take(), Some(*side), "{}: ko without hp 0", ctx());
                kos.push((*t, *side));
            }
            FightEvent::TimeUp { t } => {
                assert_eq!(*t, r.time_limit_ms(), "{}", ctx());
                time_ups += 1;
            }
            FightEvent::Intro { .. } | FightEvent::Knockdown { .. } | FightEvent::End { .. } => {}
        }
    }
    assert!(
        last_attack.is_none(),
        "{}: attack without resolution",
        ctx()
    );
    assert!(
        low_hp.iter().all(|count| *count <= 1),
        "{}: low_hp twice",
        ctx()
    );

    // Ending.
    match reason {
        EndReason::Ko => {
            assert_eq!(kos.len(), 1, "{}: exactly one ko", ctx());
            assert_eq!(time_ups, 0, "{}", ctx());
            if let Some((ko_t, side)) = kos.first() {
                assert!(
                    *end_t >= ko_t + TIMELINE_CONTRACT.win_gap_ms + TIMELINE_CONTRACT.win_pose_ms,
                    "{}",
                    ctx()
                );
                assert_eq!(*end_t, ko_t + r.end_ms(), "{}", ctx());
                assert_eq!(*winner, Some(side.opponent()), "{}", ctx());
            }
        }
        EndReason::Decision | EndReason::Draw => {
            assert_eq!(time_ups, 1, "{}: exactly one time_up", ctx());
            assert!(kos.is_empty(), "{}", ctx());
            assert_eq!(*end_t, r.time_limit_ms() + r.end_ms(), "{}", ctx());
        }
    }
    assert_eq!(winner.is_none(), *reason == EndReason::Draw, "{}", ctx());
    for (kind, side) in &moments {
        if matches!(kind, MomentKind::DecisiveCrit | MomentKind::ClutchKo) {
            assert_eq!(
                Some(*side),
                *winner,
                "{}: KO moments belong to the winner",
                ctx()
            );
        }
    }

    // Pacing: the `Timeline` contract (002, 005; 006 RF5).
    let c = TIMELINE_CONTRACT;
    let resolutions: Vec<u32> = events
        .iter()
        .filter(|e| is_resolution(e))
        .map(FightEvent::t)
        .collect();
    if let Some(first) = resolutions.first() {
        assert!(
            *first >= c.intro_gap_ms,
            "{}: first resolution at {first}",
            ctx()
        );
    }
    for pair in resolutions.windows(2) {
        if let [prev, next] = pair {
            assert!(
                next - prev >= c.strike_gap_ms,
                "{}: resolutions {prev} and {next}",
                ctx()
            );
        }
    }
    for event in events {
        if let FightEvent::Knockdown {
            t,
            side,
            duration_ms,
        } = event
        {
            assert!(
                (c.knockdown_min_ms..=c.knockdown_max_ms).contains(&u32::from(*duration_ms)),
                "{}",
                ctx()
            );
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    FightEvent::Hit { t: ht, attacker, .. } if ht == t && attacker.opponent() == *side
                )),
                "{}: knockdown without a hit on that side",
                ctx()
            );
            let until = t + u32::from(*duration_ms);
            assert!(
                !resolutions.iter().any(|rt| rt > t && *rt <= until),
                "{}: resolution during a knockdown at {t}",
                ctx()
            );
        }
    }
}

/// RF3: every string of the log is a fighter's slug or an ability code of the attacker.
pub fn assert_only_codes(log: &FightLog, a: &CharacterSpec, b: &CharacterSpec) {
    assert_eq!(log.a.slug, a.slug);
    assert_eq!(log.b.slug, b.slug);
    for event in &log.events {
        let (attacker, ability) = match event {
            FightEvent::Attack {
                attacker, ability, ..
            }
            | FightEvent::Hit {
                attacker, ability, ..
            }
            | FightEvent::Block {
                attacker, ability, ..
            }
            | FightEvent::Dodge {
                attacker, ability, ..
            } => (attacker, ability),
            _ => continue,
        };
        let spec = match attacker {
            Side::A => a,
            Side::B => b,
        };
        assert!(
            ability == "strike" || spec.abilities.iter().any(|ab| &ab.id == ability),
            "{ability} is not a code of {}",
            spec.slug
        );
    }
}
