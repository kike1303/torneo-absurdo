//! The only file of the player fights that draws a seed and runs the engine (021 step 4), so the
//! source guard of the tournament (010 RF24, `tests/fights_source_guard.rs`) names a single
//! exception. A player fight has no secret to keep: its seed is public from the moment it exists,
//! because nobody bets on it (ADR-0002 is of the tournament). What leaves here is bytes to store
//! and the result; nothing here logs.

use engine::{CharacterSpec, EndReason, Ruleset, Seed, Side, canonical_json, canonical_spec_json};
use rand_core::{OsRng, TryRngCore};

/// A fight the engine resolved, as `pvp_fights` stores it.
pub struct Resolved {
    pub seed: [u8; 32],
    pub spec_a: Vec<u8>,
    pub spec_b: Vec<u8>,
    pub log: Vec<u8>,
    pub winner: Option<Side>,
    pub end_reason: EndReason,
}

/// A new seed from `OsRng`, the fight of `a` against `b` with `ruleset`, and its canonical bytes.
///
/// # Errors
/// A code for the log: `os_rng`, a code of `SpecError`, `no_end` or `encode_failed`.
pub fn resolve(
    a: &CharacterSpec,
    b: &CharacterSpec,
    ruleset: &Ruleset,
) -> Result<Resolved, &'static str> {
    let mut seed = [0_u8; 32];
    OsRng.try_fill_bytes(&mut seed).map_err(|_| "os_rng")?;
    let log = engine::simulate(&Seed::from_bytes(seed), a, b, ruleset).map_err(|e| e.code())?;
    let (winner, end_reason) = log.outcome().ok_or("no_end")?;
    let encode = |_| "encode_failed";
    Ok(Resolved {
        seed,
        spec_a: canonical_spec_json(a).map_err(encode)?,
        spec_b: canonical_spec_json(b).map_err(encode)?,
        log: canonical_json(&log).map_err(encode)?,
        winner,
        end_reason,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::botarga::{BotargaCatalog, build};

    #[test]
    fn two_fights_have_two_seeds_and_each_replays_to_its_log() {
        let catalog = BotargaCatalog::embedded().unwrap();
        let starter = catalog.starter();
        let spec = build::resolve(
            &catalog,
            1,
            &protocol::BotargaBuild {
                stats: protocol::BotargaStats {
                    hp: starter.stats.hp,
                    attack: starter.stats.attack,
                    defense: starter.stats.defense,
                    speed: starter.stats.speed,
                    special: starter.stats.special,
                },
                style_id: starter.style_id.clone(),
                abilities: vec![starter.ability_id.clone()],
                accessories: Vec::new(),
                cosmetics: protocol::FighterCosmetics::default(),
            },
        )
        .unwrap();
        let ruleset = catalog.ruleset();
        let one = resolve(&spec, &spec, ruleset).unwrap();
        let two = resolve(&spec, &spec, ruleset).unwrap();
        assert_ne!(one.seed, two.seed);
        let again = engine::simulate(&Seed::from_bytes(one.seed), &spec, &spec, ruleset).unwrap();
        assert_eq!(canonical_json(&again).unwrap(), one.log);
        assert_eq!(one.spec_a, one.spec_b);
    }
}
