//! The roster of the new fights (RF22): loaded once at startup, and the only caller of
//! `load_roster`. A fight never runs from here: it runs from its own snapshot (RF5).

use std::path::Path;

use content::load_roster;
use engine::{Ruleset, canonical_spec_json};

use super::RULESET_VERSION;
use super::matchup::roster_sha256;
use super::snapshot::{max_duration_ms, max_hp, snapshot_sha256};
use super::state::target_slot;
use super::store::{self, NewFight};
use super::{FightsConfig, matchup};

/// A character, ready to be written in a row.
pub struct RosterEntry {
    pub slug: String,
    pub spec_json: Vec<u8>,
    pub max_hp: i32,
}

pub struct Roster {
    /// Sorted by slug.
    entries: Vec<RosterEntry>,
    slugs: Vec<String>,
    sha256: [u8; 32],
    time_limit_ms: i32,
    max_duration_ms: i32,
}

/// Why there is no roster, by code. Fights already created go on (RF22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the roster could not be loaded")]
pub struct RosterError {
    pub kind: &'static str,
}

impl Roster {
    /// Loads and converts the roster.
    ///
    /// # Errors
    /// [`RosterError`] with a code from the closed lists of `content` and of the engine.
    pub fn load_blocking(dir: &Path) -> Result<Self, RosterError> {
        let invalid = |kind: &'static str| RosterError { kind };
        let ruleset =
            Ruleset::for_version(RULESET_VERSION).ok_or_else(|| invalid("version_missing"))?;
        let characters = load_roster(dir, &ruleset).map_err(|error| invalid(error.code()))?;
        if characters.len() < 2 {
            return Err(invalid("too_few_characters"));
        }
        let mut entries = Vec::with_capacity(characters.len());
        for character in &characters {
            let spec = character.spec();
            entries.push(RosterEntry {
                slug: spec.slug.clone(),
                spec_json: canonical_spec_json(spec).map_err(|_| invalid("encode_failed"))?,
                max_hp: i32::from(max_hp(&ruleset, spec).ok_or_else(|| invalid("max_hp"))?),
            });
        }
        let slugs: Vec<String> = entries.iter().map(|entry| entry.slug.clone()).collect();
        Ok(Self {
            sha256: roster_sha256(&slugs),
            slugs,
            entries,
            time_limit_ms: i32::try_from(ruleset.time_limit_ms()).map_err(|_| invalid("bounds"))?,
            max_duration_ms: max_duration_ms(&ruleset)
                .and_then(|ms| i32::try_from(ms).ok())
                .ok_or_else(|| invalid("bounds"))?,
        })
    }

    /// Loads it off the runtime, and logs a failure.
    pub async fn load(dir: &Path) -> Option<Self> {
        let dir = dir.to_owned();
        let loaded = tokio::task::spawn_blocking(move || Self::load_blocking(&dir)).await;
        let error_kind = match loaded {
            Ok(Ok(roster)) => return Some(roster),
            Ok(Err(error)) => error.kind,
            Err(_) => "load_panicked",
        };
        tracing::error!(event = "fights.roster_invalid", error_kind);
        None
    }

    pub fn slugs(&self) -> &[String] {
        &self.slugs
    }

    fn entry(&self, slug: &str) -> Option<&RosterEntry> {
        self.entries.iter().find(|entry| entry.slug == slug)
    }

    /// Announces the fight of the slot after `now_ms`: its pair and its snapshot, in one statement,
    /// before any seed exists (RF1, RF12, RF20). `Ok(None)` if the slot already has one.
    ///
    /// # Errors
    /// The database error.
    pub async fn announce(
        &self,
        pool: &sqlx::PgPool,
        config: &FightsConfig,
        now_ms: i64,
        roll: u32,
    ) -> Result<Option<i32>, sqlx::Error> {
        let context = store::matchup_context(pool).await?;
        let new_roster = context
            .last_roster
            .as_deref()
            .is_some_and(|last| last != self.sha256.as_slice());
        // A new roster opens a new round, so a new character does not hoard its end.
        let (used, round_base): (&[(String, String)], i32) = if new_roster {
            (&[], context.round.saturating_add(1))
        } else {
            (&context.used, context.round.max(1))
        };
        let Some(pick) = matchup::pick_matchup(&self.slugs, used, &context.recent, roll) else {
            return Ok(None);
        };
        let round = if pick.new_round {
            round_base.saturating_add(1)
        } else {
            round_base
        };
        let (Some(a), Some(b)) = (self.entry(pick.a), self.entry(pick.b)) else {
            return Ok(None);
        };
        let opens_at = target_slot(now_ms, config.cadence_ms);
        let closes_at = opens_at.saturating_add(config.window_ms);
        store::insert(
            pool,
            &NewFight {
                opens_at,
                closes_at,
                live_at: closes_at.saturating_add(config.pre_live_ms),
                ruleset_version: i16::try_from(RULESET_VERSION)
                    .map_err(|_| sqlx::Error::Protocol("ruleset_version".to_owned()))?,
                spec_a: &a.spec_json,
                spec_b: &b.spec_json,
                snapshot_sha256: &snapshot_sha256(RULESET_VERSION, &a.spec_json, &b.spec_json),
                slug_a: &a.slug,
                slug_b: &b.slug,
                max_hp_a: a.max_hp,
                max_hp_b: b.max_hp,
                time_limit_ms: self.time_limit_ms,
                max_duration_ms: self.max_duration_ms,
                matchup_round: round,
                roster_sha256: &self.sha256,
            },
        )
        .await
    }
}
