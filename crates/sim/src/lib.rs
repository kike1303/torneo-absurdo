//! Mass fight simulation and balance report (003 RF15, RF17).
//!
//! [`run`] plays a deterministic schedule of fights with the real engine and returns a
//! [`Report`] of counts: the same roster, ruleset and root seed always give the same report.
//! The time measurement lives in the binary (`main.rs`), never here, so `Report` stays
//! deterministic.
//!
//! Schedule (plan-b decision 7): a `ChaCha20Rng` seeded with the root seed yields each fight
//! seed as 8 `next_u32()` in little endian. First the cross fights: for every pair `i < j` in
//! slug order, `m = fights / (2 * pairs)` seeds, each played as `(i, j)` and as `(j, i)`. Then
//! the mirrors: for every `i` in slug order, `m` new seeds played as `(i, i)`. Changing this
//! order changes the report (not the engine goldens).
//!
//! The root seed is the simulator's own: the CLI never receives the seed of a real fight
//! (ADR-0002).
use engine::{CharacterSpec, Ruleset, Seed, SpecError, canonical_json, simulate};
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

pub mod args;
mod report;

pub use report::{Criteria, Failure, Report, Warning};

use report::Tally;

/// Root seed when `--seed` is absent.
pub const DEFAULT_SEED: [u8; 32] = *b"torneo-absurdo-sim-default-v1!!!";

/// The ruleset `sim` measures (the only one of engine v0).
pub const RULESET_VERSION: u16 = 1;

/// What to simulate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `N`: the budget of cross fights (`2 * pairs * m <= N`).
    pub fights: u32,
    pub seed: [u8; 32],
}

/// Why a simulation cannot run. Codes, no text of other crates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SimError {
    /// Fewer than 2 characters: there is no cross fight.
    #[error("sim.roster_too_small: {0} character(s), at least 2 are needed")]
    RosterTooSmall(usize),
    /// `fights < 2 * pairs`, so `m = 0`.
    #[error("sim.too_few_fights: {fights} fight(s) for {pairs} pair(s), at least {} are needed", 2 * u64::from(*pairs))]
    TooFewFights { fights: u32, pairs: u32 },
    /// Two characters share a slug.
    #[error("sim.duplicate_slug: {0}")]
    DuplicateSlug(String),
    /// The engine rejects a character.
    #[error("sim.spec ({source}): {slug}")]
    Spec { slug: String, source: SpecError },
    /// `canonical_json` failed (a guard: the log has no maps nor floats).
    #[error("sim.encode_failed")]
    Encode,
    /// A log without `end` (a guard: the engine always writes it).
    #[error("sim.log_without_end")]
    LogWithoutEnd,
}

impl SimError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::RosterTooSmall(_) => "sim.roster_too_small",
            Self::TooFewFights { .. } => "sim.too_few_fights",
            Self::DuplicateSlug(_) => "sim.duplicate_slug",
            Self::Spec { .. } => "sim.spec",
            Self::Encode => "sim.encode_failed",
            Self::LogWithoutEnd => "sim.log_without_end",
        }
    }
}

/// The counts of the schedule (RF15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// `n`, distinct characters.
    pub characters: u32,
    /// `P = C(n, 2)`.
    pub pairs: u32,
    /// `m = N / (2 * P)`, seeds per pair and mirrors per character.
    pub seeds_per_pair: u32,
}

impl Plan {
    /// # Errors
    /// [`SimError::RosterTooSmall`] with fewer than 2 characters and
    /// [`SimError::TooFewFights`] when `m` would be 0.
    pub fn new(characters: usize, fights: u32) -> Result<Self, SimError> {
        let n = u32::try_from(characters).map_err(|_| SimError::RosterTooSmall(characters))?;
        if n < 2 {
            return Err(SimError::RosterTooSmall(characters));
        }
        let pairs = u32::try_from(u64::from(n) * u64::from(n - 1) / 2).map_err(|_| {
            SimError::TooFewFights {
                fights,
                pairs: u32::MAX,
            }
        })?;
        let per_fight_pair = u64::from(pairs) * 2;
        let seeds_per_pair = u32::try_from(u64::from(fights) / per_fight_pair).unwrap_or(0);
        if seeds_per_pair == 0 {
            return Err(SimError::TooFewFights { fights, pairs });
        }
        Ok(Self {
            characters: n,
            pairs,
            seeds_per_pair,
        })
    }

    /// `2 * P * m`, all of them counted in the report.
    #[must_use]
    pub const fn cross_fights(&self) -> u64 {
        2 * self.pairs as u64 * self.seeds_per_pair as u64
    }

    /// `n * m`, only for the side-a bias.
    #[must_use]
    pub const fn mirror_fights(&self) -> u64 {
        self.characters as u64 * self.seeds_per_pair as u64
    }
}

/// Runs the schedule over `roster` (in any order: it is sorted by slug) and tallies the report.
///
/// Every log also goes through `canonical_json`, so the time the binary measures around this
/// call is the one of the RNF (simulation plus encoding).
///
/// # Errors
/// See [`SimError`]. Every character is validated before the first fight.
pub fn run(
    config: &Config,
    roster: &[CharacterSpec],
    ruleset: &Ruleset,
) -> Result<Report, SimError> {
    let mut roster: Vec<&CharacterSpec> = roster.iter().collect();
    roster.sort_by(|x, y| x.slug.cmp(&y.slug));
    for (i, spec) in roster.iter().enumerate() {
        if roster.iter().take(i).any(|other| other.slug == spec.slug) {
            return Err(SimError::DuplicateSlug(spec.slug.clone()));
        }
        spec.validate(ruleset).map_err(|source| SimError::Spec {
            slug: spec.slug.clone(),
            source,
        })?;
    }
    let plan = Plan::new(roster.len(), config.fights)?;
    let slugs: Vec<String> = roster.iter().map(|spec| spec.slug.clone()).collect();
    let mut tally = Tally::new(slugs, ruleset);
    let mut rng = ChaCha20Rng::from_seed(config.seed);

    for (i, a) in roster.iter().enumerate() {
        for (j, b) in roster.iter().enumerate().skip(i + 1) {
            for _ in 0..plan.seeds_per_pair {
                let seed = next_seed(&mut rng);
                tally.cross(i, j, &fight(seed, a, b, ruleset)?)?;
                tally.cross(j, i, &fight(seed, b, a, ruleset)?)?;
            }
        }
    }
    for spec in &roster {
        for _ in 0..plan.seeds_per_pair {
            let seed = next_seed(&mut rng);
            tally.mirror(&fight(seed, spec, spec, ruleset)?)?;
        }
    }
    Ok(tally.finish(&plan, config.seed))
}

/// 32 bytes: 8 `next_u32()` in little endian.
fn next_seed(rng: &mut ChaCha20Rng) -> [u8; 32] {
    let mut seed = [0_u8; 32];
    for chunk in seed.as_chunks_mut::<4>().0 {
        *chunk = rng.next_u32().to_le_bytes();
    }
    seed
}

fn fight(
    seed: [u8; 32],
    a: &CharacterSpec,
    b: &CharacterSpec,
    ruleset: &Ruleset,
) -> Result<engine::FightLog, SimError> {
    let log =
        simulate(&Seed::from_bytes(seed), a, b, ruleset).map_err(|source| SimError::Spec {
            slug: a.slug.clone(),
            source,
        })?;
    // The encoding is part of the measured work (RNF); its bytes are not kept.
    canonical_json(&log).map_err(|_| SimError::Encode)?;
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1() -> Ruleset {
        Ruleset::for_version(RULESET_VERSION).unwrap_or_else(|| panic!("ruleset v1"))
    }

    fn spec(slug: &str, hp: u8, attack: u8) -> CharacterSpec {
        let json = format!(
            r#"{{"slug":"{slug}","stats":{{"hp":{hp},"attack":{attack},"defense":60,"speed":60,"special":60}},
            "personality":{{"aggression":50,"caution":30,"showmanship":40}},
            "abilities":[{{"id":"big_swing","kind":"heavy","power":40,"cooldown":2}},
            {{"id":"flare","kind":"special","power":70,"cooldown":4,"element":"fire"}}],
            "weaknesses":["water"]}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn the_default_seed_is_32_bytes_of_text() {
        assert_eq!(&DEFAULT_SEED, b"torneo-absurdo-sim-default-v1!!!");
    }

    #[test]
    fn plan_counts_for_2_3_and_4_characters() {
        for (n, pairs, m) in [(2, 1, 5000), (3, 3, 1666), (4, 6, 833)] {
            let plan = Plan::new(n, 10_000).unwrap();
            assert_eq!((plan.pairs, plan.seeds_per_pair), (pairs, m), "{n}");
            assert_eq!(plan.cross_fights(), 2 * u64::from(pairs) * u64::from(m));
            assert!(plan.cross_fights() <= 10_000);
            assert_eq!(
                plan.mirror_fights(),
                u64::try_from(n).unwrap() * u64::from(m)
            );
        }
        assert_eq!(Plan::new(4, 10_000).unwrap().cross_fights(), 9996);
        assert_eq!(Plan::new(4, 10_000).unwrap().mirror_fights(), 3332);
    }

    #[test]
    fn plan_errors() {
        assert_eq!(Plan::new(0, 10), Err(SimError::RosterTooSmall(0)));
        assert_eq!(Plan::new(1, 10), Err(SimError::RosterTooSmall(1)));
        assert_eq!(
            Plan::new(4, 11),
            Err(SimError::TooFewFights {
                fights: 11,
                pairs: 6
            })
        );
        assert_eq!(Plan::new(4, 12).unwrap().seeds_per_pair, 1);
        assert_eq!(
            Plan::new(2, 1),
            Err(SimError::TooFewFights {
                fights: 1,
                pairs: 1
            })
        );
    }

    #[test]
    fn next_seed_is_eight_little_endian_words() {
        let mut a = ChaCha20Rng::from_seed(DEFAULT_SEED);
        let mut b = ChaCha20Rng::from_seed(DEFAULT_SEED);
        let seed = next_seed(&mut a);
        let words: Vec<u32> = (0..8).map(|_| b.next_u32()).collect();
        for (chunk, word) in seed.as_chunks::<4>().0.iter().zip(words) {
            assert_eq!(*chunk, word.to_le_bytes());
        }
        assert_ne!(next_seed(&mut a), seed);
    }

    #[test]
    fn run_plays_the_whole_schedule() {
        let roster = [
            spec("cc-three", 60, 60),
            spec("aa-one", 60, 60),
            spec("bb-two", 70, 50),
        ];
        let config = Config {
            fights: 60,
            seed: DEFAULT_SEED,
        };
        let report = run(&config, &roster, &v1()).unwrap();
        assert_eq!(report.characters, ["aa-one", "bb-two", "cc-three"]);
        assert_eq!((report.pairs, report.seeds_per_pair), (3, 10));
        assert_eq!(report.cross_fights, 60);
        assert_eq!(report.mirror_fights, 30);
        assert_eq!(report.mirrors.fights, 30);
        let appearances: Vec<u32> = report.per_character.iter().map(|c| c.appearances).collect();
        assert_eq!(appearances, [40, 40, 40]);
        let endings = report.endings;
        assert_eq!(endings.ko + endings.decision + endings.draw, 60);
        for m in &report.matchups {
            assert_eq!(m.fights, 20);
            assert_eq!(m.wins_first + m.wins_second + m.draws, 20);
        }
    }

    #[test]
    fn the_same_root_gives_the_same_report_and_another_root_another() {
        let roster = [spec("aa-one", 60, 60), spec("bb-two", 70, 50)];
        let config = Config {
            fights: 200,
            seed: DEFAULT_SEED,
        };
        let first = run(&config, &roster, &v1()).unwrap();
        let reversed = [roster[1].clone(), roster[0].clone()];
        assert_eq!(run(&config, &reversed, &v1()).unwrap(), first);
        let other = Config {
            fights: 200,
            seed: [7; 32],
        };
        let second = run(&other, &roster, &v1()).unwrap();
        assert_ne!(second, first);
        assert_ne!(second.seed_hex, first.seed_hex);
    }

    #[test]
    fn identical_specs_split_the_wins_exactly() {
        // Each seed is played in both orientations and the slug does not enter the simulation.
        let roster = [spec("aa-one", 60, 60), spec("bb-two", 60, 60)];
        let config = Config {
            fights: 100,
            seed: DEFAULT_SEED,
        };
        let report = run(&config, &roster, &v1()).unwrap();
        let [one, two] = report.per_character.as_slice() else {
            panic!("two characters");
        };
        assert_eq!(one.wins, two.wins);
        assert_eq!(one.draws, two.draws);
    }

    #[test]
    fn run_errors() {
        let ruleset = v1();
        let config = Config {
            fights: 10,
            seed: DEFAULT_SEED,
        };
        assert_eq!(
            run(&config, &[spec("aa-one", 60, 60)], &ruleset),
            Err(SimError::RosterTooSmall(1))
        );
        assert_eq!(
            run(
                &config,
                &[spec("aa-one", 60, 60), spec("aa-one", 60, 60)],
                &ruleset
            ),
            Err(SimError::DuplicateSlug("aa-one".to_owned()))
        );
        assert_eq!(
            run(
                &config,
                &[spec("aa-one", 61, 60), spec("bb-two", 60, 60)],
                &ruleset
            ),
            Err(SimError::Spec {
                slug: "aa-one".to_owned(),
                source: SpecError::BudgetExceeded
            })
        );
        let few = Config {
            fights: 1,
            seed: DEFAULT_SEED,
        };
        assert_eq!(
            run(
                &few,
                &[spec("aa-one", 60, 60), spec("bb-two", 60, 60)],
                &ruleset
            ),
            Err(SimError::TooFewFights {
                fights: 1,
                pairs: 1
            })
        );
    }

    #[test]
    fn codes_lead_the_display() {
        let all = [
            SimError::RosterTooSmall(1),
            SimError::TooFewFights {
                fights: 1,
                pairs: 1,
            },
            SimError::DuplicateSlug("x".to_owned()),
            SimError::Spec {
                slug: "x".to_owned(),
                source: SpecError::BudgetExceeded,
            },
            SimError::Encode,
            SimError::LogWithoutEnd,
        ];
        for error in all {
            assert!(error.to_string().starts_with(error.code()), "{error}");
        }
    }
}
