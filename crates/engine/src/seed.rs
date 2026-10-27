//! The fight seed (ADR-0002, 003 RF4, RF20).
use std::fmt;

/// 32 bytes that seed the fight PRNG.
///
/// Built only with [`Seed::from_bytes`]. It is deliberately not `Clone`, `Serialize`,
/// `Display` nor hex-formattable, and it never exposes its bytes: the server keeps its own copy
/// for the commit and the database. `Debug` prints exactly `Seed(<redacted>)`.
pub struct Seed([u8; 32]);

impl Seed {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Only to seed the PRNG (`rng.rs`).
    pub(crate) const fn bytes(&self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for Seed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seed(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let seed = Seed::from_bytes([0xab; 32]);
        assert_eq!(format!("{seed:?}"), "Seed(<redacted>)");
        assert_eq!(format!("{seed:#?}"), "Seed(<redacted>)");
    }
}
