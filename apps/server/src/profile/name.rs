//! `display_name` validation and `display_name_key` (ADR-0008 D1, 008 RF-B7).
//!
//! In this order: NFC, trim, refuse control (`Cc`), bidi and zero-width characters, 3 to 24
//! graphemes, then the key (UTS #39 skeleton over full case folding over NFKC) and the
//! reserved names by key, ignoring spaces, `_`, `-` and `.`. Uniqueness is the database's `UNIQUE` on the key, never a prior
//! check here.

use std::collections::HashSet;
use std::sync::LazyLock;

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub const MIN_GRAPHEMES: usize = 3;
pub const MAX_GRAPHEMES: usize = 24;

/// Why a name is refused. Maps to `profile.display_name_invalid` and
/// `profile.display_name_reserved` when the rename arrives (phase C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Invalid,
    Reserved,
}

/// A valid display name and its comparison key. No `Display`: it is user data and never goes
/// to a log (RF-B17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName {
    text: String,
    key: String,
}

impl DisplayName {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// `display_name_key`: what the `UNIQUE` index and the reserved list compare.
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// The reserved names of `content/names/reserved.txt`, as [`reserved_form`]s.
static RESERVED: LazyLock<HashSet<String>> = LazyLock::new(|| {
    words(include_str!("../../../../content/names/reserved.txt"))
        .map(|word| reserved_form(&key_of(word)))
        .collect()
});

/// The form compared against the reserved list: the key without spaces, `_`, `-` or `.`, so
/// "Torneo Absurdo", "torneo.absurdo" and "Torneo_Absurdo" are all the brand (review of #54).
/// Only for the reserved list: the `UNIQUE` key keeps them, so "Ana Bel" and "AnaBel" are
/// still two names.
fn reserved_form(key: &str) -> String {
    key.chars()
        .filter(|c| !matches!(c, ' ' | '_' | '-' | '.'))
        .collect()
}

/// The non-comment, non-empty lines of a word list.
pub(crate) fn words(text: &'static str) -> impl Iterator<Item = &'static str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// Bidi formatting (U+061C, U+200E–U+200F, U+202A–U+202E, U+2066–U+2069) and zero-width
/// (U+200B–U+200D, U+2060, U+FEFF) characters of D1, plus every `Cc`.
fn forbidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}'
                | '\u{200E}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200B}'..='\u{200D}'
                | '\u{2060}'
                | '\u{FEFF}'
        )
}

/// `display_name_key`: the UTS #39 skeleton of the full case folding of the NFKC form. So
/// `Tоrneo` (Cyrillic о), `TORNEO` and `torneo` share a key.
///
/// One pass is not idempotent (the skeleton of `%` is `º/₀`, which NFKC changes again), and the
/// spec requires the key to be. So the pass repeats until it stops changing: deterministic, and
/// it only joins more confusable names, never splits them. It converges in a few passes; the
/// bound only guards against a pathological input.
pub fn key_of(text: &str) -> String {
    let mut key = key_pass(text);
    for _ in 0..KEY_MAX_PASSES {
        let next = key_pass(&key);
        if next == key {
            break;
        }
        key = next;
    }
    key
}

const KEY_MAX_PASSES: usize = 16;

fn key_pass(text: &str) -> String {
    let nfkc: String = text.nfkc().collect();
    let folded = caseless::default_case_fold_str(&nfkc);
    unicode_security::skeleton(&folded).collect()
}

pub fn validate(input: &str) -> Result<DisplayName, NameError> {
    let nfc: String = input.nfc().collect();
    let text = nfc.trim();
    if text.chars().any(forbidden) {
        return Err(NameError::Invalid);
    }
    let graphemes = text.graphemes(true).count();
    if !(MIN_GRAPHEMES..=MAX_GRAPHEMES).contains(&graphemes) {
        return Err(NameError::Invalid);
    }
    let key = key_of(text);
    if RESERVED.contains(&reserved_form(&key)) {
        return Err(NameError::Reserved);
    }
    Ok(DisplayName {
        text: text.to_owned(),
        key,
    })
}

#[cfg(test)]
#[path = "name_tests.rs"]
mod tests;
