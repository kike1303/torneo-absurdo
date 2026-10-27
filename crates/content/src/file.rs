//! File shape of `content/characters/<slug>/character.json` (003 "Esquema de personaje").
//!
//! It is the engine shape plus `archetype`, `ruleset_version` and `rivals`, with the fields in
//! the order of the spec. Every level rejects unknown fields (the nested types of the engine
//! already do).
use engine::{Ability, CharacterSpec, Element, Personality, Stats};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterFile {
    pub(crate) slug: String,
    pub(crate) archetype: String,
    pub(crate) ruleset_version: u16,
    pub(crate) stats: Stats,
    pub(crate) personality: Personality,
    pub(crate) abilities: Vec<Ability>,
    pub(crate) weaknesses: Vec<Element>,
    pub(crate) rivals: Vec<String>,
}

impl CharacterFile {
    /// Splits the file into the engine shape and the fields the engine ignores. Not validated:
    /// `load_roster` validates before building a `Character`.
    pub(crate) fn into_parts(self) -> (CharacterSpec, String, Vec<String>) {
        let spec = CharacterSpec {
            slug: self.slug,
            stats: self.stats,
            personality: self.personality,
            abilities: self.abilities,
            weaknesses: self.weaknesses,
        };
        (spec, self.archetype, self.rivals)
    }
}
