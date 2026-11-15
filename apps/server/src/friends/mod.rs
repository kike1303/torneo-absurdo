//! Friends, invites and challenges (feature 023, phase 1, TA-86). Separate from the ranked PvP of
//! 021: its own tables, read by nothing of the matchmaking, the probation, the ranking or
//! `farm-report`, and no function here moves rating, XP, coins or `pvp_daily`. The actor is the only
//! row locked with a wait (the botarga of a friend is read without a lock, never written), so a
//! challenge never waits on a fight nor on the close of a season.

pub mod api;
pub mod store;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand_core::{OsRng, TryRngCore};

/// Friends of one user at most (both sides are counted when accepting).
pub const MAX_FRIENDS: i64 = 200;
/// Challenges of one user per UTC day (owner, 2026-11-15).
pub const DAILY_CHALLENGES: i16 = 50;
/// Characters of an invite code: 16 bytes in base64url without padding.
pub const CODE_LEN: usize = 22;

/// A new invite code: 128 bits of `OsRng`. `None` when the system has no entropy.
#[must_use]
pub fn new_code() -> Option<String> {
    let mut bytes = [0_u8; 16];
    OsRng.try_fill_bytes(&mut bytes).ok()?;
    Some(URL_SAFE_NO_PAD.encode(bytes))
}

/// Whether `code` has the form of a code: checked at the edge, before any query.
#[must_use]
pub fn well_formed(code: &str) -> bool {
    code.len() == CODE_LEN
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The `display_name_key` of a name as a friend list shows it, or `None` for a text no name can
/// be. Not `validate`: a name saved under older rules still has to be found by its key.
#[must_use]
pub fn name_key(display_name: &str) -> Option<String> {
    let text = display_name.trim();
    if text.is_empty() || text.len() > 256 {
        return None;
    }
    Some(crate::profile::name::key_of(text))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_code_has_one_form() {
        let code = new_code().unwrap();
        assert_eq!(code.len(), CODE_LEN);
        assert!(well_formed(&code));
        assert_ne!(new_code().unwrap(), code);
        for bad in [
            "",
            "short",
            &"A".repeat(23),
            &"A".repeat(21),
            "AAAAAAAAAAAAAAAAAAAAA=",
            "AAAAAAAAAAAAAAAAAAAAA/",
        ] {
            assert!(!well_formed(bad), "{bad}");
        }
    }

    #[test]
    fn a_name_key_ignores_case() {
        assert_eq!(name_key("Ana Pérez"), name_key("ana pérez"));
        assert_eq!(name_key("  "), None);
        assert_eq!(name_key(&"x".repeat(300)), None);
    }
}
