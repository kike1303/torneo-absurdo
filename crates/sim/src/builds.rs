//! Balance of open player builds (016 phase 3): `just sim --builds N`.
//!
//! [`run_builds`] draws `N` random valid loadouts from the player catalog at one level, plays
//! `fights` random pairs of them with ruleset v2, and tallies the win rate of every catalog piece
//! (ability, accessory, style) and of every mix of kinds, plus the RF17 measures of duration,
//! `time_up` and moments. A build fills every ability slot open at its level that the player can
//! fill (the starter plus one ability per draw so far), as a player does: an empty slot only makes
//! a build weaker. When the level is above 10 it also plays builds of that level against builds 10
//! levels lower, which tells the PvP matchmaking how much a gap of 10 levels weighs. Then it looks for
//! dominant builds: a hill climb from some builds, changing one piece at a time, scored against
//! a fixed sample of opponents with fixed seeds; the climbed builds then face each other, and
//! the best of them against the rest is the dominant one. Its win rate is measured again with
//! fresh seeds, so picking the luckiest of many noisy scores does not inflate it. Everything comes from one
//! `ChaCha20Rng` seeded with the root seed, so the same catalog, level and seed give the same
//! report. The root seed is the simulator's own, never the one of a real fight (ADR-0002).
use std::collections::BTreeMap;
use std::fmt::Write as _;

use content::Catalog;
use engine::loadout::{self, Accessory, Loadout, Slot};
use engine::{
    Ability, CharacterSpec, EndReason, FightEvent, FightLog, MomentKind, MoveKind, Ruleset, Seed,
    Side, simulate,
};
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use serde::Serialize;

use crate::args::seed_hex;
use crate::report::DURATION_BAND_MS;
use crate::{Criteria, SimError};

/// The ruleset of the player botargas.
pub const BUILDS_RULESET_VERSION: u16 = 2;

/// Opponents a climbing build is scored against, each in both orientations.
pub const CLIMB_OPPONENTS: u32 = 40;
/// Steps of each climb on the command line.
pub const CLIMB_STEPS: u32 = 30;
/// Most climbs: one every 10 builds, up to this.
pub const CLIMBS_MAX: u32 = 200;
/// Rounds of fresh seeds for the final measure of the dominant build.
pub const DOMINANT_ROUNDS: u32 = 4;
/// Attempts to draw one valid build before giving up.
const DRAW_ATTEMPTS: u32 = 10_000;

/// What to simulate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildsConfig {
    pub builds: u32,
    pub fights: u32,
    pub level: u8,
    pub seed: [u8; 32],
    /// Hill climbs (at most `builds`) and steps of each.
    pub climbs: u32,
    pub climb_steps: u32,
    /// Fights between builds of `level` and builds of `level - LEVEL_GAP` (0 to skip).
    pub gap_fights: u32,
}

/// The level gap the report measures.
pub const LEVEL_GAP: u8 = 10;

impl BuildsConfig {
    /// The command line's: one climb every 10 builds (from 1 to [`CLIMBS_MAX`]) of
    /// [`CLIMB_STEPS`] steps.
    #[must_use]
    pub fn new(builds: u32, fights: u32, level: u8, seed: [u8; 32]) -> Self {
        Self {
            builds,
            fights,
            level,
            seed,
            climbs: (builds / 10).clamp(1, CLIMBS_MAX),
            climb_steps: CLIMB_STEPS,
            gap_fights: if level > LEVEL_GAP { fights / 10 } else { 0 },
        }
    }
}

/// Thresholds of the builds report, in ‰.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BuildsCriteria {
    /// Win rate of the builds that carry a piece, a slot count or a mix (draws count half).
    pub group_min_permille: u32,
    pub group_max_permille: u32,
    /// Groups with fewer appearances are reported but not judged.
    pub group_min_appearances: u64,
    /// Best climbed build against the other climbed builds.
    pub dominant_max_permille: u32,
    /// Fights with `time_up`.
    pub time_up_max_permille: u32,
    /// Fights with `end.t` inside the duration band of RF17.
    pub in_band_min_permille: u32,
    /// Fights with at least one `moment`, and with one of each kind (RF17).
    pub any_moment_min_permille: u32,
    pub moment_kind_min_permille: u32,
}

impl BuildsCriteria {
    /// 016, "`just sim` con builds abiertas".
    pub const FEATURE_016: Self = Self {
        group_min_permille: 450,
        group_max_permille: 550,
        group_min_appearances: 200,
        dominant_max_permille: 650,
        time_up_max_permille: Criteria::RF17.time_up_max_permille,
        in_band_min_permille: Criteria::RF17.in_band_min_permille,
        any_moment_min_permille: Criteria::RF17.any_moment_min_permille,
        moment_kind_min_permille: Criteria::RF17.moment_kind_min_permille,
    };
}

/// What a tallied group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    Ability,
    Accessory,
    Style,
    /// Heavies and specials, as `"<h>h<s>s"`.
    Mix,
}

impl GroupKind {
    const fn code(self) -> &'static str {
        match self {
            Self::Ability => "ability",
            Self::Accessory => "accessory",
            Self::Style => "style",
            Self::Mix => "mix",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Group {
    pub kind: GroupKind,
    pub id: String,
    /// Fighters (one per side of a fight) whose build is in the group.
    pub appearances: u64,
    /// Half points: 2 per win, 1 per draw.
    pub points: u64,
    pub win_permille: u32,
}

/// Builds of the report's level against builds `LEVEL_GAP` levels lower (informative).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LevelGap {
    pub lower_level: u8,
    pub fights: u32,
    /// Win rate of the higher level (draws count half).
    pub higher_win_permille: u32,
}

/// Fights with each `moment` kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MomentCounts {
    pub any: u64,
    pub comeback: u64,
    pub decisive_crit: u64,
    pub clutch_ko: u64,
}

/// The climbed build that does best against the other climbed builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dominant {
    pub win_permille: u32,
    /// Half points and the most it could get, for the exact comparison.
    pub points: u64,
    pub total: u64,
    pub abilities: Vec<String>,
    pub accessories: Vec<String>,
    pub style: String,
    pub stats: [u8; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildsReport {
    pub ruleset_version: u16,
    pub seed_hex: String,
    pub level: u8,
    pub builds: u32,
    pub fights: u32,
    pub climbs: u32,
    pub climb_steps: u32,
    pub time_up: u64,
    pub in_band: u64,
    pub draws: u64,
    pub moments: MomentCounts,
    pub level_gap: Option<LevelGap>,
    /// Sorted by kind, then id.
    pub groups: Vec<Group>,
    pub dominant: Dominant,
}

/// A criterion the report misses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum BuildsFailure {
    GroupWinRate {
        kind: GroupKind,
        id: String,
        permille: u32,
    },
    Dominant {
        permille: u32,
    },
    TimeUp {
        permille: u32,
    },
    InBand {
        permille: u32,
    },
    Moments {
        kind: &'static str,
        permille: u32,
    },
}

impl BuildsReport {
    #[must_use]
    pub fn failures(&self, c: &BuildsCriteria) -> Vec<BuildsFailure> {
        let mut out = Vec::new();
        let total = u64::from(self.fights);
        for g in &self.groups {
            let half = g.appearances * 2;
            if g.appearances >= c.group_min_appearances
                && !(at_least(g.points, half, c.group_min_permille)
                    && at_most(g.points, half, c.group_max_permille))
            {
                out.push(BuildsFailure::GroupWinRate {
                    kind: g.kind,
                    id: g.id.clone(),
                    permille: g.win_permille,
                });
            }
        }
        if !at_most(
            self.dominant.points,
            self.dominant.total,
            c.dominant_max_permille,
        ) {
            out.push(BuildsFailure::Dominant {
                permille: self.dominant.win_permille,
            });
        }
        if !at_most(self.time_up, total, c.time_up_max_permille) {
            out.push(BuildsFailure::TimeUp {
                permille: permille(self.time_up, total),
            });
        }
        if !at_least(self.in_band, total, c.in_band_min_permille) {
            out.push(BuildsFailure::InBand {
                permille: permille(self.in_band, total),
            });
        }
        let m = self.moments;
        for (kind, count, min) in [
            ("any", m.any, c.any_moment_min_permille),
            ("comeback", m.comeback, c.moment_kind_min_permille),
            ("decisive_crit", m.decisive_crit, c.moment_kind_min_permille),
            ("clutch_ko", m.clutch_ko, c.moment_kind_min_permille),
        ] {
            if !at_least(count, total, min) {
                out.push(BuildsFailure::Moments {
                    kind,
                    permille: permille(count, total),
                });
            }
        }
        out
    }

    #[must_use]
    pub fn render_text(&self, c: &BuildsCriteria) -> String {
        let mut s = String::new();
        let pct = |p: u32| format!("{}.{}%", p / 10, p % 10);
        let _ = writeln!(
            s,
            "sim --builds: ruleset v{}, root seed {}",
            self.ruleset_version, self.seed_hex
        );
        let _ = writeln!(
            s,
            "{} builds at level {}, {} fights, {} climbs of {} steps",
            self.builds, self.level, self.fights, self.climbs, self.climb_steps
        );
        let total = u64::from(self.fights);
        let share = |count: u64| pct(permille(count, total));
        let _ = writeln!(
            s,
            "time_up {} (max {}), in band {} (min {}), draws {}",
            share(self.time_up),
            pct(c.time_up_max_permille),
            share(self.in_band),
            pct(c.in_band_min_permille),
            share(self.draws)
        );
        let m = self.moments;
        let _ = writeln!(
            s,
            "moments: any {} (min {}), comeback {}, decisive_crit {}, clutch_ko {} (min {} each)",
            share(m.any),
            pct(c.any_moment_min_permille),
            share(m.comeback),
            share(m.decisive_crit),
            share(m.clutch_ko),
            pct(c.moment_kind_min_permille)
        );
        if let Some(gap) = &self.level_gap {
            let _ = writeln!(
                s,
                "level {} against level {}: {} over {} fights (informative)",
                self.level,
                gap.lower_level,
                pct(gap.higher_win_permille),
                gap.fights
            );
        }
        let _ = writeln!(
            s,
            "\nwin rate by group ({}-{}, groups under {} appearances not judged):",
            pct(c.group_min_permille),
            pct(c.group_max_permille),
            c.group_min_appearances
        );
        let width = self.groups.iter().map(|g| g.id.len()).max().unwrap_or(0);
        for g in &self.groups {
            let judged = g.appearances >= c.group_min_appearances;
            let half = g.appearances * 2;
            let ok = at_least(g.points, half, c.group_min_permille)
                && at_most(g.points, half, c.group_max_permille);
            let mark = match (judged, ok) {
                (false, _) => "  -",
                (true, true) => " ok",
                (true, false) => "BAD",
            };
            let _ = writeln!(
                s,
                "  {mark} {:<9} {:<width$} {:>6}  ({} appearances)",
                g.kind.code(),
                g.id,
                pct(g.win_permille),
                g.appearances
            );
        }
        let d = &self.dominant;
        let _ = writeln!(
            s,
            "\nbest climbed build: {} (max {}): {} | {} | {} | stats {:?}",
            pct(d.win_permille),
            pct(c.dominant_max_permille),
            d.abilities.join(", "),
            if d.accessories.is_empty() {
                "no accessories".to_owned()
            } else {
                d.accessories.join(", ")
            },
            d.style,
            d.stats
        );
        let failures = self.failures(c);
        let _ = writeln!(s, "\n{} failure(s)", failures.len());
        s
    }
}

/// One build: the loadout, its style id and its resolved spec.
#[derive(Debug, Clone)]
struct Build {
    loadout: Loadout,
    style: String,
    spec: CharacterSpec,
}

/// Runs the builds report.
///
/// # Errors
/// [`SimError::BuildsTooFew`] with fewer than 2 builds, no fights, no climbs or more climbs than
/// builds, or a level outside `1..=LEVEL_MAX`, [`SimError::BuildsDraw`] if the catalog cannot produce a valid build, and the
/// errors of the engine.
pub fn run_builds(
    config: &BuildsConfig,
    catalog: &Catalog,
    ruleset: &Ruleset,
) -> Result<BuildsReport, SimError> {
    if config.builds < 2
        || loadout::tier(config.level).is_none()
        || config.fights == 0
        || config.climbs == 0
        || config.climbs > config.builds
    {
        return Err(SimError::BuildsTooFew);
    }
    let mut rng = ChaCha20Rng::from_seed(config.seed);
    let mut builds = Vec::new();
    for i in 0..config.builds {
        builds.push(draw_build(&mut rng, catalog, config.level, i, ruleset)?);
    }

    let mut tally: BTreeMap<(GroupKind, String), (u64, u64)> = BTreeMap::new();
    let (mut time_up, mut in_band, mut draws) = (0_u64, 0_u64, 0_u64);
    let mut moments = MomentCounts::default();
    for _ in 0..config.fights {
        let a = pick(&mut rng, config.builds);
        let b = (a + 1 + pick(&mut rng, config.builds - 1)) % config.builds;
        let seed = next_seed(&mut rng);
        let (ba, bb) = (build(&builds, a)?, build(&builds, b)?);
        let log = fight(seed, &ba.spec, &bb.spec, ruleset)?;
        let (winner, reason) = log.outcome().ok_or(SimError::LogWithoutEnd)?;
        if log
            .events
            .iter()
            .any(|e| matches!(e, FightEvent::TimeUp { .. }))
        {
            time_up += 1;
        }
        let end = log.events.last().map_or(0, FightEvent::t);
        if (DURATION_BAND_MS.0..=DURATION_BAND_MS.1).contains(&end) {
            in_band += 1;
        }
        if reason == EndReason::Draw {
            draws += 1;
        }
        count_moments(&log, &mut moments);
        for (side, fighter) in [(Side::A, ba), (Side::B, bb)] {
            let points = match winner {
                Some(w) if w == side => 2,
                Some(_) => 0,
                None => 1,
            };
            for key in groups_of(fighter) {
                let entry = tally.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += points;
            }
        }
    }
    let groups = tally
        .into_iter()
        .map(|((kind, id), (appearances, points))| Group {
            kind,
            id,
            appearances,
            points,
            win_permille: permille(points, appearances * 2),
        })
        .collect();

    let climbs = config.climbs;
    let opponents: Vec<u32> = (0..CLIMB_OPPONENTS.min(config.builds))
        .map(|_| pick(&mut rng, config.builds))
        .collect();
    let seeds: Vec<[u8; 32]> = opponents.iter().map(|_| next_seed(&mut rng)).collect();
    let opponent_specs: Vec<&CharacterSpec> = opponents
        .iter()
        .map(|&o| build(&builds, o).map(|b| &b.spec))
        .collect::<Result<_, _>>()?;
    let mut climbed = Vec::new();
    for c in 0..climbs {
        let mut current = build(&builds, c % config.builds)?.clone();
        let mut score = evaluate(&current.spec, &opponent_specs, &seeds, ruleset)?;
        for _ in 0..config.climb_steps {
            let Some(candidate) = mutate(&mut rng, &current, catalog, config.level, ruleset) else {
                continue;
            };
            let s = evaluate(&candidate.spec, &opponent_specs, &seeds, ruleset)?;
            if s > score {
                score = s;
                current = candidate;
            }
        }
        climbed.push(current);
    }
    // The climbed builds against each other, with one fixed seed per opponent position.
    let seeds: Vec<[u8; 32]> = climbed.iter().map(|_| next_seed(&mut rng)).collect();
    let mut best: Option<(u32, &Build)> = None;
    for (i, candidate) in climbed.iter().enumerate() {
        let others: Vec<&CharacterSpec> = climbed
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, b)| &b.spec)
            .collect();
        let score = if others.is_empty() {
            500
        } else {
            evaluate(&candidate.spec, &others, &seeds, ruleset)?
        };
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, candidate));
        }
    }
    let (_, top) = best.ok_or(SimError::BuildsTooFew)?;
    let top = top.clone();
    let others: Vec<&CharacterSpec> = climbed
        .iter()
        .filter(|b| b.loadout.slug != top.loadout.slug)
        .map(|b| &b.spec)
        .collect();
    let (mut points, mut total) = (0_u64, 0_u64);
    if others.is_empty() {
        (points, total) = (1, 2);
    } else {
        for _ in 0..DOMINANT_ROUNDS {
            let fresh: Vec<[u8; 32]> = others.iter().map(|_| next_seed(&mut rng)).collect();
            let (p, t) = evaluate_points(&top.spec, &others, &fresh, ruleset)?;
            points += p;
            total += t;
        }
    }
    let win_permille = permille(points, total);
    let s = top.spec.stats;
    let dominant = Dominant {
        win_permille,
        points,
        total,
        abilities: top.loadout.abilities.iter().map(|a| a.id.clone()).collect(),
        accessories: top
            .loadout
            .accessories
            .iter()
            .map(|a| a.id.clone())
            .collect(),
        style: top.style,
        stats: [s.hp, s.attack, s.defense, s.speed, s.special],
    };

    let level_gap = if config.gap_fights > 0 && config.level > LEVEL_GAP {
        let lower_level = config.level - LEVEL_GAP;
        let mut lower = Vec::new();
        for i in 0..config.builds {
            lower.push(draw_build(&mut rng, catalog, lower_level, i, ruleset)?);
        }
        let mut points = 0_u64;
        for k in 0..config.gap_fights {
            let high = &build(&builds, pick(&mut rng, config.builds))?.spec;
            let low = &build(&lower, pick(&mut rng, config.builds))?.spec;
            let seed = next_seed(&mut rng);
            // Alternate sides, so side `a` does not lean the result.
            let (a, b, side) = if k % 2 == 0 {
                (high, low, Side::A)
            } else {
                (low, high, Side::B)
            };
            let log = fight(seed, a, b, ruleset)?;
            points += match log.outcome().ok_or(SimError::LogWithoutEnd)?.0 {
                Some(w) if w == side => 2,
                Some(_) => 0,
                None => 1,
            };
        }
        Some(LevelGap {
            lower_level,
            fights: config.gap_fights,
            higher_win_permille: permille(points, u64::from(config.gap_fights) * 2),
        })
    } else {
        None
    };

    Ok(BuildsReport {
        ruleset_version: ruleset.version(),
        seed_hex: seed_hex(&config.seed),
        level: config.level,
        builds: config.builds,
        fights: config.fights,
        climbs,
        climb_steps: config.climb_steps,
        time_up,
        in_band,
        draws,
        moments,
        level_gap,
        groups,
        dominant,
    })
}

fn build(builds: &[Build], i: u32) -> Result<&Build, SimError> {
    builds
        .get(usize::try_from(i).map_err(|_| SimError::BuildsTooFew)?)
        .ok_or(SimError::BuildsTooFew)
}

fn groups_of(b: &Build) -> Vec<(GroupKind, String)> {
    let mut out: Vec<(GroupKind, String)> = b
        .loadout
        .abilities
        .iter()
        .map(|a| (GroupKind::Ability, a.id.clone()))
        .chain(
            b.loadout
                .accessories
                .iter()
                .map(|a| (GroupKind::Accessory, a.id.clone())),
        )
        .collect();
    out.push((GroupKind::Style, b.style.clone()));
    let n = b.loadout.abilities.len();
    let heavies = b
        .loadout
        .abilities
        .iter()
        .filter(|a| a.kind == MoveKind::Heavy)
        .count();

    out.push((GroupKind::Mix, format!("{heavies}h{}s", n - heavies)));
    out
}

/// Win share (‰, draws half) of `candidate` against every opponent, in both orientations.
fn evaluate(
    candidate: &CharacterSpec,
    opponents: &[&CharacterSpec],
    seeds: &[[u8; 32]],
    ruleset: &Ruleset,
) -> Result<u32, SimError> {
    let (points, total) = evaluate_points(candidate, opponents, seeds, ruleset)?;
    Ok(permille(points, total))
}

/// Half points of `candidate` and the most it could get, against every opponent in both
/// orientations.
fn evaluate_points(
    candidate: &CharacterSpec,
    opponents: &[&CharacterSpec],
    seeds: &[[u8; 32]],
    ruleset: &Ruleset,
) -> Result<(u64, u64), SimError> {
    let (mut points, mut total) = (0_u64, 0_u64);
    for (&other, &seed) in opponents.iter().zip(seeds) {
        for (a, b, side) in [(candidate, other, Side::A), (other, candidate, Side::B)] {
            let log = fight(seed, a, b, ruleset)?;
            points += match log.outcome().ok_or(SimError::LogWithoutEnd)?.0 {
                Some(w) if w == side => 2,
                Some(_) => 0,
                None => 1,
            };
            total += 2;
        }
    }
    Ok((points, total))
}

/// A uniform value in `0..n` by rejection sampling (`0` when `n <= 1`).
fn pick(rng: &mut ChaCha20Rng, n: u32) -> u32 {
    if n <= 1 {
        return 0;
    }
    let zone = u32::MAX - (u32::MAX % n);
    loop {
        let x = rng.next_u32();
        if x < zone {
            return x % n;
        }
    }
}

fn pick_from<'a, T>(rng: &mut ChaCha20Rng, items: &'a [T]) -> Option<&'a T> {
    let n = u32::try_from(items.len()).ok()?;
    if n == 0 {
        return None;
    }
    items.get(usize::try_from(pick(rng, n)).ok()?)
}

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
) -> Result<FightLog, SimError> {
    simulate(&Seed::from_bytes(seed), a, b, ruleset).map_err(|source| SimError::Spec {
        slug: a.slug.clone(),
        source,
    })
}

fn count_moments(log: &FightLog, counts: &mut MomentCounts) {
    let (mut comeback, mut crit, mut clutch) = (false, false, false);
    for event in &log.events {
        if let FightEvent::Moment { kind, .. } = event {
            match kind {
                MomentKind::Comeback => comeback = true,
                MomentKind::DecisiveCrit => crit = true,
                MomentKind::ClutchKo => clutch = true,
            }
        }
    }
    counts.any += u64::from(comeback || crit || clutch);
    counts.comeback += u64::from(comeback);
    counts.decisive_crit += u64::from(crit);
    counts.clutch_ko += u64::from(clutch);
}

/// `count / total >= min ‰`, exactly.
fn at_least(count: u64, total: u64, min_permille: u32) -> bool {
    count * 1000 >= u64::from(min_permille) * total
}

/// `count / total <= max ‰`, exactly.
fn at_most(count: u64, total: u64, max_permille: u32) -> bool {
    count * 1000 <= u64::from(max_permille) * total
}

fn permille(count: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }
    u32::try_from(count * 1000 / total).unwrap_or(u32::MAX)
}

const SLOTS: [Slot; 5] = [Slot::Head, Slot::Hands, Slot::Torso, Slot::Legs, Slot::Feet];
const STAT_STEP: u8 = 5;

/// The abilities of a kind that a level reaches.
fn abilities_at(catalog: &Catalog, level: u8, kind: MoveKind) -> Vec<Ability> {
    let tier = loadout::tier(level).unwrap_or(0);
    catalog
        .abilities
        .iter()
        .filter(|a| a.tier <= tier && a.ability.kind == kind)
        .map(|a| a.ability.clone())
        .collect()
}

fn accessories_at(catalog: &Catalog, level: u8, slot: Slot) -> Vec<Accessory> {
    let tier = loadout::tier(level).unwrap_or(0);
    catalog
        .accessories
        .iter()
        .filter(|a| a.tier <= tier && a.accessory.slot == slot)
        .map(|a| a.accessory.clone())
        .collect()
}

/// `stat_min` in every stat, then the rest of the level budget in steps of [`STAT_STEP`] to
/// random stats below `stat_max`.
fn random_stats(rng: &mut ChaCha20Rng, level: u8, ruleset: &Ruleset) -> [u8; 5] {
    let mut stats = [ruleset.stat_min(); 5];
    let budget = loadout::budget(level).unwrap_or(0);
    let mut left = budget.saturating_sub(5 * u16::from(ruleset.stat_min()));
    while left > 0 {
        let step = u8::try_from(left.min(u16::from(STAT_STEP))).unwrap_or(STAT_STEP);
        let open: Vec<usize> = (0..5)
            .filter(|&i| {
                stats
                    .get(i)
                    .is_some_and(|&s| s.saturating_add(step) <= ruleset.stat_max())
            })
            .collect();
        let Some(&i) = pick_from(rng, &open) else {
            break;
        };
        if let Some(s) = stats.get_mut(i) {
            *s += step;
        }
        left -= u16::from(step);
    }
    stats
}

fn draw_build(
    rng: &mut ChaCha20Rng,
    catalog: &Catalog,
    level: u8,
    index: u32,
    ruleset: &Ruleset,
) -> Result<Build, SimError> {
    let heavies = abilities_at(catalog, level, MoveKind::Heavy);
    let specials = abilities_at(catalog, level, MoveKind::Special);
    // The open slots the player can fill: the starter plus one ability per draw so far.
    let filled = loadout::slots(level).min(1 + loadout::draws(level));
    let slots = u32::try_from(filled).unwrap_or(0);
    for _ in 0..DRAW_ATTEMPTS {
        let n = slots.max(1);
        // Heavies h with h <= 2 and n - h <= 2.
        let low = n.saturating_sub(2);
        let high = n.min(2);
        let h = low + pick(rng, high - low + 1);
        let mut abilities = if loadout::draws(level) == 0 {
            // Before the first draw the player only owns the starter.
            catalog
                .starter()
                .map(|s| vec![s.ability.clone()])
                .unwrap_or_default()
        } else {
            choose(rng, &heavies, h)
        };
        if loadout::draws(level) > 0 {
            abilities.extend(choose(rng, &specials, n - h));
        }
        if abilities.len() != usize::try_from(n).unwrap_or(0) {
            continue;
        }
        let mut accessories = Vec::new();
        for slot in SLOTS {
            if pick(rng, 2) == 0 {
                continue;
            }
            if let Some(a) = pick_from(rng, &accessories_at(catalog, level, slot)) {
                accessories.push(a.clone());
            }
        }
        let Some(style) = pick_from(rng, &catalog.styles) else {
            return Err(SimError::BuildsDraw);
        };
        let [hp, attack, defense, speed, special] = random_stats(rng, level, ruleset);
        let candidate = Loadout {
            slug: format!("build-{index}"),
            level,
            stats: engine::Stats {
                hp,
                attack,
                defense,
                speed,
                special,
            },
            personality: style.personality,
            abilities,
            accessories,
        };
        if let Ok(spec) = loadout::resolve(&candidate, ruleset) {
            return Ok(Build {
                loadout: candidate,
                style: style.id.clone(),
                spec,
            });
        }
    }
    Err(SimError::BuildsDraw)
}

/// `k` distinct items, in the order drawn.
fn choose(rng: &mut ChaCha20Rng, items: &[Ability], k: u32) -> Vec<Ability> {
    let mut pool: Vec<Ability> = items.to_vec();
    let mut out = Vec::new();
    for _ in 0..k {
        let Ok(n) = u32::try_from(pool.len()) else {
            break;
        };
        if n == 0 {
            break;
        }
        let i = usize::try_from(pick(rng, n)).unwrap_or(0);
        if i < pool.len() {
            out.push(pool.swap_remove(i));
        }
    }
    out
}

/// One piece changed: an ability for another of its kind, an accessory slot, the style, or
/// [`STAT_STEP`] points moved between two stats. `None` when the change is not valid.
fn mutate(
    rng: &mut ChaCha20Rng,
    current: &Build,
    catalog: &Catalog,
    level: u8,
    ruleset: &Ruleset,
) -> Option<Build> {
    let mut next = current.loadout.clone();
    let mut style = current.style.clone();
    match pick(rng, 4) {
        0 => {
            // Before the first draw the starter is the only ability.
            if loadout::draws(level) == 0 {
                return None;
            }
            let n = u32::try_from(next.abilities.len()).ok()?;
            let i = usize::try_from(pick(rng, n)).ok()?;
            let kind = next.abilities.get(i)?.kind;
            let options: Vec<Ability> = abilities_at(catalog, level, kind)
                .into_iter()
                .filter(|a| next.abilities.iter().all(|o| o.id != a.id))
                .collect();
            *next.abilities.get_mut(i)? = pick_from(rng, &options)?.clone();
        }
        1 => {
            let slot = *pick_from(rng, &SLOTS)?;
            next.accessories.retain(|a| a.slot != slot);
            let mut options: Vec<Option<Accessory>> = vec![None];
            options.extend(accessories_at(catalog, level, slot).into_iter().map(Some));
            if let Some(a) = pick_from(rng, &options)?.clone() {
                next.accessories.push(a);
            }
        }
        2 => {
            let s = pick_from(rng, &catalog.styles)?;
            next.personality = s.personality;
            style.clone_from(&s.id);
        }
        _ => {
            let mut stats = [
                next.stats.hp,
                next.stats.attack,
                next.stats.defense,
                next.stats.speed,
                next.stats.special,
            ];
            let from = usize::try_from(pick(rng, 5)).ok()?;
            let to = usize::try_from(pick(rng, 5)).ok()?;
            *stats.get_mut(from)? = stats.get(from)?.checked_sub(STAT_STEP)?;
            *stats.get_mut(to)? = stats.get(to)?.checked_add(STAT_STEP)?;
            let [hp, attack, defense, speed, special] = stats;
            next.stats = engine::Stats {
                hp,
                attack,
                defense,
                speed,
                special,
            };
        }
    }
    let spec = loadout::resolve(&next, ruleset).ok()?;
    Some(Build {
        loadout: next,
        style,
        spec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/catalog");
        content::load_catalog(&root, &v2()).unwrap()
    }

    fn v2() -> Ruleset {
        Ruleset::for_version(BUILDS_RULESET_VERSION).unwrap()
    }

    fn small(level: u8) -> BuildsConfig {
        BuildsConfig {
            builds: 40,
            fights: 200,
            level,
            seed: crate::DEFAULT_SEED,
            climbs: 2,
            climb_steps: 3,
            gap_fights: 50,
        }
    }

    #[test]
    fn the_same_config_gives_the_same_report() {
        let (catalog, r) = (catalog(), v2());
        let first = run_builds(&small(100), &catalog, &r).unwrap();
        assert_eq!(first, run_builds(&small(100), &catalog, &r).unwrap());
        let mut other = small(100);
        other.seed[0] ^= 1;
        assert_ne!(first, run_builds(&other, &catalog, &r).unwrap());
    }

    #[test]
    fn builds_fill_the_open_slots_of_their_level_and_are_valid() {
        let (catalog, r) = (catalog(), v2());
        let mut rng = ChaCha20Rng::from_seed(crate::DEFAULT_SEED);
        for level in [1, 5, 15, 30, 100] {
            for i in 0..50 {
                let b = draw_build(&mut rng, &catalog, level, i, &r).unwrap();
                assert_eq!(
                    b.loadout.abilities.len(),
                    loadout::slots(level).min(1 + loadout::draws(level)),
                    "level {level}"
                );
                assert_eq!(loadout::resolve(&b.loadout, &r).unwrap(), b.spec);
                let tier = loadout::tier(level).unwrap();
                for a in &b.loadout.abilities {
                    let entry = catalog
                        .abilities
                        .iter()
                        .find(|c| c.ability.id == a.id)
                        .unwrap();
                    assert!(entry.tier <= tier, "{} at level {level}", a.id);
                }
            }
        }
    }

    #[test]
    fn mutations_keep_builds_valid() {
        let (catalog, r) = (catalog(), v2());
        let mut rng = ChaCha20Rng::from_seed(crate::DEFAULT_SEED);
        let mut current = draw_build(&mut rng, &catalog, 100, 0, &r).unwrap();
        let mut changed = 0;
        for _ in 0..200 {
            if let Some(next) = mutate(&mut rng, &current, &catalog, 100, &r) {
                assert_eq!(loadout::resolve(&next.loadout, &r).unwrap(), next.spec);
                assert_eq!(next.loadout.abilities.len(), 4);
                current = next;
                changed += 1;
            }
        }
        assert!(changed > 100, "{changed}");
    }

    #[test]
    fn invalid_configs() {
        let (catalog, r) = (catalog(), v2());
        for change in [
            (|c: &mut BuildsConfig| c.builds = 1) as fn(&mut BuildsConfig),
            |c| c.fights = 0,
            |c| c.level = 0,
            |c| c.level = 101,
            |c| c.climbs = 0,
            |c| c.climbs = 41,
        ] {
            let mut config = small(100);
            change(&mut config);
            assert_eq!(
                run_builds(&config, &catalog, &r),
                Err(SimError::BuildsTooFew),
                "{config:?}"
            );
        }
    }

    #[test]
    fn pick_is_in_range_and_covers_it() {
        let mut rng = ChaCha20Rng::from_seed(crate::DEFAULT_SEED);
        let mut seen = [false; 7];
        for _ in 0..500 {
            let x = pick(&mut rng, 7);
            assert!(x < 7);
            seen[usize::try_from(x).unwrap()] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert_eq!(pick(&mut rng, 1), 0);
        assert_eq!(pick(&mut rng, 0), 0);
    }

    #[test]
    fn failures_follow_the_criteria() {
        let report = run_builds(&small(100), &catalog(), &v2()).unwrap();
        let strict = BuildsCriteria {
            group_min_permille: 1000,
            group_max_permille: 1000,
            group_min_appearances: 0,
            dominant_max_permille: 0,
            time_up_max_permille: 0,
            in_band_min_permille: 1001,
            any_moment_min_permille: 1001,
            moment_kind_min_permille: 1001,
        };
        let failures = report.failures(&strict);
        assert!(
            failures
                .iter()
                .any(|f| matches!(f, BuildsFailure::Dominant { .. }))
        );
        assert!(
            failures
                .iter()
                .any(|f| matches!(f, BuildsFailure::InBand { .. }))
        );
        assert!(
            failures
                .iter()
                .any(|f| matches!(f, BuildsFailure::GroupWinRate { .. }))
        );
        assert!(
            failures
                .iter()
                .any(|f| matches!(f, BuildsFailure::Moments { .. }))
        );
        let lax = BuildsCriteria {
            group_min_permille: 0,
            group_max_permille: 1000,
            group_min_appearances: 0,
            dominant_max_permille: 1000,
            time_up_max_permille: 1000,
            in_band_min_permille: 0,
            any_moment_min_permille: 0,
            moment_kind_min_permille: 0,
        };
        assert!(report.failures(&lax).is_empty());
        assert!(report.render_text(&lax).contains("0 failure(s)"));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn exact_comparisons_do_not_truncate() {
        // 1009 of 10 000 is 100.9 ‰: above a 100 ‰ cap, though it truncates to 100.
        assert!(!at_most(1009, 10_000, 100));
        assert!(at_most(1000, 10_000, 100));
        assert!(at_least(4500, 10_000, 450));
        assert!(!at_least(4499, 10_000, 450));
    }

    #[test]
    fn the_level_gap_is_measured_above_level_10_only() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/catalog");
        let r = Ruleset::for_version(BUILDS_RULESET_VERSION).unwrap();
        let catalog = content::load_catalog(&root, &r).unwrap();
        let config = BuildsConfig::new(40, 200, 30, crate::DEFAULT_SEED);
        assert_eq!(config.gap_fights, 20);
        let report = run_builds(&config, &catalog, &r).unwrap();
        let gap = report.level_gap.unwrap();
        assert_eq!((gap.lower_level, gap.fights), (20, 20));
        assert_eq!(
            BuildsConfig::new(40, 200, 10, crate::DEFAULT_SEED).gap_fights,
            0
        );
    }
}
