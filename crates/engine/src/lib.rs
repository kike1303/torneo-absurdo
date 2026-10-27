//! Deterministic fight engine (ADR-0001, feature 003).
//!
//! [`simulate`] turns a [`Seed`], two [`CharacterSpec`]s and a [`Ruleset`] into a
//! [`FightLog`]: the same inputs always give the same bytes of [`canonical_json`], natively
//! and in WASM.
//!
//! Purity contract (RF5), enforced by the lints below, `clippy.toml` and
//! `scripts/check-engine.mjs`:
//! - no I/O, clock, threads nor logging;
//! - no `HashMap`/`HashSet`, no floats, no `rand` and no `getrandom`: the only randomness is
//!   ChaCha20 seeded with the [`Seed`], consumed in the order of RF7;
//! - integer arithmetic only, in the order of the ruleset table;
//! - no panics with untrusted input (RF21): every input is validated, and the log never
//!   contains the seed (RF20).
#![deny(
    clippy::float_arithmetic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

mod encode;
mod rng;
mod ruleset;
mod seed;
mod sim;
mod spec;

pub use encode::{EncodeError, canonical_json, canonical_spec_json};
pub use protocol::{AbilityKind, EndReason, FightEvent, FightLog, FighterInfo, MomentKind, Side};
pub use ruleset::Ruleset;
pub use seed::Seed;
pub use spec::{
    Ability, CharacterSpec, Element, MoveKind, Personality, SpecError, Stats, is_valid_slug,
};

/// Validates both characters against the ruleset and simulates the fight.
///
/// # Errors
/// The first [`SpecError`] of `a`, then of `b`. With valid characters and a ruleset that passes
/// the bounds test the simulation itself does not fail (`SpecError::EventLimit` is a guard).
pub fn simulate(
    seed: &Seed,
    a: &CharacterSpec,
    b: &CharacterSpec,
    ruleset: &Ruleset,
) -> Result<FightLog, SpecError> {
    a.validate(ruleset)?;
    b.validate(ruleset)?;
    sim::run(&mut rng::Roller::from_seed(seed), a, b, ruleset)
}
