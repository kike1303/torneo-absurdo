//! The `Timeline` contract seen from the engine (006 RF5).
//!
//! A mirror of `TIMELINE_LIMITS` in `packages/renderer/src/timeline/types.ts`, whose owners are
//! 002 and 005. It is **not** a game constant nor a field of [`crate::Ruleset`]: the simulation
//! never reads it and it changes no byte of a log. The ruleset bounds test and the test
//! invariants check against it, and `crates/sim` measures against it.
//!
//! Tie: `tests/golden.rs` writes it to `tests/golden/timeline-contract.json` and a Vitest compares
//! that file key by key with `TIMELINE_LIMITS`, so changing one side alone fails `just check`.
use serde::Serialize;

/// The rules of the renderer's `Timeline` that every engine log has to respect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TimelineContract {
    /// The first strike lands at least this long after the intro.
    pub intro_gap_ms: u32,
    /// Minimum separation between two consecutive strikes.
    pub strike_gap_ms: u32,
    /// Shortest knockdown.
    pub knockdown_min_ms: u32,
    /// Longest knockdown.
    pub knockdown_max_ms: u32,
    /// From the KO to the win cue.
    pub win_gap_ms: u32,
    /// Length of the win pose.
    pub win_pose_ms: u32,
}

/// The current values of `TIMELINE_LIMITS`.
pub const TIMELINE_CONTRACT: TimelineContract = TimelineContract {
    intro_gap_ms: 2000,
    strike_gap_ms: 600,
    knockdown_min_ms: 800,
    knockdown_max_ms: 3000,
    win_gap_ms: 1500,
    win_pose_ms: 2000,
};
