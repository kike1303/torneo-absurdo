//! Balance of open player builds (016 phase 3): `just sim --builds N`.
//!
//! [`run_builds`] draws `N` random valid loadouts from the player catalog at one level, plays
//! `fights` random pairs of them with ruleset v2, and tallies the win rate of every catalog piece
//! (ability, accessory, style) and of every mix of kinds. A build fills every ability slot open
//! at its level, as a player does: an empty slot only makes a build weaker. Then it looks for
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
    Ability, CharacterSpec, EndReason, FightEvent, FightLog, MoveKind, Ruleset, Seed, Side,
    simulate,
};
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use serde::Serialize;

use crate::SimError;
use crate::args::seed_hex;
use crate::report::DURATION_BAND_MS;

/// The ruleset of the player botargas.
pub const BUILDS_RULESET_VERSION: u16 = 2;

/// Opponents a climbing build is scored against, each in both orientations.
pub const CLIMB_OPPONENTS: u32 = 40;
/// Steps of each climb.
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
}

impl BuildsConfig {
    /// One climb every 10 builds, at least 1 and at most [`CLIMBS_MAX`].
    #[must_use]
    pub fn climbs(&self) -> u32 {
        (self.builds / 10).clamp(1, CLIMBS_MAX)
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
}

impl BuildsCriteria {
    /// 016, "`just sim` con builds abiertas".
    pub const FEATURE_016: Self = Self {
        group_min_permille: 450,
        group_max_permille: 550,
        group_min_appearances: 200,
        dominant_max_permille: 650,
        time_up_max_permille: 100,
        in_band_min_permille: 900,
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
    pub win_permille: u32,
}

/// The climbed build that does best against the other climbed builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dominant {
    pub win_permille: u32,
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
    pub time_up_permille: u32,
    pub in_band_permille: u32,
    pub draw_permille: u32,
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
}

impl BuildsReport {
    #[must_use]
    pub fn failures(&self, c: &BuildsCriteria) -> Vec<BuildsFailure> {
        let mut out = Vec::new();
        for g in &self.groups {
            if g.appearances >= c.group_min_appearances
                && !(c.group_min_permille..=c.group_max_permille).contains(&g.win_permille)
            {
                out.push(BuildsFailure::GroupWinRate {
                    kind: g.kind,
                    id: g.id.clone(),
                    permille: g.win_permille,
                });
            }
        }
        if self.dominant.win_permille > c.dominant_max_permille {
            out.push(BuildsFailure::Dominant {
                permille: self.dominant.win_permille,
            });
        }
        if self.time_up_permille > c.time_up_max_permille {
            out.push(BuildsFailure::TimeUp {
                permille: self.time_up_permille,
            });
        }
        if self.in_band_permille < c.in_band_min_permille {
            out.push(BuildsFailure::InBand {
                permille: self.in_band_permille,
            });
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
            "{} builds at level {}, {} fights, {} climbs of {CLIMB_STEPS} steps",
            self.builds, self.level, self.fights, self.climbs
        );
        let _ = writeln!(
            s,
            "time_up {} (max {}), in band {} (min {}), draws {}",
            pct(self.time_up_permille),
            pct(c.time_up_max_permille),
            pct(self.in_band_permille),
            pct(c.in_band_min_permille),
            pct(self.draw_permille)
        );
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
            let ok = (c.group_min_permille..=c.group_max_permille).contains(&g.win_permille);
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
/// [`SimError::BuildsTooFew`] with fewer than 2 builds, no fights or a level outside
/// `1..=LEVEL_MAX`, [`SimError::BuildsDraw`] if the catalog cannot produce a valid build, and the
/// errors of the engine.
pub fn run_builds(
    config: &BuildsConfig,
    catalog: &Catalog,
    ruleset: &Ruleset,
) -> Result<BuildsReport, SimError> {
    if config.builds < 2 || loadout::tier(config.level).is_none() || config.fights == 0 {
        return Err(SimError::BuildsTooFew);
    }
    let mut rng = ChaCha20Rng::from_seed(config.seed);
    let mut builds = Vec::new();
    for i in 0..config.builds {
        builds.push(draw_build(&mut rng, catalog, config.level, i, ruleset)?);
    }

    let mut tally: BTreeMap<(GroupKind, String), (u64, u64)> = BTreeMap::new();
    let (mut time_up, mut in_band, mut draws) = (0_u64, 0_u64, 0_u64);
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
            win_permille: permille(points, appearances * 2),
        })
        .collect();

    let climbs = config.climbs();
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
        for _ in 0..CLIMB_STEPS {
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
    let mut win_permille = 500;
    if !others.is_empty() {
        let (mut sum, mut rounds) = (0_u64, 0_u64);
        for _ in 0..DOMINANT_ROUNDS {
            let fresh: Vec<[u8; 32]> = others.iter().map(|_| next_seed(&mut rng)).collect();
            sum += u64::from(evaluate(&top.spec, &others, &fresh, ruleset)?);
            rounds += 1;
        }
        win_permille = u32::try_from(sum / rounds).unwrap_or(u32::MAX);
    }
    let s = top.spec.stats;
    let dominant = Dominant {
        win_permille,
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

    let total = u64::from(config.fights);
    Ok(BuildsReport {
        ruleset_version: ruleset.version(),
        seed_hex: seed_hex(&config.seed),
        level: config.level,
        builds: config.builds,
        fights: config.fights,
        climbs,
        time_up_permille: permille(time_up, total),
        in_band_permille: permille(in_band, total),
        draw_permille: permille(draws, total),
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
    Ok(permille(points, total))
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
    let slots = u32::try_from(loadout::slots(level)).unwrap_or(0);
    for _ in 0..DRAW_ATTEMPTS {
        let n = slots.max(1);
        // Heavies h with h <= 2 and n - h <= 2.
        let low = n.saturating_sub(2);
        let high = n.min(2);
        let h = low + pick(rng, high - low + 1);
        let mut abilities = choose(rng, &heavies, h);
        abilities.extend(choose(rng, &specials, n - h));
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
