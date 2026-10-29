//! The generated alias: noun + adjective + number from 10 to 999 (ADR-0008 D1, 008 RF-B7).
//!
//! Spanish, noun first and only gender-invariable adjectives, so the agreement is always right.
//! Every user sees the same alias in en and es: it is user data, not UI text (ADR-0007). The
//! lists live in `content/names/` and game-designer reviews them.

use std::ops::RangeInclusive;
use std::sync::LazyLock;

use rand_core::{OsRng, TryRngCore};

use super::name::words;

pub const NUMBERS: RangeInclusive<u32> = 10..=999;

pub static NOUNS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| words(include_str!("../../../../content/names/nouns.txt")).collect());

pub static ADJECTIVES: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| words(include_str!("../../../../content/names/adjectives.txt")).collect());

/// Where new aliases come from. Production uses [`OsAlias`]; the tests inject a fixed one to
/// force collisions (T20).
pub trait AliasSource: Send + Sync {
    /// A new candidate, or `None` if the randomness source failed.
    fn next(&self) -> Option<String>;
}

/// Aliases from `OsRng` (never the engine's PRNG, ADR-0001).
#[derive(Debug, Clone, Copy, Default)]
pub struct OsAlias;

impl AliasSource for OsAlias {
    fn next(&self) -> Option<String> {
        let mut draw = || OsRng.try_next_u32().ok();
        let noun = NOUNS.get(usize::try_from(uniform_index(len(&NOUNS)?, &mut draw)?).ok()?)?;
        let adjective =
            ADJECTIVES.get(usize::try_from(uniform_index(len(&ADJECTIVES)?, &mut draw)?).ok()?)?;
        let span = NUMBERS.end() - NUMBERS.start() + 1;
        let number = NUMBERS.start() + uniform_index(span, &mut draw)?;
        Some(compose(noun, adjective, number))
    }
}

fn len(list: &[&str]) -> Option<u32> {
    u32::try_from(list.len()).ok()
}

pub fn compose(noun: &str, adjective: &str, number: u32) -> String {
    format!("{noun}{adjective}{number}")
}

/// An index in `0..n` without modulo bias: draws in the incomplete last block of `2^32` are
/// rejected and drawn again (at most 64 times; `None` if `draw` fails or `n` is 0).
pub fn uniform_index(n: u32, mut draw: impl FnMut() -> Option<u32>) -> Option<u32> {
    if n == 0 {
        return None;
    }
    let zone = (u32::MAX / n) * n;
    for _ in 0..64 {
        let value = draw()?;
        if value < zone {
            return Some(value % n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::profile::name::{self, NameError};

    #[test]
    fn the_lists_are_large_enough() {
        assert!(NOUNS.len() >= 200, "{}", NOUNS.len());
        assert!(ADJECTIVES.len() >= 100, "{}", ADJECTIVES.len());
        let combinations = NOUNS.len() * ADJECTIVES.len() * NUMBERS.clone().count();
        assert!(combinations >= 4_000_000, "{combinations}");
    }

    #[test]
    fn words_are_capitalized_ascii_and_short() {
        for word in NOUNS.iter().chain(ADJECTIVES.iter()) {
            assert!((3..=10).contains(&word.len()), "{word}");
            let mut chars = word.chars();
            assert!(
                chars.next().is_some_and(|c| c.is_ascii_uppercase()),
                "{word}"
            );
            assert!(chars.all(|c| c.is_ascii_lowercase()), "{word}");
        }
    }

    #[test]
    fn no_duplicates_even_by_key_and_no_reserved_word() {
        let mut keys = HashSet::new();
        for word in NOUNS.iter().chain(ADJECTIVES.iter()) {
            assert!(keys.insert(name::key_of(word)), "duplicate {word}");
            assert_ne!(
                name::validate(word).err(),
                Some(NameError::Reserved),
                "{word}"
            );
        }
    }

    #[test]
    fn the_extreme_combinations_are_valid_names() {
        let by_len = |list: &[&'static str]| {
            let shortest = list
                .iter()
                .min_by_key(|w| w.len())
                .copied()
                .unwrap_or_default();
            let longest = list
                .iter()
                .max_by_key(|w| w.len())
                .copied()
                .unwrap_or_default();
            [shortest, longest]
        };
        for noun in by_len(&NOUNS) {
            for adjective in by_len(&ADJECTIVES) {
                for number in [*NUMBERS.start(), *NUMBERS.end()] {
                    let alias = compose(noun, adjective, number);
                    assert!(name::validate(&alias).is_ok(), "{alias}");
                }
            }
        }
    }

    #[test]
    fn os_aliases_have_the_format() {
        for _ in 0..200 {
            let alias = OsAlias.next().unwrap();
            assert!(name::validate(&alias).is_ok(), "{alias}");
            let digits: String = alias.chars().filter(char::is_ascii_digit).collect();
            let number: u32 = digits.parse().unwrap();
            assert!(NUMBERS.contains(&number), "{alias}");
            assert!(alias.ends_with(&digits), "{alias}");
        }
    }

    #[test]
    fn uniform_index_rejects_the_biased_tail() {
        // With n = 3, values from the incomplete last block of 2^32 are drawn again.
        let tail = u32::MAX;
        let mut draws = [tail, 4].into_iter();
        assert_eq!(uniform_index(3, || draws.next()), Some(1));
        let mut draws = [7].into_iter();
        assert_eq!(uniform_index(3, || draws.next()), Some(1));
        let mut none = std::iter::empty();
        assert_eq!(uniform_index(3, || none.next()), None);
        assert_eq!(uniform_index(0, || Some(1)), None);
    }
}
