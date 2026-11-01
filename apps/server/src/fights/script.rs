//! `dev-fights` (RF45): a list of fights, with their seeds, that replaces `OsRng` and the matchups, to
//! force a victory and a draw with the golden vectors of 003. This module does not exist in a release:
//! it is the only one that names its variable, and `check-release-binary` looks for that name in the
//! binaries.
//!
//! A row is chosen by the rank of the fight in its table, `rank % rows` (amendment DP-B6). Fights are
//! never deleted, so the rank survives a restart, and a counter in memory would separate the seed from
//! the specs.
//!
//! The seeds of a script are for development, but their way out is as closed as the real one: no
//! `Debug` nor `Serialize` here, no error keeps the text of `serde_json`, which can quote a value, and
//! this module writes no log line.

use engine::{CharacterSpec, Ruleset, canonical_spec_json};
use serde::Deserialize;

use super::matchup::roster_sha256;
use super::roster::RosterEntry;
use super::snapshot::{max_duration_ms, max_hp, snapshot_sha256};
use super::state::target_slot;
use super::store::{self, NewFight};
use super::{FightsConfig, RULESET_VERSION};

/// The variable with the path of the script.
pub const VAR: &str = "FIGHT_SCRIPT_PATH";

/// Why a script cannot be used, with nothing of its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the fight script is not valid")]
pub struct ScriptError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    seed: String,
    spec_a: CharacterSpec,
    spec_b: CharacterSpec,
}

struct ScriptRow {
    seed: [u8; 32],
    a: RosterEntry,
    b: RosterEntry,
}

pub struct FightScript {
    rows: Vec<ScriptRow>,
    sha256: [u8; 32],
    time_limit_ms: i32,
    max_duration_ms: i32,
}

fn seed_of(text: &str) -> Result<[u8; 32], ScriptError> {
    let digit = |byte: u8| {
        char::from(byte)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return Err(ScriptError);
    }
    let mut seed = [0_u8; 32];
    let (pairs, _) = bytes.as_chunks::<2>();
    for (out, [high, low]) in seed.iter_mut().zip(pairs) {
        *out = digit(*high)
            .zip(digit(*low))
            .map(|(high, low)| high * 16 + low)
            .ok_or(ScriptError)?;
    }
    Ok(seed)
}

fn entry_of(spec: &CharacterSpec, ruleset: &Ruleset) -> Result<RosterEntry, ScriptError> {
    spec.validate(ruleset).map_err(|_| ScriptError)?;
    Ok(RosterEntry {
        slug: spec.slug.clone(),
        spec_json: canonical_spec_json(spec).map_err(|_| ScriptError)?,
        max_hp: i32::from(max_hp(ruleset, spec).ok_or(ScriptError)?),
    })
}

impl FightScript {
    /// The script in these bytes: at least one row, with each spec valid for today's ruleset.
    ///
    /// # Errors
    /// [`ScriptError`], whatever it was: the error of `serde_json` is dropped right here.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ScriptError> {
        let raw: Vec<RawRow> = serde_json::from_slice(bytes).map_err(|_| ScriptError)?;
        let ruleset = Ruleset::for_version(RULESET_VERSION).ok_or(ScriptError)?;
        let mut rows = Vec::with_capacity(raw.len());
        let mut slugs = Vec::new();
        for row in &raw {
            let (a, b) = (
                entry_of(&row.spec_a, &ruleset)?,
                entry_of(&row.spec_b, &ruleset)?,
            );
            slugs.extend([a.slug.clone(), b.slug.clone()]);
            rows.push(ScriptRow {
                seed: seed_of(&row.seed)?,
                a,
                b,
            });
        }
        if rows.is_empty() {
            return Err(ScriptError);
        }
        slugs.sort_unstable();
        slugs.dedup();
        Ok(Self {
            rows,
            sha256: roster_sha256(&slugs),
            time_limit_ms: i32::try_from(ruleset.time_limit_ms()).map_err(|_| ScriptError)?,
            max_duration_ms: max_duration_ms(&ruleset)
                .and_then(|ms| i32::try_from(ms).ok())
                .ok_or(ScriptError)?,
        })
    }

    /// The script at this path.
    ///
    /// # Errors
    /// [`ScriptError`] if it cannot be read or is not valid.
    pub fn read(path: &str) -> Result<Self, ScriptError> {
        let bytes = std::fs::read(path).map_err(|_| ScriptError)?;
        Self::from_json(&bytes)
    }

    fn row(&self, rank: i64) -> Option<&ScriptRow> {
        let count = i64::try_from(self.rows.len()).ok()?;
        let index = usize::try_from(rank.checked_rem_euclid(count)?).ok()?;
        self.rows.get(index)
    }

    /// The seed of the fight with this rank, if its specs are the ones of its row: a fight announced
    /// before the script was configured gets its seed from `OsRng`.
    pub(super) fn seed_for(&self, rank: i64, spec_a: &[u8], spec_b: &[u8]) -> Option<[u8; 32]> {
        self.row(rank)
            .filter(|row| row.a.spec_json == spec_a && row.b.spec_json == spec_b)
            .map(|row| row.seed)
    }

    /// Announces the fight of the slot after `now_ms`, with the next row. `Ok(None)` if the slot
    /// already has one.
    pub(super) async fn announce(
        &self,
        pool: &sqlx::PgPool,
        config: &FightsConfig,
        now_ms: i64,
    ) -> Result<Option<i32>, sqlx::Error> {
        let rank = store::fight_rank(pool, i32::MAX).await?;
        let Some(ScriptRow { a, b, .. }) = self.row(rank) else {
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
                matchup_round: 1,
                roster_sha256: &self.sha256,
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_A: &str = include_str!("../../../../crates/engine/tests/golden/spec-striker.json");
    const SPEC_B: &str = include_str!("../../../../crates/engine/tests/golden/spec-tank.json");
    const SEED: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn row(seed: &str) -> String {
        format!(r#"{{"seed":"{seed}","spec_a":{SPEC_A},"spec_b":{SPEC_B}}}"#)
    }

    #[test]
    fn a_script_gives_its_rows_in_a_cycle_and_only_for_their_specs() {
        let script = FightScript::from_json(format!("[{}]", row(SEED)).as_bytes()).unwrap();
        let a = script.rows[0].a.spec_json.clone();
        let b = script.rows[0].b.spec_json.clone();
        let seed = script.seed_for(0, &a, &b).unwrap();
        assert_eq!(seed[0], 0x00);
        assert_eq!(seed[15], 0xff);
        assert_eq!(script.seed_for(7, &a, &b), Some(seed));
        assert_eq!(script.seed_for(0, &b, &a), None);
    }

    #[test]
    fn whatever_is_not_a_script_is_one_error_with_nothing_in_it() {
        let bad_spec = format!(r#"[{{"seed":"{SEED}","spec_a":{{}},"spec_b":{SPEC_B}}}]"#);
        let unknown =
            format!(r#"[{{"seed":"{SEED}","spec_a":{SPEC_A},"spec_b":{SPEC_B},"extra":1}}]"#);
        for text in [
            String::new(),
            "[]".to_owned(),
            "{".to_owned(),
            format!("[{}]", row("00")),
            format!("[{}]", row(&"zz".repeat(32))),
            format!("[{}]", row(&"é".repeat(32))),
            bad_spec,
            unknown,
        ] {
            assert_eq!(
                FightScript::from_json(text.as_bytes()).err(),
                Some(ScriptError)
            );
        }
        assert_eq!(ScriptError.to_string(), "the fight script is not valid");
        assert_eq!(
            FightScript::read("/nonexistent/script.json").err(),
            Some(ScriptError)
        );
    }
}
