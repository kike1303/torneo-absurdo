//! The player fights (feature 021, phase 2, TA-63): "buscar pelea" resolves an attack at once with
//! the engine and the player ruleset (v2) against the botarga of another user, connected or not, or
//! against a character of the tournament when there is nobody. Its own tables (`pvp_fights`,
//! `pvp_daily`): it never touches `fights`, the scheduler nor the settlement of the tournament.
//!
//! Every draw of chance of the server (the defender, the character of the tournament and the seed)
//! comes from `OsRng`, never from the PRNG of the engine. The coins of an attack move only as a
//! `pvp_reward` entry of the ledger.

pub mod api;
pub mod matchmaking;
pub mod resolve;
pub mod rewards;
pub mod rival;
pub mod season;
pub mod store;

use std::path::PathBuf;
use std::sync::Arc;

use engine::CharacterSpec;
use rand_core::{OsRng, TryRngCore};
use tokio::sync::OnceCell;

use crate::ledger::Coins;

/// `PVP_WIN_COINS` and what else the server validates at startup (021 "Números").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvpConfig {
    /// The coins of a victory of the attacker against a user, at full pay. 0 turns them off.
    pub win_coins: Coins,
}

impl PvpConfig {
    pub const DEFAULT_WIN_COINS: i64 = 15;
    /// A typo must not inflate the economy: at most a fifth of the largest `DAILY_GRANT`.
    pub const WIN_COINS_RANGE: std::ops::RangeInclusive<i64> = 0..=2000;

    /// The coins as the rewards compute them.
    #[must_use]
    pub fn win_coins(self) -> u32 {
        u32::try_from(self.win_coins.0).unwrap_or(0)
    }
}

impl Default for PvpConfig {
    fn default() -> Self {
        Self {
            win_coins: Coins(Self::DEFAULT_WIN_COINS),
        }
    }
}

/// The characters of the tournament as rivals of last resort: the roster of `CONTENT_DIR`, read
/// before the first search outside any transaction, and kept for the life of the process like the
/// roster of the scheduler once it loads. A directory that does not load gives no character (and
/// `pvp.no_rival`) until it does.
#[derive(Debug)]
pub struct Rivals {
    dir: PathBuf,
    specs: OnceCell<Arc<[CharacterSpec]>>,
}

impl Rivals {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            specs: OnceCell::new(),
        }
    }

    /// The specs of the roster, in the order of `load_roster` (by slug). A roster that loads is
    /// kept for the life of the process; one that fails is not, so the next search reads it again
    /// (at most one per quota of `POST /api/pvp/fights`).
    pub async fn specs(&self) -> Arc<[CharacterSpec]> {
        let loaded = self
            .specs
            .get_or_try_init(|| async {
                let dir = self.dir.clone();
                let loaded = tokio::task::spawn_blocking(move || {
                    crate::fights::roster::rival_specs(&dir).map_err(|error| error.kind)
                })
                .await;
                match loaded {
                    Ok(Ok(specs)) => Ok(Arc::<[CharacterSpec]>::from(specs)),
                    Ok(Err(error_kind)) => Err(error_kind),
                    Err(_) => Err("load_panicked"),
                }
            })
            .await;
        match loaded {
            Ok(specs) => specs.clone(),
            Err(error_kind) => {
                tracing::warn!(event = "pvp.rivals_invalid", error_kind);
                Arc::from([])
            }
        }
    }
}

/// A uniform number in `0..n` from `OsRng`, by rejection (no modulo bias). `None` with `n = 0` or
/// without entropy.
#[must_use]
pub fn uniform(n: u64) -> Option<u64> {
    if n == 0 {
        return None;
    }
    // The largest multiple of `n` that fits: draws at or above it are thrown away.
    let zone = u64::MAX - (u64::MAX % n);
    loop {
        let draw = OsRng.try_next_u64().ok()?;
        if draw < zone {
            return Some(draw % n);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn uniform_stays_in_range() {
        assert_eq!(uniform(0), None);
        assert_eq!(uniform(1), Some(0));
        let mut seen = [false; 3];
        for _ in 0..200 {
            let draw = uniform(3).unwrap();
            seen[usize::try_from(draw).unwrap()] = true;
        }
        assert_eq!(seen, [true; 3]);
    }

    #[tokio::test]
    async fn the_rivals_of_a_missing_directory_are_none() {
        let rivals = Rivals::new(PathBuf::from("/nonexistent/content/characters"));
        assert!(rivals.specs().await.is_empty());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/characters");
        assert!(!Rivals::new(dir).specs().await.is_empty());
    }
}
