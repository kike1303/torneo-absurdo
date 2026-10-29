//! `display_name` validation and `display_name_key` (ADR-0008 D1, 008 RF-B7).
//!
//! In this order: NFC, trim, refuse control (`Cc`), the line and paragraph separators and
//! invisible characters (every `Default_Ignorable_Code_Point` and a few other blanks), 3 to 24
//! graphemes, then the key (UTS #39 skeleton over full case folding over NFKC) and the
//! reserved names, compared by two forms built from the letters and digits of the name. Uniqueness is the database's `UNIQUE` on the key, never a prior
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

/// Both [`reserved_forms`] of every name of `content/names/reserved.txt`.
static RESERVED: LazyLock<HashSet<String>> = LazyLock::new(|| {
    words(include_str!("../../../../content/names/reserved.txt"))
        .flat_map(reserved_forms)
        .collect()
});

/// The letters and digits of `text`, without accents (NFD, then `char::is_alphanumeric`).
fn letters(text: &str) -> String {
    text.nfd().filter(|c| c.is_alphanumeric()).collect()
}

/// The two forms compared against the reserved list (review of #54):
/// - `key_of(letters(text))`: no punctuation, space or accent between the letters gets around
///   the list ("Torneo·Absurdo", "Torneo—Absurdo", "Anónimo");
/// - `key_of(letters(key_of(text)))`: a symbol the skeleton reads as a letter still counts as
///   that letter ("Syst℮m", "Sta₽f"); the first form alone would drop it.
///
/// Invariant: a name whose `display_name_key` equals that of a reserved name always has the
/// second form of that reserved name, so this only adds refusals to a plain key match. Only for
/// the reserved list: the `UNIQUE` key keeps everything, so "Ana Bel" and "AnaBel" are still
/// two names.
fn reserved_forms(text: &str) -> [String; 2] {
    [key_of(&letters(text)), key_of(&letters(&key_of(text)))]
}

/// Whether `text` is a reserved name by either form.
pub(crate) fn is_reserved(text: &str) -> bool {
    reserved_forms(text)
        .iter()
        .any(|form| RESERVED.contains(form))
}

/// The non-comment, non-empty lines of a word list.
pub(crate) fn words(text: &'static str) -> impl Iterator<Item = &'static str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// Every `Default_Ignorable_Code_Point` of Unicode (`DerivedCoreProperties.txt`, complete as of
/// Unicode 16): characters that render as nothing, so a name made of them is blank, and one with
/// them looks like another. It contains the bidi formatting and zero-width characters of D1.
const DEFAULT_IGNORABLE: &[(char, char)] = &[
    ('\u{00AD}', '\u{00AD}'),
    ('\u{034F}', '\u{034F}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{115F}', '\u{1160}'),
    ('\u{17B4}', '\u{17B5}'),
    ('\u{180B}', '\u{180F}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{206F}'),
    ('\u{3164}', '\u{3164}'),
    ('\u{FE00}', '\u{FE0F}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFA0}', '\u{FFA0}'),
    ('\u{FFF0}', '\u{FFF8}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0000}', '\u{E0FFF}'),
];

/// Blank characters outside `Default_Ignorable_Code_Point` (security-reviewer on #54): the
/// braille blank, the full and half blanks of Egyptian hieroglyphs and the Khitan filler.
const OTHER_BLANKS: &[char] = &['\u{2800}', '\u{13441}', '\u{13442}', '\u{16FE4}'];

/// A character that shows nothing: a `Default_Ignorable_Code_Point` or one of [`OTHER_BLANKS`].
pub fn is_invisible(c: char) -> bool {
    OTHER_BLANKS.contains(&c)
        || DEFAULT_IGNORABLE
            .iter()
            .any(|(first, last)| (*first..=*last).contains(&c))
}

/// Every `Cc`, the line and paragraph separators (U+2028, U+2029) and every invisible
/// character.
fn forbidden(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') || is_invisible(c)
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
    if is_reserved(text) {
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
