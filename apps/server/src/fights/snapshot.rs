//! The snapshot of a fight (RF1, RF2, RF5): what the fight was announced with, before its seed
//! existed. The engine only ever runs from these bytes, never from `content/`.

use engine::{CharacterSpec, Ruleset, canonical_spec_json};
use sha2::{Digest, Sha256};

use super::state::FightRow;

/// `SHA-256({"ruleset_version":N,"a":<spec_a>,"b":<spec_b>})`, with these exact bytes and no
/// spaces, so any language reproduces it by concatenation (RF2).
pub fn snapshot_sha256(ruleset_version: u16, spec_a: &[u8], spec_b: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"{\"ruleset_version\":");
    hasher.update(ruleset_version.to_string().as_bytes());
    hasher.update(b",\"a\":");
    hasher.update(spec_a);
    hasher.update(b",\"b\":");
    hasher.update(spec_b);
    hasher.update(b"}");
    hasher.finalize().into()
}

/// `max_hp` of the summary: what the engine writes in the log (a test ties them together).
pub fn max_hp(ruleset: &Ruleset, spec: &CharacterSpec) -> Option<u16> {
    let hp = ruleset
        .hp_per_point()
        .checked_mul(u32::from(spec.stats.hp))?
        .checked_add(ruleset.hp_base())?;
    u16::try_from(hp).ok()
}

/// The constant upper bound of a fight's duration under `ruleset` (RF41).
pub fn max_duration_ms(ruleset: &Ruleset) -> Option<u32> {
    ruleset.time_limit_ms().checked_add(ruleset.end_ms())
}

/// Why a row cannot run. A code, never a value of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    #[error("the ruleset version is not in this binary")]
    VersionMissing,
    #[error("a spec does not decode")]
    Decode,
    #[error("a spec does not reserialize to its stored bytes")]
    Reserialize,
    #[error("the snapshot hash does not match")]
    Hash,
    #[error("a slug differs from its spec")]
    Slug,
    #[error("a max_hp differs from its spec")]
    MaxHp,
    #[error("the bounds differ from the ruleset")]
    Bounds,
    /// A spec that decodes and does not pass `validate` (amendment M6). Its own variant: RF59 and
    /// the logs tell it apart from a corrupt snapshot and from a failure of the engine.
    #[error("a spec does not pass validation")]
    SpecInvalid,
}

impl SnapshotError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::VersionMissing => "version_missing",
            Self::Decode => "snapshot_decode",
            Self::Reserialize => "snapshot_reserialize",
            Self::Hash => "snapshot_hash",
            Self::Slug => "snapshot_slug",
            Self::MaxHp => "snapshot_max_hp",
            Self::Bounds => "snapshot_bounds",
            Self::SpecInvalid => "spec_invalid",
        }
    }
}

/// The inputs of the engine, built only from a row. Holding one means every check of RF5 passed.
pub struct FightSnapshot {
    a: CharacterSpec,
    b: CharacterSpec,
    ruleset: Ruleset,
}

impl FightSnapshot {
    /// Checks a row against itself and its ruleset. Pure: a decision can rely on it (RF14).
    ///
    /// # Errors
    /// The first [`SnapshotError`] found, in the order of RF5.
    pub fn from_row(row: &FightRow) -> Result<Self, SnapshotError> {
        let version =
            u16::try_from(row.ruleset_version).map_err(|_| SnapshotError::VersionMissing)?;
        let ruleset = Ruleset::for_version(version).ok_or(SnapshotError::VersionMissing)?;
        let a = decode(&row.spec_a)?;
        let b = decode(&row.spec_b)?;
        if snapshot_sha256(version, &row.spec_a, &row.spec_b).as_slice() != row.snapshot_sha256 {
            return Err(SnapshotError::Hash);
        }
        if a.slug != row.slug_a || b.slug != row.slug_b {
            return Err(SnapshotError::Slug);
        }
        if max_hp(&ruleset, &a).map(i32::from) != Some(row.max_hp_a)
            || max_hp(&ruleset, &b).map(i32::from) != Some(row.max_hp_b)
        {
            return Err(SnapshotError::MaxHp);
        }
        if i64::from(ruleset.time_limit_ms()) != i64::from(row.time_limit_ms)
            || max_duration_ms(&ruleset).map(i64::from) != Some(i64::from(row.max_duration_ms))
        {
            return Err(SnapshotError::Bounds);
        }
        if a.validate(&ruleset).is_err() || b.validate(&ruleset).is_err() {
            return Err(SnapshotError::SpecInvalid);
        }
        Ok(Self { a, b, ruleset })
    }

    pub(super) const fn parts(&self) -> (&CharacterSpec, &CharacterSpec, &Ruleset) {
        (&self.a, &self.b, &self.ruleset)
    }
}

fn decode(bytes: &[u8]) -> Result<CharacterSpec, SnapshotError> {
    let spec: CharacterSpec = serde_json::from_slice(bytes).map_err(|_| SnapshotError::Decode)?;
    let again = canonical_spec_json(&spec).map_err(|_| SnapshotError::Reserialize)?;
    if again != bytes {
        return Err(SnapshotError::Reserialize);
    }
    Ok(spec)
}
