//! The balance report (003 RF15) and the criteria of RF17.
//!
//! The report keeps counts only; rates are compared by cross-multiplication and printed in ‰
//! with integer division, so a value on the edge of a threshold is never decided by rounding
//! (plan-b decision 9). Every metric is over the cross fights; the mirrors only measure the
//! side-a bias (decision 8). No `HashMap`: every collection is a `Vec` in roster order.
use std::fmt::Write as _;

use engine::{EndReason, FightEvent, FightLog, MomentKind, Ruleset, Side, TIMELINE_CONTRACT};
use serde::Serialize;

use crate::{Plan, SimError, args::seed_hex};

/// `end.t` band of RF17. It comes from the intent and from 002, so it is not one of the
/// thresholds the game-designer may adjust (it is not in [`Criteria`]).
pub const DURATION_BAND_MS: (u32, u32) = (45_000, 90_000);

/// A separation between consecutive resolutions up to this is a burst (006 RF11). A definition
/// of measure from 002 ("Datos para 003") and the owner, like [`DURATION_BAND_MS`]: not a game
/// constant and not adjusted by the game-designer.
pub const BURST_GAP_MAX_MS: u32 = 900;

/// A separation between consecutive resolutions from this on is a pause (006 RF11). Same rule
/// as [`BURST_GAP_MAX_MS`]; a test ties it to `Ruleset::breather_gap_min()`.
pub const PAUSE_GAP_MIN_MS: u32 = 2500;

const MOMENT_KINDS: [MomentKind; 3] = [
    MomentKind::Comeback,
    MomentKind::DecisiveCrit,
    MomentKind::ClutchKo,
];

/// Thresholds of RF17, in ‰ of the cross fights unless stated otherwise. `--check` and the
/// balance test both use [`Criteria::RF17`]; an adjustment is a change here, recorded as a
/// deviation of plan-b and in the spec (decision 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Criteria {
    /// Global win rate of each character (draws count half), inclusive.
    pub win_rate_min_permille: u32,
    pub win_rate_max_permille: u32,
    /// Fights with at least one `moment`.
    pub any_moment_min_permille: u32,
    /// Fights with `end.t` inside [`DURATION_BAND_MS`].
    pub in_band_min_permille: u32,
    /// Fights with `time_up` (above 100 ‰ also needs the product-owner's OK).
    pub time_up_max_permille: u32,
    /// Draws, strictly below.
    pub draw_below_permille: u32,
    /// Side `a` in the mirrors (draws count half), inclusive.
    pub mirror_side_a_min_permille: u32,
    pub mirror_side_a_max_permille: u32,
    /// Fights with at least one `moment` of each `kind`.
    pub moment_kind_min_permille: u32,
    /// Whether a dead `MomentKind` fails (`true`, RF17 as amended) or only warns.
    pub moment_kind_fails: bool,
    /// A matchup outside this band is a warning, never a failure.
    pub matchup_warn_min_permille: u32,
    pub matchup_warn_max_permille: u32,
    /// Mean separation between resolutions, inclusive (006 RF12; moving it needs the
    /// product-owner's OK).
    pub mean_gap_min_ms: u32,
    pub mean_gap_max_ms: u32,
    /// Separations of at most [`BURST_GAP_MAX_MS`], in ‰ of all separations.
    pub burst_min_permille: u32,
    /// Median of the pauses per fight.
    pub pauses_p50_min: u32,
    /// Fights with at least one pause.
    pub paused_fights_min_permille: u32,
}

impl Criteria {
    pub const RF17: Self = Self {
        win_rate_min_permille: 400,
        win_rate_max_permille: 600,
        any_moment_min_permille: 150,
        in_band_min_permille: 900,
        time_up_max_permille: 100,
        draw_below_permille: 10,
        mirror_side_a_min_permille: 470,
        mirror_side_a_max_permille: 530,
        moment_kind_min_permille: 30,
        moment_kind_fails: true,
        matchup_warn_min_permille: 300,
        matchup_warn_max_permille: 700,
        mean_gap_min_ms: 1100,
        mean_gap_max_ms: 1300,
        burst_min_permille: 250,
        pauses_p50_min: 2,
        paused_fights_min_permille: 900,
    };
}

/// A criterion of RF17 that fails. `permille` is informative (integer division).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Failure {
    WinRate {
        slug: String,
        permille: u32,
    },
    AnyMoment {
        permille: u32,
    },
    DurationInBand {
        permille: u32,
    },
    TimeUp {
        permille: u32,
    },
    Draw {
        permille: u32,
    },
    MirrorSideA {
        permille: u32,
    },
    MomentKind {
        kind: MomentKind,
        permille: u32,
    },
    /// The mean separation (integer division; 0 without separations).
    MeanGap {
        ms: u64,
    },
    BurstShare {
        permille: u32,
    },
    PausesMedian {
        count: u32,
    },
    PausedFights {
        permille: u32,
    },
    /// The shortest separation, below `TIMELINE_CONTRACT.strike_gap_ms`.
    StrikeGap {
        ms: u32,
    },
}

/// Printed, never fails `--check` nor the balance test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Warning {
    /// `first`'s win rate against `second`.
    Matchup {
        first: String,
        second: String,
        permille: u32,
    },
    /// Only when [`Criteria::moment_kind_fails`] is `false`.
    MomentKind { kind: MomentKind, permille: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CharacterRecord {
    pub slug: String,
    pub appearances: u32,
    pub wins: u32,
    pub draws: u32,
}

/// The fights between two characters, both orientations; `first` comes first in slug order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Matchup {
    pub first: String,
    pub second: String,
    pub fights: u32,
    pub wins_first: u32,
    pub wins_second: u32,
    pub draws: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Mirrors {
    pub fights: u32,
    pub wins_a: u32,
    pub wins_b: u32,
    pub draws: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Endings {
    pub ko: u32,
    pub decision: u32,
    pub draw: u32,
}

/// `end.t`, nearest-rank percentiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Duration {
    pub p10_ms: u32,
    pub p50_ms: u32,
    pub p90_ms: u32,
    /// Fights with `end.t` in [`DURATION_BAND_MS`], inclusive.
    pub in_band: u32,
}

/// Fights with at least one `moment` (of any kind, and of each kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Moments {
    pub any: u32,
    pub comeback: u32,
    pub decisive_crit: u32,
    pub clutch_ko: u32,
}

impl Moments {
    #[must_use]
    pub const fn of(&self, kind: MomentKind) -> u32 {
        match kind {
            MomentKind::Comeback => self.comeback,
            MomentKind::DecisiveCrit => self.decisive_crit,
            MomentKind::ClutchKo => self.clutch_ko,
        }
    }
}

/// Resolutions (`hit`, `block`, `dodge`) per fight and the separations between consecutive
/// ones, the pacing of 002 and 006 (RF11). A pause counts by its separation, whatever its cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Density {
    pub resolutions_p10: u32,
    pub resolutions_p50: u32,
    pub resolutions_p90: u32,
    pub gap_total_ms: u64,
    pub gaps: u64,
    /// `gap_total_ms / gaps` over every separation, pauses included (0 without gaps).
    pub mean_gap_ms: u64,
    /// The shortest separation (0 without gaps).
    pub min_gap_ms: u32,
    /// Separations of at most [`BURST_GAP_MAX_MS`].
    pub burst_gaps: u64,
    /// Separations of at least [`PAUSE_GAP_MIN_MS`].
    pub pause_gaps: u64,
    /// Pauses per fight, nearest-rank percentiles.
    pub pauses_p10: u32,
    pub pauses_p50: u32,
    pub pauses_p90: u32,
    /// Fights with at least one pause.
    pub fights_with_pause: u32,
}

/// Counts over a schedule. Deterministic: no clock, no maps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub seed_hex: String,
    pub ruleset_version: u16,
    /// Slugs in slug order; every per-character `Vec` follows it.
    pub characters: Vec<String>,
    pub pairs: u32,
    pub seeds_per_pair: u32,
    pub cross_fights: u32,
    pub mirror_fights: u32,
    pub per_character: Vec<CharacterRecord>,
    /// Pairs `i < j` in slug order.
    pub matchups: Vec<Matchup>,
    pub mirrors: Mirrors,
    pub endings: Endings,
    pub time_up: u32,
    pub duration: Duration,
    pub moments: Moments,
    /// The threshold of [`Report::wins_after_deficit`]
    /// (`Ruleset::comeback_deficit_permille`).
    pub deficit_permille: u32,
    /// Fights won by a side that was at some point at least `deficit_permille` of relative hp
    /// below the other (informative).
    pub wins_after_deficit: u32,
    /// Fights with at least one `knockdown`.
    pub knockdown: u32,
    pub density: Density,
}

/// `1000 * count >= min * total`.
fn at_least(count: u64, total: u64, min_permille: u32) -> bool {
    1000 * count >= u64::from(min_permille) * total
}

/// `1000 * count <= max * total`.
fn at_most(count: u64, total: u64, max_permille: u32) -> bool {
    1000 * count <= u64::from(max_permille) * total
}

/// `count * 1000 / total`, 0 when `total` is 0.
fn permille(count: u64, total: u64) -> u32 {
    (1000 * count)
        .checked_div(total)
        .and_then(|p| u32::try_from(p).ok())
        .unwrap_or(0)
}

/// Half points: `2 * wins + draws` over `2 * fights`.
const fn half_points(wins: u32, draws: u32, fights: u32) -> (u64, u64) {
    (2 * wins as u64 + draws as u64, 2 * fights as u64)
}

impl Report {
    /// The criteria of RF17 that fail (empty if the content is balanced).
    #[must_use]
    pub fn failures(&self, criteria: &Criteria) -> Vec<Failure> {
        let mut out = Vec::new();
        for c in &self.per_character {
            let (points, total) = half_points(c.wins, c.draws, c.appearances);
            if !(at_least(points, total, criteria.win_rate_min_permille)
                && at_most(points, total, criteria.win_rate_max_permille))
            {
                out.push(Failure::WinRate {
                    slug: c.slug.clone(),
                    permille: permille(points, total),
                });
            }
        }
        let n = u64::from(self.cross_fights);
        let moments = u64::from(self.moments.any);
        if !at_least(moments, n, criteria.any_moment_min_permille) {
            out.push(Failure::AnyMoment {
                permille: permille(moments, n),
            });
        }
        let in_band = u64::from(self.duration.in_band);
        if !at_least(in_band, n, criteria.in_band_min_permille) {
            out.push(Failure::DurationInBand {
                permille: permille(in_band, n),
            });
        }
        let time_up = u64::from(self.time_up);
        if !at_most(time_up, n, criteria.time_up_max_permille) {
            out.push(Failure::TimeUp {
                permille: permille(time_up, n),
            });
        }
        let draws = u64::from(self.endings.draw);
        if 1000 * draws >= u64::from(criteria.draw_below_permille) * n {
            out.push(Failure::Draw {
                permille: permille(draws, n),
            });
        }
        let m = self.mirrors;
        let (points, total) = half_points(m.wins_a, m.draws, m.fights);
        if !(at_least(points, total, criteria.mirror_side_a_min_permille)
            && at_most(points, total, criteria.mirror_side_a_max_permille))
        {
            out.push(Failure::MirrorSideA {
                permille: permille(points, total),
            });
        }
        if criteria.moment_kind_fails {
            for (kind, permille) in self.dead_moment_kinds(criteria) {
                out.push(Failure::MomentKind { kind, permille });
            }
        }
        self.pacing_failures(criteria, &mut out);
        out
    }

    /// 006 RF12. Without separations the mean fails and the shortest one is not evaluated.
    fn pacing_failures(&self, criteria: &Criteria, out: &mut Vec<Failure>) {
        let d = self.density;
        let (total, gaps) = (d.gap_total_ms, d.gaps);
        if gaps == 0
            || total < u64::from(criteria.mean_gap_min_ms) * gaps
            || total > u64::from(criteria.mean_gap_max_ms) * gaps
        {
            out.push(Failure::MeanGap { ms: d.mean_gap_ms });
        }
        if !at_least(d.burst_gaps, gaps, criteria.burst_min_permille) {
            out.push(Failure::BurstShare {
                permille: permille(d.burst_gaps, gaps),
            });
        }
        if d.pauses_p50 < criteria.pauses_p50_min {
            out.push(Failure::PausesMedian {
                count: d.pauses_p50,
            });
        }
        let n = u64::from(self.cross_fights);
        let paused = u64::from(d.fights_with_pause);
        if !at_least(paused, n, criteria.paused_fights_min_permille) {
            out.push(Failure::PausedFights {
                permille: permille(paused, n),
            });
        }
        if gaps > 0 && d.min_gap_ms < TIMELINE_CONTRACT.strike_gap_ms {
            out.push(Failure::StrikeGap { ms: d.min_gap_ms });
        }
    }

    /// What to look at without failing: matchups outside the warning band and, if
    /// [`Criteria::moment_kind_fails`] is `false`, the dead moment kinds.
    #[must_use]
    pub fn warnings(&self, criteria: &Criteria) -> Vec<Warning> {
        let mut out = Vec::new();
        for m in &self.matchups {
            let (points, total) = half_points(m.wins_first, m.draws, m.fights);
            if !(at_least(points, total, criteria.matchup_warn_min_permille)
                && at_most(points, total, criteria.matchup_warn_max_permille))
            {
                out.push(Warning::Matchup {
                    first: m.first.clone(),
                    second: m.second.clone(),
                    permille: permille(points, total),
                });
            }
        }
        if !criteria.moment_kind_fails {
            for (kind, permille) in self.dead_moment_kinds(criteria) {
                out.push(Warning::MomentKind { kind, permille });
            }
        }
        out
    }

    fn dead_moment_kinds(&self, criteria: &Criteria) -> Vec<(MomentKind, u32)> {
        let n = u64::from(self.cross_fights);
        MOMENT_KINDS
            .iter()
            .filter_map(|&kind| {
                let count = u64::from(self.moments.of(kind));
                (!at_least(count, n, criteria.moment_kind_min_permille))
                    .then(|| (kind, permille(count, n)))
            })
            .collect()
    }

    /// The report as text, in English (a development tool, not UI: RF15).
    #[must_use]
    pub fn render_text(&self, criteria: &Criteria) -> String {
        let mut s = String::new();
        let n = u64::from(self.cross_fights);
        let share = |count: u32| pct(permille(u64::from(count), n));
        let width = self.characters.iter().map(String::len).max().unwrap_or(0);
        let _ = writeln!(
            s,
            "sim: ruleset v{}, root seed {}",
            self.ruleset_version, self.seed_hex
        );
        let _ = writeln!(
            s,
            "{} characters, {} pairs, {} seeds per pair: {} cross fights \
             (both orientations); {} mirror fights (side-a bias only)",
            self.characters.len(),
            self.pairs,
            self.seeds_per_pair,
            self.cross_fights,
            self.mirror_fights
        );
        let _ = writeln!(s, "\nwin rate (cross fights, a draw counts half):");
        for c in &self.per_character {
            let (points, total) = half_points(c.wins, c.draws, c.appearances);
            let _ = writeln!(
                s,
                "  {:<width$}  {:>6}  ({} wins, {} draws, {} fights)",
                c.slug,
                pct(permille(points, total)),
                c.wins,
                c.draws,
                c.appearances
            );
        }
        let _ = writeln!(s, "\nmatchups (win rate of the row against the column):");
        let _ = write!(s, "  {:<width$}", "");
        for slug in &self.characters {
            let _ = write!(s, "  {slug:>width$}");
        }
        let _ = writeln!(s);
        for (i, row) in self.characters.iter().enumerate() {
            let _ = write!(s, "  {row:<width$}");
            for j in 0..self.characters.len() {
                let cell = self.matchup_rate(i, j).map_or_else(|| "-".to_owned(), pct);
                let _ = write!(s, "  {cell:>width$}");
            }
            let _ = writeln!(s);
        }
        let m = self.mirrors;
        let (points, total) = half_points(m.wins_a, m.draws, m.fights);
        let _ = writeln!(
            s,
            "\nmirrors: side a {} ({} a, {} b, {} draws of {})",
            pct(permille(points, total)),
            m.wins_a,
            m.wins_b,
            m.draws,
            m.fights
        );
        let e = self.endings;
        let _ = writeln!(
            s,
            "endings: ko {}, decision {}, draw {}; time_up {}",
            share(e.ko),
            share(e.decision),
            share(e.draw),
            share(self.time_up)
        );
        let d = self.duration;
        let _ = writeln!(
            s,
            "duration (end.t): p10 {} ms, p50 {} ms, p90 {} ms; {} in {}-{} ms",
            d.p10_ms,
            d.p50_ms,
            d.p90_ms,
            share(d.in_band),
            DURATION_BAND_MS.0,
            DURATION_BAND_MS.1
        );
        let mo = self.moments;
        let _ = writeln!(
            s,
            "moments: any {}; comeback {}, decisive_crit {}, clutch_ko {}",
            share(mo.any),
            share(mo.comeback),
            share(mo.decisive_crit),
            share(mo.clutch_ko)
        );
        let _ = writeln!(
            s,
            "wins after being >= {} permille of hp behind: {}",
            self.deficit_permille,
            share(self.wins_after_deficit)
        );
        let _ = writeln!(s, "knockdown: {}", share(self.knockdown));
        let de = self.density;
        let _ = writeln!(
            s,
            "density: resolutions per fight p10 {}, p50 {}, p90 {}; mean gap {} ms",
            de.resolutions_p10, de.resolutions_p50, de.resolutions_p90, de.mean_gap_ms
        );
        let _ = writeln!(
            s,
            "pacing: min gap {} ms; burst (<= {} ms) {} of gaps; pauses (>= {} ms) {} of gaps, \
             per fight p10 {}, p50 {}, p90 {}; fights with a pause {}",
            de.min_gap_ms,
            BURST_GAP_MAX_MS,
            pct(permille(de.burst_gaps, de.gaps)),
            PAUSE_GAP_MIN_MS,
            pct(permille(de.pause_gaps, de.gaps)),
            de.pauses_p10,
            de.pauses_p50,
            de.pauses_p90,
            share(de.fights_with_pause)
        );
        let warnings = self.warnings(criteria);
        if !warnings.is_empty() {
            let _ = writeln!(s, "\nwarnings:");
            for w in &warnings {
                let _ = writeln!(s, "  {}", describe_warning(w, criteria));
            }
        }
        let failures = self.failures(criteria);
        if failures.is_empty() {
            let _ = writeln!(s, "\nRF17: all criteria pass");
        } else {
            let _ = writeln!(s, "\nRF17: {} criteria fail:", failures.len());
            for f in &failures {
                let _ = writeln!(s, "  {}", describe_failure(f, criteria));
            }
        }
        s
    }

    /// Win rate (‰, half points) of `i` against `j`; `None` on the diagonal.
    fn matchup_rate(&self, i: usize, j: usize) -> Option<u32> {
        let (first, second) = (i.min(j), i.max(j));
        if first == second {
            return None;
        }
        let first_slug = self.characters.get(first)?;
        let second_slug = self.characters.get(second)?;
        let m = self
            .matchups
            .iter()
            .find(|m| &m.first == first_slug && &m.second == second_slug)?;
        let wins = if i == first {
            m.wins_first
        } else {
            m.wins_second
        };
        let (points, total) = half_points(wins, m.draws, m.fights);
        Some(permille(points, total))
    }
}

/// `512` ‰ as `51.2%`.
fn pct(permille: u32) -> String {
    format!("{}.{}%", permille / 10, permille % 10)
}

fn describe_failure(f: &Failure, c: &Criteria) -> String {
    match f {
        Failure::WinRate { slug, permille } => format!(
            "win_rate: {slug} {} (must be {}..={})",
            pct(*permille),
            pct(c.win_rate_min_permille),
            pct(c.win_rate_max_permille)
        ),
        Failure::AnyMoment { permille } => format!(
            "any_moment: {} (must be >= {})",
            pct(*permille),
            pct(c.any_moment_min_permille)
        ),
        Failure::DurationInBand { permille } => format!(
            "duration_in_band: {} (must be >= {})",
            pct(*permille),
            pct(c.in_band_min_permille)
        ),
        Failure::TimeUp { permille } => format!(
            "time_up: {} (must be <= {})",
            pct(*permille),
            pct(c.time_up_max_permille)
        ),
        Failure::Draw { permille } => format!(
            "draw: {} (must be < {})",
            pct(*permille),
            pct(c.draw_below_permille)
        ),
        Failure::MirrorSideA { permille } => format!(
            "mirror_side_a: {} (must be {}..={})",
            pct(*permille),
            pct(c.mirror_side_a_min_permille),
            pct(c.mirror_side_a_max_permille)
        ),
        Failure::MomentKind { kind, permille } => format!(
            "moment_kind: {} {} (must be >= {})",
            kind_code(*kind),
            pct(*permille),
            pct(c.moment_kind_min_permille)
        ),
        Failure::MeanGap { ms } => format!(
            "mean_gap: {ms} ms (must be {}..={} ms)",
            c.mean_gap_min_ms, c.mean_gap_max_ms
        ),
        Failure::BurstShare { permille } => format!(
            "burst_share: {} (must be >= {})",
            pct(*permille),
            pct(c.burst_min_permille)
        ),
        Failure::PausesMedian { count } => {
            format!("pauses_median: {count} (must be >= {})", c.pauses_p50_min)
        }
        Failure::PausedFights { permille } => format!(
            "paused_fights: {} (must be >= {})",
            pct(*permille),
            pct(c.paused_fights_min_permille)
        ),
        Failure::StrikeGap { ms } => format!(
            "strike_gap: {ms} ms (must be >= {} ms)",
            TIMELINE_CONTRACT.strike_gap_ms
        ),
    }
}

fn describe_warning(w: &Warning, c: &Criteria) -> String {
    match w {
        Warning::Matchup {
            first,
            second,
            permille,
        } => format!(
            "matchup: {first} wins {} against {second} (outside {}..={})",
            pct(*permille),
            pct(c.matchup_warn_min_permille),
            pct(c.matchup_warn_max_permille)
        ),
        Warning::MomentKind { kind, permille } => {
            format!(
                "moment_kind: {} {} (below {})",
                kind_code(*kind),
                pct(*permille),
                pct(c.moment_kind_min_permille)
            )
        }
    }
}

const fn kind_code(kind: MomentKind) -> &'static str {
    match kind {
        MomentKind::Comeback => "comeback",
        MomentKind::DecisiveCrit => "decisive_crit",
        MomentKind::ClutchKo => "clutch_ko",
    }
}

/// What one log contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Facts {
    winner: Option<Side>,
    reason: EndReason,
    end_t: u32,
    time_up: bool,
    kinds: [bool; 3],
    knockdown: bool,
    resolutions: u32,
    gap_total_ms: u64,
    gaps: u64,
    min_gap_ms: Option<u32>,
    burst_gaps: u64,
    pause_gaps: u64,
    /// The winner was at some point at least `deficit` ‰ of relative hp below the loser.
    winner_was_behind: bool,
}

fn relative_hp(hp: u16, max_hp: u16) -> u32 {
    (u32::from(hp) * 1000)
        .checked_div(u32::from(max_hp))
        .unwrap_or(0)
}

fn facts(log: &FightLog, deficit_permille: u32) -> Result<Facts, SimError> {
    let mut hp = [log.a.max_hp, log.b.max_hp];
    let max = [log.a.max_hp, log.b.max_hp];
    let mut worst_behind = [0_u32; 2];
    let mut end = None;
    let mut f = Facts {
        winner: None,
        reason: EndReason::Draw,
        end_t: 0,
        time_up: false,
        kinds: [false; 3],
        knockdown: false,
        resolutions: 0,
        gap_total_ms: 0,
        gaps: 0,
        min_gap_ms: None,
        burst_gaps: 0,
        pause_gaps: 0,
        winner_was_behind: false,
    };
    let mut last_resolution: Option<u32> = None;
    let index = |side: Side| match side {
        Side::A => 0,
        Side::B => 1,
    };
    for event in &log.events {
        let mut resolved = |t: u32| {
            f.resolutions += 1;
            if let Some(last) = last_resolution {
                let gap = t.saturating_sub(last);
                f.gap_total_ms += u64::from(gap);
                f.gaps += 1;
                f.min_gap_ms = Some(f.min_gap_ms.map_or(gap, |m| m.min(gap)));
                f.burst_gaps += u64::from(gap <= BURST_GAP_MAX_MS);
                f.pause_gaps += u64::from(gap >= PAUSE_GAP_MIN_MS);
            }
            last_resolution = Some(t);
        };
        match event {
            FightEvent::Hit {
                t,
                attacker,
                hp: after,
                ..
            }
            | FightEvent::Block {
                t,
                attacker,
                hp: after,
                ..
            } => {
                resolved(*t);
                if let Some(slot) = hp.get_mut(index(attacker.opponent())) {
                    *slot = *after;
                }
                let rel = [relative_hp(hp[0], max[0]), relative_hp(hp[1], max[1])];
                worst_behind[0] = worst_behind[0].max(rel[1].saturating_sub(rel[0]));
                worst_behind[1] = worst_behind[1].max(rel[0].saturating_sub(rel[1]));
            }
            FightEvent::Dodge { t, .. } => resolved(*t),
            FightEvent::Knockdown { .. } => f.knockdown = true,
            FightEvent::Moment { kind, .. } => {
                if let Some(seen) = MOMENT_KINDS
                    .iter()
                    .position(|k| k == kind)
                    .and_then(|i| f.kinds.get_mut(i))
                {
                    *seen = true;
                }
            }
            FightEvent::TimeUp { .. } => f.time_up = true,
            FightEvent::End { t, winner, reason } => end = Some((*t, *winner, *reason)),
            FightEvent::Intro { .. }
            | FightEvent::Attack { .. }
            | FightEvent::LowHp { .. }
            | FightEvent::Ko { .. } => {}
        }
    }
    let (end_t, winner, reason) = end.ok_or(SimError::LogWithoutEnd)?;
    f.end_t = end_t;
    f.winner = winner;
    f.reason = reason;
    f.winner_was_behind = winner.is_some_and(|w| worst_behind[index(w)] >= deficit_permille);
    Ok(f)
}

/// Accumulates fights into a [`Report`].
#[derive(Debug, Clone)]
pub(crate) struct Tally {
    slugs: Vec<String>,
    ruleset_version: u16,
    deficit_permille: u32,
    per_character: Vec<CharacterRecord>,
    matchups: Vec<Matchup>,
    mirrors: Mirrors,
    endings: Endings,
    time_up: u32,
    durations: Vec<u32>,
    in_band: u32,
    moments: Moments,
    wins_after_deficit: u32,
    knockdown: u32,
    resolutions: Vec<u32>,
    gap_total_ms: u64,
    gaps: u64,
    min_gap_ms: Option<u32>,
    burst_gaps: u64,
    pause_gaps: u64,
    /// Pauses per cross fight (sorted in `finish`).
    pauses: Vec<u32>,
    fights_with_pause: u32,
}

impl Tally {
    /// `slugs` in slug order, distinct.
    pub(crate) fn new(slugs: Vec<String>, ruleset: &Ruleset) -> Self {
        let per_character = slugs
            .iter()
            .map(|slug| CharacterRecord {
                slug: slug.clone(),
                appearances: 0,
                wins: 0,
                draws: 0,
            })
            .collect();
        let mut matchups = Vec::new();
        for (i, first) in slugs.iter().enumerate() {
            for second in slugs.iter().skip(i + 1) {
                matchups.push(Matchup {
                    first: first.clone(),
                    second: second.clone(),
                    fights: 0,
                    wins_first: 0,
                    wins_second: 0,
                    draws: 0,
                });
            }
        }
        Self {
            slugs,
            ruleset_version: ruleset.version(),
            deficit_permille: ruleset.comeback_deficit_permille(),
            per_character,
            matchups,
            mirrors: Mirrors {
                fights: 0,
                wins_a: 0,
                wins_b: 0,
                draws: 0,
            },
            endings: Endings {
                ko: 0,
                decision: 0,
                draw: 0,
            },
            time_up: 0,
            durations: Vec::new(),
            in_band: 0,
            moments: Moments {
                any: 0,
                comeback: 0,
                decisive_crit: 0,
                clutch_ko: 0,
            },
            wins_after_deficit: 0,
            knockdown: 0,
            resolutions: Vec::new(),
            gap_total_ms: 0,
            gaps: 0,
            min_gap_ms: None,
            burst_gaps: 0,
            pause_gaps: 0,
            pauses: Vec::new(),
            fights_with_pause: 0,
        }
    }

    /// A cross fight: roster index `a` on side `a`, `b` on side `b`, `a != b`.
    pub(crate) fn cross(&mut self, a: usize, b: usize, log: &FightLog) -> Result<(), SimError> {
        let f = facts(log, self.deficit_permille)?;
        for (index, side) in [(a, Side::A), (b, Side::B)] {
            if let Some(c) = self.per_character.get_mut(index) {
                c.appearances += 1;
                match f.winner {
                    Some(w) if w == side => c.wins += 1,
                    None => c.draws += 1,
                    Some(_) => {}
                }
            }
        }
        let (first, second) = (a.min(b), a.max(b));
        let (first_slug, second_slug) = (self.slugs.get(first), self.slugs.get(second));
        if let Some(m) = self
            .matchups
            .iter_mut()
            .find(|m| Some(&m.first) == first_slug && Some(&m.second) == second_slug)
        {
            m.fights += 1;
            let first_side = if a == first { Side::A } else { Side::B };
            match f.winner {
                Some(w) if w == first_side => m.wins_first += 1,
                Some(_) => m.wins_second += 1,
                None => m.draws += 1,
            }
        }
        match f.reason {
            EndReason::Ko => self.endings.ko += 1,
            EndReason::Decision => self.endings.decision += 1,
            EndReason::Draw => self.endings.draw += 1,
        }
        self.time_up += u32::from(f.time_up);
        self.durations.push(f.end_t);
        self.in_band += u32::from((DURATION_BAND_MS.0..=DURATION_BAND_MS.1).contains(&f.end_t));
        let [comeback, decisive_crit, clutch_ko] = f.kinds;
        self.moments.any += u32::from(f.kinds.contains(&true));
        self.moments.comeback += u32::from(comeback);
        self.moments.decisive_crit += u32::from(decisive_crit);
        self.moments.clutch_ko += u32::from(clutch_ko);
        self.wins_after_deficit += u32::from(f.winner_was_behind);
        self.knockdown += u32::from(f.knockdown);
        self.resolutions.push(f.resolutions);
        self.gap_total_ms += f.gap_total_ms;
        self.gaps += f.gaps;
        self.min_gap_ms = match (self.min_gap_ms, f.min_gap_ms) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        };
        self.burst_gaps += f.burst_gaps;
        self.pause_gaps += f.pause_gaps;
        self.pauses
            .push(u32::try_from(f.pause_gaps).unwrap_or(u32::MAX));
        self.fights_with_pause += u32::from(f.pause_gaps > 0);
        Ok(())
    }

    /// A mirror fight: only the side-a bias.
    pub(crate) fn mirror(&mut self, log: &FightLog) -> Result<(), SimError> {
        let f = facts(log, self.deficit_permille)?;
        self.mirrors.fights += 1;
        match f.winner {
            Some(Side::A) => self.mirrors.wins_a += 1,
            Some(Side::B) => self.mirrors.wins_b += 1,
            None => self.mirrors.draws += 1,
        }
        Ok(())
    }

    pub(crate) fn finish(mut self, plan: &Plan, seed: [u8; 32]) -> Report {
        self.durations.sort_unstable();
        self.resolutions.sort_unstable();
        self.pauses.sort_unstable();
        let cross_fights = u32::try_from(self.durations.len()).unwrap_or(u32::MAX);
        Report {
            seed_hex: seed_hex(&seed),
            ruleset_version: self.ruleset_version,
            characters: self.slugs,
            pairs: plan.pairs,
            seeds_per_pair: plan.seeds_per_pair,
            cross_fights,
            mirror_fights: self.mirrors.fights,
            per_character: self.per_character,
            matchups: self.matchups,
            mirrors: self.mirrors,
            endings: self.endings,
            time_up: self.time_up,
            duration: Duration {
                p10_ms: nearest_rank(&self.durations, 10),
                p50_ms: nearest_rank(&self.durations, 50),
                p90_ms: nearest_rank(&self.durations, 90),
                in_band: self.in_band,
            },
            moments: self.moments,
            deficit_permille: self.deficit_permille,
            wins_after_deficit: self.wins_after_deficit,
            knockdown: self.knockdown,
            density: Density {
                resolutions_p10: nearest_rank(&self.resolutions, 10),
                resolutions_p50: nearest_rank(&self.resolutions, 50),
                resolutions_p90: nearest_rank(&self.resolutions, 90),
                gap_total_ms: self.gap_total_ms,
                gaps: self.gaps,
                mean_gap_ms: self.gap_total_ms.checked_div(self.gaps).unwrap_or(0),
                min_gap_ms: self.min_gap_ms.unwrap_or(0),
                burst_gaps: self.burst_gaps,
                pause_gaps: self.pause_gaps,
                pauses_p10: nearest_rank(&self.pauses, 10),
                pauses_p50: nearest_rank(&self.pauses, 50),
                pauses_p90: nearest_rank(&self.pauses, 90),
                fights_with_pause: self.fights_with_pause,
            },
        }
    }
}

/// Nearest-rank percentile of sorted values: the value at rank `ceil(p * n / 100)`, 0 if empty.
fn nearest_rank(sorted: &[u32], p: usize) -> u32 {
    let rank = (p * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests;
