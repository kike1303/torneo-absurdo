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

/// Numbers with a loaded meaning, never in an alias (game-designer on #54): 69, 88 (a neo-Nazi
/// code, next to `Vikingo`), 420 and 666.
pub const SKIPPED_NUMBERS: &[u32] = &[69, 88, 420, 666];

/// [`NUMBERS`] without [`SKIPPED_NUMBERS`]: the numbers an alias draws from.
pub static ALIAS_NUMBERS: LazyLock<Vec<u32>> =
    LazyLock::new(|| NUMBERS.filter(|n| !SKIPPED_NUMBERS.contains(n)).collect());

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

/// The alias source of the server, in `Settings` (not an environment variable): [`OsAlias`] by
/// default, a fixed list in the tests.
#[derive(Clone)]
pub struct AliasConfig(pub std::sync::Arc<dyn AliasSource>);

impl AliasConfig {
    pub fn new(source: std::sync::Arc<dyn AliasSource>) -> Self {
        Self(source)
    }
}

impl Default for AliasConfig {
    fn default() -> Self {
        Self(std::sync::Arc::new(OsAlias))
    }
}

impl std::fmt::Debug for AliasConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AliasConfig(..)")
    }
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
        let number = *ALIAS_NUMBERS
            .get(usize::try_from(uniform_index(len_of(&ALIAS_NUMBERS)?, &mut draw)?).ok()?)?;
        Some(compose(noun, adjective, number))
    }
}

fn len(list: &[&str]) -> Option<u32> {
    len_of(list)
}

fn len_of<T>(list: &[T]) -> Option<u32> {
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
        let combinations = NOUNS.len() * ADJECTIVES.len() * ALIAS_NUMBERS.len();
        // The space of the spec (≥ 4 M) and of game-designer's review (≈ 19.8 M), after the
        // skipped numbers.
        assert!(combinations >= 19_800_000, "{combinations}");
    }

    #[test]
    fn the_counts_stay_after_the_review_swaps() {
        assert_eq!(NOUNS.len(), 281);
        assert_eq!(ADJECTIVES.len(), 123);
    }

    /// game-designer on #54: words that form franchise names, regional vulgarities or insults
    /// against a person are out of the lists.
    #[test]
    fn the_removed_words_stay_out() {
        for word in [
            "Linterna",
            "Flecha",
            "Duende",
            "Escarabajo",
            "Pantera",
            "Chile",
            "Papaya",
            "Maraca",
            "Mono",
            "Gorila",
            "Foca",
            "Morsa",
            "Ballena",
            "Paloma",
            "Perico",
            "Concha",
            "Ogro",
        ] {
            assert!(!NOUNS.contains(&word), "{word}");
        }
        for word in ["Ardiente", "Mejor"] {
            assert!(!ADJECTIVES.contains(&word), "{word}");
        }
    }

    /// No noun + adjective of the lists may form the Spanish name of a protected character
    /// (ADR-0004). A future edit that brings one back fails here.
    #[test]
    fn no_blocked_pair_is_complete() {
        const BLOCKED_PAIRS: &[(&str, &str)] = &[
            ("Linterna", "Verde"),
            ("Flecha", "Verde"),
            ("Duende", "Verde"),
            ("Escarabajo", "Azul"),
            ("Pantera", "Rosa"),
            ("Ogro", "Verde"),
        ];
        for (noun, adjective) in BLOCKED_PAIRS {
            assert!(
                !(NOUNS.contains(noun) && ADJECTIVES.contains(adjective)),
                "{noun}{adjective}"
            );
        }
    }

    #[test]
    fn the_alias_numbers_skip_the_loaded_ones() {
        for number in SKIPPED_NUMBERS {
            assert!(NUMBERS.contains(number), "{number}");
            assert!(!ALIAS_NUMBERS.contains(number), "{number}");
        }
        assert_eq!(ALIAS_NUMBERS.len(), 990 - SKIPPED_NUMBERS.len());
        assert!(ALIAS_NUMBERS.iter().all(|n| NUMBERS.contains(n)));
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
            assert!(ALIAS_NUMBERS.contains(&number), "{alias}");
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
