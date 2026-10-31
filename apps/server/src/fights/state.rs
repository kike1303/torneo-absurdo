//! A row of `fights` and the pure decision of what to do with it (RF14, RF15). `next_action` knows
//! the row, the time and three facts of the runner: nothing of the log, so a decision never
//! depends on how the fight ends.

use protocol::{
    CancelReason, EndReason, FightFinal, FightState, FightSummary, FighterCard, Side, TimestampMs,
    VoidReason,
};

use super::snapshot::FightSnapshot;
use super::{LATE_START_WAIT_MS, MAX_MIN_OPEN_WINDOW_MS, ON_TIME_MARGIN_MS, PLAYBACK_DELAY_MS};

/// A row of `fights`, with its hours in milliseconds since the epoch. No `Debug`: a settled row
/// carries the log.
#[derive(Clone)]
pub struct FightRow {
    pub id: i32,
    pub state: FightState,
    pub opens_at: i64,
    pub closes_at: i64,
    pub live_at: i64,
    pub ruleset_version: i16,
    pub spec_a: Vec<u8>,
    pub spec_b: Vec<u8>,
    pub snapshot_sha256: Vec<u8>,
    pub slug_a: String,
    pub slug_b: String,
    pub max_hp_a: i32,
    pub max_hp_b: i32,
    pub time_limit_ms: i32,
    pub max_duration_ms: i32,
    pub matchup_round: i32,
    pub roster_sha256: Vec<u8>,
    pub seed_commit: Option<Vec<u8>>,
    pub opened_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub live_started_at: Option<i64>,
    pub settled_at: Option<i64>,
    pub winner: Option<String>,
    pub end_reason: Option<String>,
    pub cancel_reason: Option<String>,
    pub void_reason: Option<String>,
    pub revealed_seed: Option<Vec<u8>>,
    pub log: Option<Vec<u8>>,
}

impl FightRow {
    /// The final of a settled row, or `None` (any other row, or values outside the closed lists
    /// that the `CHECK`s of the table forbid).
    pub fn final_result(&self) -> Option<FightFinal> {
        if let Some(reason) = &self.end_reason {
            let reason = match reason.as_str() {
                "ko" => EndReason::Ko,
                "decision" => EndReason::Decision,
                "draw" => EndReason::Draw,
                _ => return None,
            };
            let winner = match self.winner.as_deref() {
                None => None,
                Some("a") => Some(Side::A),
                Some("b") => Some(Side::B),
                Some(_) => return None,
            };
            return Some(FightFinal::Decided { winner, reason });
        }
        if let Some(reason) = &self.cancel_reason {
            return Some(FightFinal::Cancelled {
                reason: CancelReason::parse(reason)?,
            });
        }
        if let Some(reason) = &self.void_reason {
            return Some(FightFinal::Voided {
                reason: VoidReason::parse(reason)?,
            });
        }
        None
    }

    /// What everyone may know of this fight, or `None` with values the `CHECK`s of the table
    /// forbid.
    pub fn summary(&self) -> Option<FightSummary> {
        let settled_with_final = self.state != FightState::Settled || self.final_result().is_some();
        settled_with_final.then_some(())?;
        Some(FightSummary {
            id: u32::try_from(self.id).ok()?,
            state: self.state,
            a: FighterCard {
                slug: self.slug_a.clone(),
                max_hp: u16::try_from(self.max_hp_a).ok()?,
            },
            b: FighterCard {
                slug: self.slug_b.clone(),
                max_hp: u16::try_from(self.max_hp_b).ok()?,
            },
            ruleset_version: u16::try_from(self.ruleset_version).ok()?,
            snapshot_sha256: super::hex(&self.snapshot_sha256),
            seed_commit: self.seed_commit.as_deref().map(super::hex),
            opens_at: TimestampMs::from_ms(self.opens_at),
            closes_at: TimestampMs::from_ms(self.closes_at),
            live_at: TimestampMs::from_ms(self.live_at),
            live_started_at: self.live_started_at.map(TimestampMs::from_ms),
            time_limit_ms: u32::try_from(self.time_limit_ms).ok()?,
            max_duration_ms: u32::try_from(self.max_duration_ms).ok()?,
            playback_delay_ms: PLAYBACK_DELAY_MS,
            final_result: self.final_result(),
        })
    }
}

/// What the runner knows beyond the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// Another fight is in `live`.
    pub other_live: bool,
    /// The fights are halted (RF16).
    pub halted: bool,
    /// The sealed log of this fight is in memory.
    pub log_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Nothing until this hour, unless something else changes.
    Wait(i64),
    /// Nothing until something else changes.
    Idle,
    Open,
    Cancel(CancelReason),
    Close,
    /// Rebuild the sealed log.
    Recompute,
    GoLive {
        started_at_ms: i64,
    },
}

/// The window a fight needs left to open late: `min(60 s, window / 2)`, in milliseconds (amendment
/// DP-1). From the hours of the row, so a change of configuration never touches an announced fight.
pub const fn min_open_window_ms(opens_at: i64, closes_at: i64) -> i64 {
    let half = closes_at.saturating_sub(opens_at) / 2;
    if half < MAX_MIN_OPEN_WINDOW_MS {
        half
    } else {
        MAX_MIN_OPEN_WINDOW_MS
    }
}

/// The table of RF15, with the amendments DP-1 and DP-2.
pub fn next_action(row: &FightRow, now_ms: i64, context: &Context) -> Action {
    match row.state {
        FightState::Scheduled => {
            if now_ms < row.opens_at {
                return Action::Wait(row.opens_at);
            }
            let min_open = min_open_window_ms(row.opens_at, row.closes_at);
            if row.closes_at.saturating_sub(now_ms) < min_open {
                return Action::Cancel(CancelReason::MissedWindow);
            }
            if context.halted {
                // Until it loses its window (one millisecond later it has less than the minimum).
                return Action::Wait(row.closes_at.saturating_sub(min_open).saturating_add(1));
            }
            if FightSnapshot::from_row(row).is_err() {
                return Action::Cancel(CancelReason::FightUnrunnable);
            }
            Action::Open
        }
        FightState::BettingOpen => {
            if now_ms < row.closes_at {
                Action::Wait(row.closes_at)
            } else {
                Action::Close
            }
        }
        FightState::BettingClosed => {
            if !context.log_ready {
                return Action::Recompute;
            }
            if context.halted {
                return Action::Idle;
            }
            if now_ms < row.live_at {
                return Action::Wait(row.live_at);
            }
            if context.other_live {
                return Action::Idle;
            }
            let started_at_ms = if now_ms <= row.live_at.saturating_add(ON_TIME_MARGIN_MS) {
                row.live_at
            } else {
                now_ms.saturating_add(LATE_START_WAIT_MS)
            };
            Action::GoLive { started_at_ms }
        }
        FightState::Live => {
            if context.log_ready {
                Action::Idle
            } else {
                Action::Recompute
            }
        }
        FightState::Settled => Action::Idle,
    }
}

/// The slot the next announcement goes to: the next multiple of the cadence strictly after `now`.
/// A row exists for it as soon as `now >= opens_at - cadence`, and that is this same formula, so a
/// server that just started and one that keeps running do the same (RF11, RF12).
pub fn target_slot(now_ms: i64, cadence_ms: i64) -> i64 {
    now_ms
        .div_euclid(cadence_ms)
        .saturating_add(1)
        .saturating_mul(cadence_ms)
}

#[cfg(test)]
pub(super) mod tests {
    use engine::{Ruleset, canonical_spec_json};
    use protocol::FightState::{BettingClosed, BettingOpen, Live, Scheduled, Settled};

    use super::*;
    use crate::fights::snapshot::{max_duration_ms, max_hp, snapshot_sha256};

    const OPENS: i64 = 1_000_000;
    const CLOSES: i64 = OPENS + 450_000;
    const LIVE_AT: i64 = CLOSES + 30_000;

    fn spec(slug: &str) -> engine::CharacterSpec {
        let text = include_str!("../../../../crates/engine/tests/golden/spec-striker.json");
        let mut spec: engine::CharacterSpec = serde_json::from_str(text).unwrap();
        spec.slug = slug.to_owned();
        spec
    }

    pub(crate) fn row(state: FightState) -> FightRow {
        let ruleset = Ruleset::for_version(1).unwrap();
        let (a, b) = (spec("x-a"), spec("x-b"));
        let (spec_a, spec_b) = (
            canonical_spec_json(&a).unwrap(),
            canonical_spec_json(&b).unwrap(),
        );
        FightRow {
            id: 1,
            state,
            opens_at: OPENS,
            closes_at: CLOSES,
            live_at: LIVE_AT,
            ruleset_version: 1,
            snapshot_sha256: snapshot_sha256(1, &spec_a, &spec_b).to_vec(),
            spec_a,
            spec_b,
            slug_a: "x-a".to_owned(),
            slug_b: "x-b".to_owned(),
            max_hp_a: i32::from(max_hp(&ruleset, &a).unwrap()),
            max_hp_b: i32::from(max_hp(&ruleset, &b).unwrap()),
            time_limit_ms: i32::try_from(ruleset.time_limit_ms()).unwrap(),
            max_duration_ms: i32::try_from(max_duration_ms(&ruleset).unwrap()).unwrap(),
            matchup_round: 1,
            roster_sha256: vec![0; 32],
            seed_commit: None,
            opened_at: None,
            closed_at: None,
            live_started_at: None,
            settled_at: None,
            winner: None,
            end_reason: None,
            cancel_reason: None,
            void_reason: None,
            revealed_seed: None,
            log: None,
        }
    }

    const CLEAR: Context = Context {
        other_live: false,
        halted: false,
        log_ready: true,
    };
    const HALTED: Context = Context {
        halted: true,
        ..CLEAR
    };
    const NO_LOG: Context = Context {
        log_ready: false,
        ..CLEAR
    };
    const OTHER_LIVE: Context = Context {
        other_live: true,
        ..CLEAR
    };

    /// The table of RF15, row by row, with the amendments DP-1 and DP-2.
    #[test]
    fn the_table_of_rf15() {
        let min = 60_000;
        let cases: &[(&str, FightState, i64, Context, Action)] = &[
            (
                "scheduled, before",
                Scheduled,
                OPENS - 1,
                CLEAR,
                Action::Wait(OPENS),
            ),
            (
                "scheduled, before, halted",
                Scheduled,
                OPENS - 1,
                HALTED,
                Action::Wait(OPENS),
            ),
            ("scheduled, on time", Scheduled, OPENS, CLEAR, Action::Open),
            (
                "scheduled, late with window",
                Scheduled,
                CLOSES - min,
                CLEAR,
                Action::Open,
            ),
            (
                "scheduled, under the minimum",
                Scheduled,
                CLOSES - min + 1,
                CLEAR,
                Action::Cancel(CancelReason::MissedWindow),
            ),
            (
                "scheduled, under the minimum, halted",
                Scheduled,
                CLOSES - min + 1,
                HALTED,
                Action::Cancel(CancelReason::MissedWindow),
            ),
            (
                "scheduled, after the close",
                Scheduled,
                CLOSES + 1,
                CLEAR,
                Action::Cancel(CancelReason::MissedWindow),
            ),
            (
                "scheduled, halted with window",
                Scheduled,
                OPENS,
                HALTED,
                Action::Wait(CLOSES - min + 1),
            ),
            (
                "open, before the close",
                BettingOpen,
                CLOSES - 1,
                CLEAR,
                Action::Wait(CLOSES),
            ),
            (
                "open, before the close, halted",
                BettingOpen,
                CLOSES - 1,
                HALTED,
                Action::Wait(CLOSES),
            ),
            (
                "open, at the close",
                BettingOpen,
                CLOSES,
                CLEAR,
                Action::Close,
            ),
            (
                "open, at the close, halted",
                BettingOpen,
                CLOSES,
                HALTED,
                Action::Close,
            ),
            (
                "open, long after",
                BettingOpen,
                CLOSES + 9_000_000,
                CLEAR,
                Action::Close,
            ),
            (
                "closed, without log",
                BettingClosed,
                CLOSES,
                NO_LOG,
                Action::Recompute,
            ),
            (
                "closed, without log, halted",
                BettingClosed,
                CLOSES,
                Context {
                    halted: true,
                    ..NO_LOG
                },
                Action::Recompute,
            ),
            (
                "closed, halted",
                BettingClosed,
                LIVE_AT,
                HALTED,
                Action::Idle,
            ),
            // DP-2: it waits for live_at, it never skips pre_live.
            (
                "closed, before live_at",
                BettingClosed,
                LIVE_AT - 1,
                CLEAR,
                Action::Wait(LIVE_AT),
            ),
            (
                "closed, at live_at",
                BettingClosed,
                LIVE_AT,
                CLEAR,
                Action::GoLive {
                    started_at_ms: LIVE_AT,
                },
            ),
            (
                "closed, one second late",
                BettingClosed,
                LIVE_AT + 1000,
                CLEAR,
                Action::GoLive {
                    started_at_ms: LIVE_AT,
                },
            ),
            (
                "closed, later",
                BettingClosed,
                LIVE_AT + 1001,
                CLEAR,
                Action::GoLive {
                    started_at_ms: LIVE_AT + 1001 + 10_000,
                },
            ),
            (
                "closed, another in live",
                BettingClosed,
                LIVE_AT,
                OTHER_LIVE,
                Action::Idle,
            ),
            (
                "live, without log",
                Live,
                LIVE_AT,
                NO_LOG,
                Action::Recompute,
            ),
            ("live, with log", Live, LIVE_AT, CLEAR, Action::Idle),
            ("settled", Settled, LIVE_AT, CLEAR, Action::Idle),
        ];
        for (name, state, now, context, action) in cases {
            assert_eq!(next_action(&row(*state), *now, context), *action, "{name}");
        }
    }

    /// DP-1: with a short window a fight opens, and the minimum is computed in milliseconds.
    #[test]
    fn the_minimum_window_follows_a_short_window() {
        assert_eq!(min_open_window_ms(0, 450_000), 60_000);
        assert_eq!(min_open_window_ms(0, 120_000), 60_000);
        assert_eq!(min_open_window_ms(0, 119_998), 59_999);
        assert_eq!(min_open_window_ms(0, 5000), 2500);
        assert_eq!(min_open_window_ms(0, 1000), 500);
        let mut short = row(Scheduled);
        short.closes_at = OPENS + 1000;
        short.live_at = OPENS + 1000;
        assert_eq!(next_action(&short, OPENS, &CLEAR), Action::Open);
        assert_eq!(next_action(&short, OPENS + 500, &CLEAR), Action::Open);
        assert_eq!(
            next_action(&short, OPENS + 501, &CLEAR),
            Action::Cancel(CancelReason::MissedWindow)
        );
    }

    #[test]
    fn a_scheduled_fight_that_cannot_run_is_cancelled_before_any_commit() {
        let unrunnable = Action::Cancel(CancelReason::FightUnrunnable);
        let mut version = row(Scheduled);
        version.ruleset_version = 999;
        assert_eq!(next_action(&version, OPENS, &CLEAR), unrunnable);
        let mut corrupt = row(Scheduled);
        corrupt.spec_a = b"{".to_vec();
        assert_eq!(next_action(&corrupt, OPENS, &CLEAR), unrunnable);
        // M6: a spec that decodes, hashes well and does not pass `validate`.
        let mut invalid_spec = spec("x-a");
        invalid_spec.stats.hp = 0;
        let mut invalid = row(Scheduled);
        invalid.spec_a = canonical_spec_json(&invalid_spec).unwrap();
        invalid.snapshot_sha256 = snapshot_sha256(1, &invalid.spec_a, &invalid.spec_b).to_vec();
        let ruleset = Ruleset::for_version(1).unwrap();
        invalid.max_hp_a = i32::from(max_hp(&ruleset, &invalid_spec).unwrap());
        assert_eq!(
            FightSnapshot::from_row(&invalid).err(),
            Some(crate::fights::snapshot::SnapshotError::SpecInvalid)
        );
        assert_eq!(next_action(&invalid, OPENS, &CLEAR), unrunnable);
        // With a halt it waits: the halt may be what makes it look unrunnable.
        assert!(matches!(
            next_action(&invalid, OPENS, &HALTED),
            Action::Wait(_)
        ));
    }

    #[test]
    fn every_check_of_the_snapshot_has_its_code() {
        use crate::fights::snapshot::SnapshotError as E;
        let check = |change: fn(&mut FightRow), expected: E| {
            let mut bad = row(Scheduled);
            change(&mut bad);
            assert_eq!(
                FightSnapshot::from_row(&bad).err(),
                Some(expected),
                "{expected:?}"
            );
        };
        assert!(FightSnapshot::from_row(&row(Scheduled)).is_ok());
        check(|r| r.ruleset_version = 999, E::VersionMissing);
        check(|r| r.ruleset_version = -1, E::VersionMissing);
        check(|r| r.spec_b = vec![0xff], E::Decode);
        check(
            |r| {
                let spaced = [b" ".as_slice(), &r.spec_a].concat();
                r.spec_a = spaced;
            },
            E::Reserialize,
        );
        check(|r| r.snapshot_sha256 = vec![0; 32], E::Hash);
        check(|r| r.slug_a = "other".to_owned(), E::Slug);
        check(|r| r.slug_b = "other".to_owned(), E::Slug);
        check(|r| r.max_hp_a += 1, E::MaxHp);
        check(|r| r.max_hp_b += 1, E::MaxHp);
        check(|r| r.time_limit_ms += 1, E::Bounds);
        check(|r| r.max_duration_ms += 1, E::Bounds);
    }

    #[test]
    fn the_target_slot_is_the_next_multiple_of_the_cadence() {
        assert_eq!(target_slot(0, 600), 600);
        assert_eq!(target_slot(1, 600), 600);
        assert_eq!(target_slot(599, 600), 600);
        assert_eq!(target_slot(600, 600), 1200);
        assert_eq!(target_slot(1_700_000_000_000, 600_000), 1_700_000_400_000);
    }

    #[test]
    fn a_summary_has_the_finals_of_the_row() {
        let summary = row(Scheduled).summary().unwrap();
        assert_eq!(summary.final_result, None);
        assert_eq!(summary.playback_delay_ms, PLAYBACK_DELAY_MS);
        assert_eq!(summary.max_duration_ms, 90_000);

        let mut draw = row(Settled);
        draw.end_reason = Some("draw".to_owned());
        assert_eq!(
            draw.summary().unwrap().final_result,
            Some(FightFinal::Decided {
                winner: None,
                reason: EndReason::Draw
            })
        );
        let mut win = row(Settled);
        win.end_reason = Some("ko".to_owned());
        win.winner = Some("b".to_owned());
        assert_eq!(
            win.summary().unwrap().final_result,
            Some(FightFinal::Decided {
                winner: Some(Side::B),
                reason: EndReason::Ko
            })
        );
        let mut cancelled = row(Settled);
        cancelled.cancel_reason = Some("fight_unrunnable".to_owned());
        assert_eq!(
            cancelled.summary().unwrap().final_result,
            Some(FightFinal::Cancelled {
                reason: CancelReason::FightUnrunnable
            })
        );
        let mut voided = row(Settled);
        voided.void_reason = Some("snapshot_invalid".to_owned());
        assert_eq!(
            voided.summary().unwrap().final_result,
            Some(FightFinal::Voided {
                reason: VoidReason::SnapshotInvalid
            })
        );
        // Settled without a final: only with the guards off.
        assert!(row(Settled).summary().is_none());
    }
}
