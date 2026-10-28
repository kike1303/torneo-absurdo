//! Metrics over hand-built logs and every edge of [`Criteria::RF17`].
use engine::{FighterInfo, Ruleset, TIMELINE_CONTRACT};

use super::*;
use crate::DEFAULT_SEED;

fn v1() -> Ruleset {
    Ruleset::for_version(1).unwrap()
}

fn log(max_a: u16, max_b: u16, events: Vec<FightEvent>) -> FightLog {
    FightLog {
        ruleset_version: 1,
        a: FighterInfo {
            slug: "aa".to_owned(),
            max_hp: max_a,
        },
        b: FighterInfo {
            slug: "bb".to_owned(),
            max_hp: max_b,
        },
        events,
    }
}

fn end(t: u32, winner: Option<Side>, reason: EndReason) -> FightEvent {
    FightEvent::End { t, winner, reason }
}

fn won(winner: Side, t: u32) -> FightLog {
    log(1000, 1000, vec![end(t, Some(winner), EndReason::Ko)])
}

fn drawn() -> FightLog {
    log(
        1000,
        1000,
        vec![
            FightEvent::TimeUp { t: 86_000 },
            end(90_000, None, EndReason::Draw),
        ],
    )
}

fn hit(t: u32, attacker: Side, hp: u16) -> FightEvent {
    FightEvent::Hit {
        t,
        attacker,
        ability: "strike".to_owned(),
        damage: 1,
        crit: false,
        weak: false,
        hp,
    }
}

fn moment(kind: MomentKind) -> FightEvent {
    FightEvent::Moment {
        t: 1,
        kind,
        side: Side::A,
    }
}

fn tally(n: usize) -> Tally {
    let slugs = ["aa", "bb", "cc", "dd"]
        .iter()
        .take(n)
        .map(|s| (*s).to_owned())
        .collect();
    Tally::new(slugs, &v1())
}

fn finish(t: Tally, characters: usize) -> Report {
    t.finish(&Plan::new(characters, 1_000_000).unwrap(), DEFAULT_SEED)
}

#[test]
fn half_points_count_a_draw_as_half_a_win() {
    let mut t = tally(2);
    t.cross(0, 1, &won(Side::A, 50_000)).unwrap(); // aa wins
    t.cross(1, 0, &won(Side::A, 50_000)).unwrap(); // bb wins
    t.cross(1, 0, &won(Side::B, 50_000)).unwrap(); // aa wins
    t.cross(0, 1, &drawn()).unwrap();
    let r = finish(t, 2);
    let rec = |i: usize| {
        let c = &r.per_character[i];
        (c.appearances, c.wins, c.draws)
    };
    assert_eq!(rec(0), (4, 2, 1));
    assert_eq!(rec(1), (4, 1, 1));
    let m = &r.matchups[0];
    assert_eq!(
        (m.fights, m.wins_first, m.wins_second, m.draws),
        (4, 2, 1, 1)
    );
    // aa: (2*2 + 1) / 8 = 625 ‰; bb: (2*1 + 1) / 8 = 375 ‰.
    assert_eq!(r.matchup_rate(0, 1), Some(625));
    assert_eq!(r.matchup_rate(1, 0), Some(375));
    assert_eq!(r.matchup_rate(0, 0), None);
    assert_eq!(
        (r.endings.ko, r.endings.decision, r.endings.draw, r.time_up),
        (3, 0, 1, 1)
    );
    assert_eq!(r.cross_fights, 4);
}

#[test]
fn the_matrix_has_every_pair_once_in_slug_order() {
    let mut t = tally(3);
    t.cross(2, 0, &won(Side::A, 50_000)).unwrap(); // cc beats aa
    t.cross(1, 2, &won(Side::B, 50_000)).unwrap(); // cc beats bb
    let r = finish(t, 3);
    let pairs: Vec<(&str, &str, u32, u32)> = r
        .matchups
        .iter()
        .map(|m| {
            (
                m.first.as_str(),
                m.second.as_str(),
                m.wins_first,
                m.wins_second,
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [("aa", "bb", 0, 0), ("aa", "cc", 0, 1), ("bb", "cc", 0, 1)]
    );
    assert_eq!(r.matchup_rate(2, 0), Some(1000));
    assert_eq!(r.matchup_rate(0, 2), Some(0));
}

#[test]
fn mirrors_only_count_for_the_side_bias() {
    let mut t = tally(2);
    t.mirror(&won(Side::A, 1)).unwrap();
    t.mirror(&won(Side::B, 1)).unwrap();
    t.mirror(&drawn()).unwrap();
    t.cross(0, 1, &won(Side::A, 50_000)).unwrap();
    let r = finish(t, 2);
    assert_eq!(
        (
            r.mirrors.fights,
            r.mirrors.wins_a,
            r.mirrors.wins_b,
            r.mirrors.draws
        ),
        (3, 1, 1, 1)
    );
    assert_eq!(r.mirror_fights, 3);
    assert_eq!(r.cross_fights, 1);
    assert_eq!((r.endings.ko, r.endings.draw, r.time_up), (1, 0, 0));
    assert_eq!(r.duration.p50_ms, 50_000);
    assert_eq!(r.per_character[0].appearances, 1);
}

#[test]
fn nearest_rank_percentiles() {
    let ten: Vec<u32> = (1..=10).collect();
    assert_eq!(nearest_rank(&ten, 10), 1);
    assert_eq!(nearest_rank(&ten, 50), 5);
    assert_eq!(nearest_rank(&ten, 90), 9);
    assert_eq!(nearest_rank(&ten, 100), 10);
    let eleven: Vec<u32> = (1..=11).collect();
    assert_eq!(nearest_rank(&eleven, 10), 2); // ceil(1.1)
    assert_eq!(nearest_rank(&eleven, 50), 6); // ceil(5.5)
    assert_eq!(nearest_rank(&eleven, 90), 10); // ceil(9.9)
    assert_eq!(nearest_rank(&[7], 10), 7);
    assert_eq!(nearest_rank(&[7], 90), 7);
    assert_eq!(nearest_rank(&[], 50), 0);
}

#[test]
fn durations_are_sorted_and_the_band_is_inclusive() {
    let mut t = tally(2);
    for end_t in [90_001, 45_000, 44_999, 90_000, 60_000] {
        t.cross(0, 1, &won(Side::A, end_t)).unwrap();
    }
    let r = finish(t, 2);
    assert_eq!(r.duration.in_band, 3);
    // Sorted: 44 999, 45 000, 60 000, 90 000, 90 001.
    assert_eq!(
        (r.duration.p10_ms, r.duration.p50_ms, r.duration.p90_ms),
        (44_999, 60_000, 90_001)
    );
}

#[test]
fn moments_count_fights_not_events() {
    let mut t = tally(2);
    let with = |kinds: &[MomentKind]| {
        let mut events: Vec<FightEvent> = kinds.iter().map(|&k| moment(k)).collect();
        events.push(end(50_000, Some(Side::A), EndReason::Ko));
        log(1000, 1000, events)
    };
    t.cross(0, 1, &with(&[MomentKind::Comeback, MomentKind::Comeback]))
        .unwrap();
    t.cross(
        0,
        1,
        &with(&[MomentKind::DecisiveCrit, MomentKind::ClutchKo]),
    )
    .unwrap();
    t.cross(0, 1, &with(&[MomentKind::ClutchKo])).unwrap();
    t.cross(0, 1, &with(&[])).unwrap();
    let r = finish(t, 2);
    assert_eq!(
        r.moments,
        Moments {
            any: 3,
            comeback: 1,
            decisive_crit: 1,
            clutch_ko: 2,
        }
    );
}

#[test]
fn knockdown_and_density() {
    let mut t = tally(2);
    t.cross(
        0,
        1,
        &log(
            1000,
            1000,
            vec![
                FightEvent::Intro { t: 0 },
                hit(1000, Side::A, 990),
                FightEvent::Knockdown {
                    t: 1000,
                    side: Side::B,
                    duration_ms: 1500,
                },
                FightEvent::Dodge {
                    t: 3000,
                    attacker: Side::B,
                    ability: "strike".to_owned(),
                },
                FightEvent::Block {
                    t: 3600,
                    attacker: Side::A,
                    ability: "strike".to_owned(),
                    damage: 1,
                    hp: 989,
                },
                end(50_000, Some(Side::A), EndReason::Ko),
            ],
        ),
    )
    .unwrap();
    t.cross(0, 1, &won(Side::B, 50_000)).unwrap();
    let r = finish(t, 2);
    assert_eq!(r.knockdown, 1);
    // Resolutions per fight: [0, 3]; gaps 2000 and 600 (a burst, no pause).
    assert_eq!(
        r.density,
        Density {
            resolutions_p10: 0,
            resolutions_p50: 0,
            resolutions_p90: 3,
            gap_total_ms: 2600,
            gaps: 2,
            mean_gap_ms: 1300,
            min_gap_ms: 600,
            burst_gaps: 1,
            pause_gaps: 0,
            pauses_p10: 0,
            pauses_p50: 0,
            pauses_p90: 0,
            fights_with_pause: 0,
        }
    );
}

fn dodge(t: u32) -> FightEvent {
    FightEvent::Dodge {
        t,
        attacker: Side::A,
        ability: "strike".to_owned(),
    }
}

/// A KO fight whose resolutions are separated by `gaps`, starting at 3000.
fn with_gaps(gaps: &[u32]) -> FightLog {
    let mut t = 3000;
    let mut events = vec![FightEvent::Intro { t: 0 }, dodge(t)];
    for gap in gaps {
        t += gap;
        events.push(dodge(t));
    }
    events.push(end(t + 4000, Some(Side::A), EndReason::Ko));
    log(1000, 1000, events)
}

#[test]
fn burst_and_pause_edges() {
    let mut t = tally(2);
    t.cross(0, 1, &with_gaps(&[900, 901, 2500, 2499])).unwrap();
    let d = finish(t, 2).density;
    assert_eq!((d.gaps, d.gap_total_ms, d.mean_gap_ms), (4, 6800, 1700));
    assert_eq!(d.min_gap_ms, 900);
    assert_eq!((d.burst_gaps, d.pause_gaps), (1, 1));
    assert_eq!((d.pauses_p50, d.fights_with_pause), (1, 1));
    // A knockdown counts as a pause only by its separation; no gaps gives a min of 0.
    let mut t = tally(2);
    t.cross(0, 1, &won(Side::A, 50_000)).unwrap();
    let d = finish(t, 2).density;
    assert_eq!((d.min_gap_ms, d.gaps, d.mean_gap_ms), (0, 0, 0));
}

#[test]
fn pauses_per_fight_are_nearest_rank_percentiles() {
    let pause = PAUSE_GAP_MIN_MS;
    let mut t = tally(2);
    // Pauses per fight, out of order: 3, 0, 1, 4, 2, 0, 1, 2, 5, 2.
    for count in [3, 0, 1, 4, 2, 0, 1, 2, 5, 2] {
        let mut gaps = vec![1000];
        gaps.extend(std::iter::repeat_n(pause, count));
        t.cross(0, 1, &with_gaps(&gaps)).unwrap();
    }
    let d = finish(t, 2).density;
    // Sorted: 0, 0, 1, 1, 2, 2, 2, 3, 4, 5.
    assert_eq!((d.pauses_p10, d.pauses_p50, d.pauses_p90), (0, 2, 4));
    assert_eq!(d.fights_with_pause, 8);
    assert_eq!(d.pause_gaps, 20);
    assert_eq!(d.min_gap_ms, 1000);
}

/// The pacing failures of a report built from `logs` (cross fights between `aa` and `bb`).
fn pacing_failures_of(logs: &[FightLog]) -> Vec<Failure> {
    let mut t = tally(2);
    for l in logs {
        t.cross(0, 1, l).unwrap();
    }
    finish(t, 2)
        .failures(&Criteria::RF17)
        .into_iter()
        .filter(|f| {
            matches!(
                f,
                Failure::MeanGap { .. }
                    | Failure::BurstShare { .. }
                    | Failure::PausesMedian { .. }
                    | Failure::PausedFights { .. }
                    | Failure::StrikeGap { .. }
            )
        })
        .collect()
}

#[test]
fn every_pacing_failure_from_built_logs() {
    // Fast and flat: mean 500, every gap a burst, no pause, below the contract.
    assert_eq!(
        pacing_failures_of(&[with_gaps(&[500; 20])]),
        vec![
            Failure::MeanGap { ms: 500 },
            Failure::PausesMedian { count: 0 },
            Failure::PausedFights { permille: 0 },
            Failure::StrikeGap { ms: 500 },
        ]
    );
    // Even and slow: no burst.
    assert_eq!(
        pacing_failures_of(&[with_gaps(&[1000; 20])]),
        vec![
            Failure::MeanGap { ms: 1000 },
            Failure::BurstShare { permille: 0 },
            Failure::PausesMedian { count: 0 },
            Failure::PausedFights { permille: 0 },
        ]
    );
    // Two rhythms: 6 bursts of 700, 10 gaps of 1000 and 4 pauses of 2500 (mean 1210).
    let mut gaps = vec![700; 6];
    gaps.extend([1000; 10]);
    gaps.extend([2500; 4]);
    assert_eq!(pacing_failures_of(&[with_gaps(&gaps)]), vec![]);
    // Without any separation, the mean fails and the shortest one is not evaluated.
    let failures = pacing_failures_of(&[won(Side::A, 50_000)]);
    assert!(failures.contains(&Failure::MeanGap { ms: 0 }));
    assert!(
        !failures
            .iter()
            .any(|f| matches!(f, Failure::StrikeGap { .. }))
    );
}

/// 006 RF12: the measure definitions of `sim` against the ruleset and the `Timeline` contract.
#[test]
fn every_breather_is_a_pause() {
    assert!(v1().breather_gap_min() >= PAUSE_GAP_MIN_MS);
}

#[test]
fn a_burst_is_reachable() {
    assert!(v1().strike_gap_min() <= BURST_GAP_MAX_MS);
}

#[test]
fn the_shortest_gap_respects_the_contract() {
    assert!(v1().strike_gap_min() >= TIMELINE_CONTRACT.strike_gap_ms);
}

/// `a` falls to `a_low` while `b` is still full, then wins.
fn win_after_falling_to(a_low: u16) -> FightLog {
    log(
        1000,
        2000,
        vec![
            hit(1000, Side::B, a_low),
            hit(2000, Side::A, 1500),
            hit(3000, Side::A, 0),
            end(7000, Some(Side::A), EndReason::Ko),
        ],
    )
}

#[test]
fn a_win_after_being_250_permille_behind_counts_and_249_does_not() {
    assert_eq!(v1().comeback_deficit_permille(), 250);
    // b at 2000/2000 = 1000 ‰; a at 750/1000 = 750 ‰ (250 behind) or 751 (249 behind).
    let mut t = tally(2);
    t.cross(0, 1, &win_after_falling_to(750)).unwrap();
    assert_eq!(finish(t, 2).wins_after_deficit, 1);
    let mut t = tally(2);
    t.cross(0, 1, &win_after_falling_to(751)).unwrap();
    assert_eq!(finish(t, 2).wins_after_deficit, 0);
    // The loser being behind does not count, and a draw never does.
    let mut t = tally(2);
    let lost = log(
        1000,
        2000,
        vec![
            hit(1000, Side::B, 500),
            end(7000, Some(Side::B), EndReason::Decision),
        ],
    );
    t.cross(0, 1, &lost).unwrap();
    t.cross(0, 1, &drawn()).unwrap();
    let r = finish(t, 2);
    assert_eq!(r.wins_after_deficit, 0);
    assert_eq!(r.deficit_permille, 250);
}

#[test]
fn relative_hp_uses_max_hp_of_each_side() {
    // a: 600/1000 = 600 ‰; b: 700/2000 = 350 ‰. b is 250 behind, then wins.
    let mut t = tally(2);
    t.cross(
        0,
        1,
        &log(
            1000,
            2000,
            vec![
                hit(1000, Side::B, 600),
                hit(2000, Side::A, 700),
                hit(3000, Side::B, 0),
                end(7000, Some(Side::B), EndReason::Ko),
            ],
        ),
    )
    .unwrap();
    assert_eq!(finish(t, 2).wins_after_deficit, 1);
}

#[test]
fn a_log_without_end_is_an_error() {
    let mut t = tally(2);
    assert_eq!(
        t.cross(0, 1, &log(1000, 1000, vec![FightEvent::Intro { t: 0 }])),
        Err(SimError::LogWithoutEnd)
    );
    assert_eq!(
        t.mirror(&log(1000, 1000, vec![])),
        Err(SimError::LogWithoutEnd)
    );
}

/// 1000 cross fights and 1000 mirrors where every criterion passes with room.
fn passing() -> Report {
    let record = |slug: &str| CharacterRecord {
        slug: slug.to_owned(),
        appearances: 1000,
        wins: 500,
        draws: 0,
    };
    Report {
        seed_hex: seed_hex(&DEFAULT_SEED),
        ruleset_version: 1,
        characters: vec!["aa".to_owned(), "bb".to_owned()],
        pairs: 1,
        seeds_per_pair: 500,
        cross_fights: 1000,
        mirror_fights: 1000,
        per_character: vec![record("aa"), record("bb")],
        matchups: vec![Matchup {
            first: "aa".to_owned(),
            second: "bb".to_owned(),
            fights: 1000,
            wins_first: 500,
            wins_second: 500,
            draws: 0,
        }],
        mirrors: Mirrors {
            fights: 1000,
            wins_a: 500,
            wins_b: 500,
            draws: 0,
        },
        endings: Endings {
            ko: 1000,
            decision: 0,
            draw: 0,
        },
        time_up: 0,
        duration: Duration {
            p10_ms: 50_000,
            p50_ms: 60_000,
            p90_ms: 70_000,
            in_band: 1000,
        },
        moments: Moments {
            any: 1000,
            comeback: 1000,
            decisive_crit: 1000,
            clutch_ko: 1000,
        },
        deficit_permille: 250,
        wins_after_deficit: 0,
        knockdown: 0,
        density: Density {
            resolutions_p10: 40,
            resolutions_p50: 50,
            resolutions_p90: 60,
            gap_total_ms: 1_200_000,
            gaps: 1000,
            mean_gap_ms: 1200,
            min_gap_ms: 700,
            burst_gaps: 300,
            pause_gaps: 30,
            pauses_p10: 1,
            pauses_p50: 3,
            pauses_p90: 5,
            fights_with_pause: 1000,
        },
    }
}

fn failures_with(change: impl FnOnce(&mut Report)) -> Vec<Failure> {
    let mut r = passing();
    change(&mut r);
    r.failures(&Criteria::RF17)
}

#[test]
fn the_criteria_are_the_ones_of_rf17() {
    let c = Criteria::RF17;
    assert_eq!(
        (c.win_rate_min_permille, c.win_rate_max_permille),
        (400, 600)
    );
    assert_eq!(c.any_moment_min_permille, 150);
    assert_eq!(c.in_band_min_permille, 900);
    assert_eq!(DURATION_BAND_MS, (45_000, 90_000));
    assert_eq!((c.mean_gap_min_ms, c.mean_gap_max_ms), (1100, 1300));
    assert_eq!(c.burst_min_permille, 250);
    assert_eq!(c.pauses_p50_min, 2);
    assert_eq!(c.paused_fights_min_permille, 900);
    assert_eq!((BURST_GAP_MAX_MS, PAUSE_GAP_MIN_MS), (900, 2500));
    assert_eq!(TIMELINE_CONTRACT.strike_gap_ms, 600);
    assert_eq!(c.time_up_max_permille, 100);
    assert_eq!(c.draw_below_permille, 10);
    assert_eq!(
        (c.mirror_side_a_min_permille, c.mirror_side_a_max_permille),
        (470, 530)
    );
    assert_eq!(c.moment_kind_min_permille, 30);
    assert!(c.moment_kind_fails, "RF17 as amended: a dead kind fails");
    assert_eq!(
        (c.matchup_warn_min_permille, c.matchup_warn_max_permille),
        (300, 700)
    );
    assert_eq!(passing().failures(&c), vec![]);
    assert_eq!(passing().warnings(&c), vec![]);
}

#[test]
fn win_rate_edges() {
    let set = |wins: u32, draws: u32| {
        failures_with(|r| {
            r.per_character[0].wins = wins;
            r.per_character[0].draws = draws;
        })
    };
    assert_eq!(set(400, 0), vec![]);
    assert_eq!(set(399, 2), vec![]); // 800 half points of 2000
    assert_eq!(
        set(399, 1),
        vec![Failure::WinRate {
            slug: "aa".to_owned(),
            permille: 399,
        }]
    );
    assert_eq!(set(600, 0), vec![]);
    assert_eq!(
        set(600, 1),
        vec![Failure::WinRate {
            slug: "aa".to_owned(),
            permille: 600,
        }]
    );
}

#[test]
fn global_criteria_edges() {
    assert_eq!(failures_with(|r| r.moments.any = 150), vec![]);
    assert_eq!(
        failures_with(|r| r.moments.any = 149),
        vec![Failure::AnyMoment { permille: 149 }]
    );
    assert_eq!(failures_with(|r| r.duration.in_band = 900), vec![]);
    assert_eq!(
        failures_with(|r| r.duration.in_band = 899),
        vec![Failure::DurationInBand { permille: 899 }]
    );
    assert_eq!(failures_with(|r| r.time_up = 100), vec![]);
    assert_eq!(
        failures_with(|r| r.time_up = 101),
        vec![Failure::TimeUp { permille: 101 }]
    );
    assert_eq!(failures_with(|r| r.endings.draw = 9), vec![]);
    assert_eq!(
        failures_with(|r| r.endings.draw = 10),
        vec![Failure::Draw { permille: 10 }]
    );
}

#[test]
fn pacing_criteria_edges() {
    let mean = |total: u64| failures_with(|r| r.density.gap_total_ms = total);
    assert_eq!(mean(1_100_000), vec![]);
    assert_eq!(mean(1_300_000), vec![]);
    // The reported mean is informative; the band is compared by cross-multiplication.
    assert_eq!(
        failures_with(|r| {
            r.density.gap_total_ms = 1_099_999;
            r.density.mean_gap_ms = 1099;
        }),
        vec![Failure::MeanGap { ms: 1099 }]
    );
    assert_eq!(
        failures_with(|r| {
            r.density.gap_total_ms = 1_300_001;
            r.density.mean_gap_ms = 1300;
        }),
        vec![Failure::MeanGap { ms: 1300 }]
    );
    assert_eq!(failures_with(|r| r.density.burst_gaps = 250), vec![]);
    assert_eq!(
        failures_with(|r| r.density.burst_gaps = 249),
        vec![Failure::BurstShare { permille: 249 }]
    );
    assert_eq!(failures_with(|r| r.density.pauses_p50 = 2), vec![]);
    assert_eq!(
        failures_with(|r| r.density.pauses_p50 = 1),
        vec![Failure::PausesMedian { count: 1 }]
    );
    assert_eq!(failures_with(|r| r.density.fights_with_pause = 900), vec![]);
    assert_eq!(
        failures_with(|r| r.density.fights_with_pause = 899),
        vec![Failure::PausedFights { permille: 899 }]
    );
    assert_eq!(failures_with(|r| r.density.min_gap_ms = 600), vec![]);
    assert_eq!(
        failures_with(|r| r.density.min_gap_ms = 599),
        vec![Failure::StrikeGap { ms: 599 }]
    );
}

#[test]
fn mirror_side_a_edges() {
    let set = |wins_a: u32, draws: u32| {
        failures_with(|r| {
            r.mirrors.wins_a = wins_a;
            r.mirrors.draws = draws;
        })
    };
    assert_eq!(set(470, 0), vec![]);
    assert_eq!(set(469, 2), vec![]);
    assert_eq!(set(469, 1), vec![Failure::MirrorSideA { permille: 469 }]);
    assert_eq!(set(530, 0), vec![]);
    assert_eq!(set(530, 1), vec![Failure::MirrorSideA { permille: 530 }]);
}

#[test]
fn every_moment_kind_needs_3_percent() {
    for kind in MOMENT_KINDS {
        let set = |count: u32| {
            failures_with(|r| match kind {
                MomentKind::Comeback => r.moments.comeback = count,
                MomentKind::DecisiveCrit => r.moments.decisive_crit = count,
                MomentKind::ClutchKo => r.moments.clutch_ko = count,
            })
        };
        assert_eq!(set(30), vec![], "{kind:?}");
        assert_eq!(
            set(29),
            vec![Failure::MomentKind { kind, permille: 29 }],
            "{kind:?}"
        );
    }
}

#[test]
fn a_dead_kind_only_warns_if_the_criterion_is_switched_off() {
    let mut r = passing();
    r.moments.clutch_ko = 0;
    let lenient = Criteria {
        moment_kind_fails: false,
        ..Criteria::RF17
    };
    assert_eq!(r.failures(&lenient), vec![]);
    assert_eq!(
        r.warnings(&lenient),
        vec![Warning::MomentKind {
            kind: MomentKind::ClutchKo,
            permille: 0,
        }]
    );
    assert_eq!(r.warnings(&Criteria::RF17), vec![]);
}

#[test]
fn matchup_warning_edges_never_fail() {
    for (wins_first, warns) in [(290, true), (300, false), (700, false), (710, true)] {
        let mut r = passing();
        r.matchups[0].wins_first = wins_first;
        r.matchups[0].wins_second = 1000 - wins_first;
        let warnings = r.warnings(&Criteria::RF17);
        assert_eq!(!warnings.is_empty(), warns, "{wins_first}");
        if warns {
            assert_eq!(
                warnings,
                vec![Warning::Matchup {
                    first: "aa".to_owned(),
                    second: "bb".to_owned(),
                    permille: wins_first,
                }]
            );
        }
        assert_eq!(r.failures(&Criteria::RF17), vec![], "{wins_first}");
    }
}

#[test]
fn the_text_report_is_english_and_lists_failures_and_warnings() {
    let ok = passing().render_text(&Criteria::RF17);
    assert!(ok.contains("RF17: all criteria pass"), "{ok}");
    assert!(
        ok.contains("  aa   50.0%  (500 wins, 0 draws, 1000 fights)"),
        "{ok}"
    );
    assert!(!ok.contains("warnings:"), "{ok}");
    let mut r = passing();
    r.time_up = 101;
    r.matchups[0].wins_first = 710;
    r.matchups[0].wins_second = 290;
    let text = r.render_text(&Criteria::RF17);
    assert!(text.contains("RF17: 1 criteria fail:"), "{text}");
    assert!(text.contains("time_up: 10.1% (must be <= 10.0%)"), "{text}");
    assert!(
        text.contains("matchup: aa wins 71.0% against bb (outside 30.0%..=70.0%)"),
        "{text}"
    );
    assert!(text.contains(&r.seed_hex), "{text}");
    assert!(
        ok.contains(
            "pacing: min gap 700 ms; burst (<= 900 ms) 30.0% of gaps; pauses (>= 2500 ms) 3.0% \
             of gaps, per fight p10 1, p50 3, p90 5; fights with a pause 100.0%"
        ),
        "{ok}"
    );
    let mut r = passing();
    r.density.burst_gaps = 0;
    r.density.min_gap_ms = 500;
    let text = r.render_text(&Criteria::RF17);
    assert!(
        text.contains("burst_share: 0.0% (must be >= 25.0%)"),
        "{text}"
    );
    assert!(
        text.contains("strike_gap: 500 ms (must be >= 600 ms)"),
        "{text}"
    );
    for f in [
        Failure::MeanGap { ms: 1400 },
        Failure::PausesMedian { count: 1 },
        Failure::PausedFights { permille: 800 },
    ] {
        let line = describe_failure(&f, &Criteria::RF17);
        assert!(
            [
                "mean_gap: 1400 ms (must be 1100..=1300 ms)",
                "pauses_median: 1 (must be >= 2)",
                "paused_fights: 80.0% (must be >= 90.0%)",
            ]
            .contains(&line.as_str()),
            "{line}"
        );
    }
}

#[test]
fn the_json_of_failures_has_a_code() {
    let json = serde_json::to_string(&Failure::MomentKind {
        kind: MomentKind::DecisiveCrit,
        permille: 12,
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"code":"moment_kind","kind":"decisive_crit","permille":12}"#
    );
}
