//! Friends, invites and challenges (feature 023, phase 1, TA-86). Separate from the ranked PvP of
//! 021: its own tables, read by nothing of the matchmaking, the probation, the ranking or
//! `farm-report`, and no function here moves rating, XP, coins or `pvp_daily`. The `users` row of the
//! actor is the only row locked with a wait; no botarga is locked, not even the actor's, because
//! the foreign keys of a fight between friends take `FOR KEY SHARE` on the botargas it names, and a
//! botarga locked `FOR UPDATE` by one actor would make the crossed fight of another wait on it.

pub mod api;
pub mod store;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::Side;
use rand_core::{OsRng, TryRngCore};

/// Friends of one user at most (both sides are counted when accepting).
pub const MAX_FRIENDS: i64 = 200;
/// Challenges of one user per UTC day (owner, 2026-11-15).
pub const DAILY_CHALLENGES: i16 = 50;
/// Tournaments of one user per UTC day (owner, 2026-11-15).
pub const DAILY_TOURNAMENTS: i16 = 10;
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

/// A uniform index below `n` from `OsRng` (rejection sampling, so no bias). `None` without entropy
/// or with `n == 0`.
fn below(n: u32) -> Option<u32> {
    if n == 0 {
        return None;
    }
    let zone = u32::MAX - u32::MAX % n;
    loop {
        let x = OsRng.try_next_u32().ok()?;
        if x < zone {
            return Some(x % n);
        }
    }
}

/// Shuffles `items` with `OsRng` (Fisher–Yates): the order of a bracket. `None` without entropy.
pub fn shuffle<T>(items: &mut [T]) -> Option<()> {
    for i in (1..items.len()).rev() {
        let j = below(u32::try_from(i + 1).ok()?)?;
        items.swap(i, usize::try_from(j).ok()?);
    }
    Some(())
}

/// Who goes on from a match of a bracket: the winner, or after a draw the side the low bit of the
/// first byte of the public seed says, 0 = `a` (owner, 2026-11-15). The same rule as the `CHECK`
/// of `friend_fights.advances`.
#[must_use]
pub const fn advances(winner: Option<Side>, seed: &[u8; 32]) -> Side {
    match winner {
        Some(side) => side,
        None if seed[0] & 1 == 0 => Side::A,
        None => Side::B,
    }
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
    fn a_draw_goes_by_the_low_bit_of_the_seed() {
        let mut seed = [0_u8; 32];
        assert_eq!(advances(None, &seed), Side::A);
        seed[0] = 0b1010_0001;
        assert_eq!(advances(None, &seed), Side::B);
        assert_eq!(advances(Some(Side::A), &seed), Side::A);
        seed[0] = 0b1111_1110;
        assert_eq!(advances(None, &seed), Side::A);
        assert_eq!(advances(Some(Side::B), &seed), Side::B);
    }

    #[test]
    fn a_shuffle_keeps_every_item() {
        let mut items: Vec<u8> = (0..8).collect();
        shuffle(&mut items).unwrap();
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..8).collect::<Vec<u8>>());
        let mut seen_first = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let mut items: Vec<u8> = (0..4).collect();
            shuffle(&mut items).unwrap();
            seen_first.insert(items[0]);
        }
        assert_eq!(seen_first.len(), 4);
    }

    #[test]
    fn a_name_key_ignores_case() {
        assert_eq!(name_key("Ana Pérez"), name_key("ana pérez"));
        assert_eq!(name_key("  "), None);
        assert_eq!(name_key(&"x".repeat(300)), None);
    }
}
