//! File shape of `content/characters/<slug>/character.json` (003 "Esquema de personaje").
//!
//! It is the engine shape plus `archetype`, `ruleset_version`, `rivals` and the optional
//! `sprites` link (007 RF-B5), with the fields in the order of the spec. Every level rejects unknown fields (the nested types of the engine
//! already do).
use engine::{Ability, CharacterSpec, Element, Personality, Stats};
use serde::{Deserialize, Deserializer};

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
    #[serde(default, deserialize_with = "present_string")]
    pub(crate) sprites: Option<String>,
}

/// With the key present the value must be a string (`null` is not one, 007 decision 17); an
/// absent key is `None` through `default`. The pattern is not checked here.
fn present_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

/// The file split into the engine shape and the fields the engine ignores.
pub(crate) struct FileParts {
    pub(crate) spec: CharacterSpec,
    pub(crate) archetype: String,
    pub(crate) rivals: Vec<String>,
    pub(crate) sprites: Option<String>,
}

impl CharacterFile {
    /// Splits the file into the engine shape and the fields the engine ignores. Not validated:
    /// `load_roster` validates before building a `Character`.
    pub(crate) fn into_parts(self) -> FileParts {
        let spec = CharacterSpec {
            slug: self.slug,
            stats: self.stats,
            personality: self.personality,
            abilities: self.abilities,
            weaknesses: self.weaknesses,
        };
        FileParts {
            spec,
            archetype: self.archetype,
            rivals: self.rivals,
            sprites: self.sprites,
        }
    }
}
