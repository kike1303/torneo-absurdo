//! Canonical serialization (003 RF11): compact `serde_json::to_vec`, fields in declaration
//! order, no maps and no floats. "Byte-identical" means these functions return the same bytes.
use std::fmt;

use protocol::FightLog;

use crate::CharacterSpec;

/// The value could not be serialized. A code without text; never silently replaced by an
/// empty output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeError;

impl EncodeError {
    /// Stable code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        "encode_failed"
    }
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for EncodeError {}

/// Canonical bytes of a fight log (what the database, the goldens and the verifier compare).
///
/// # Errors
/// [`EncodeError`] if serialization fails.
pub fn canonical_json(log: &FightLog) -> Result<Vec<u8>, EncodeError> {
    serde_json::to_vec(log).map_err(|_| EncodeError)
}

/// Canonical bytes of a character in the engine shape (RF22: snapshot or extended commit).
///
/// # Errors
/// [`EncodeError`] if serialization fails.
pub fn canonical_spec_json(spec: &CharacterSpec) -> Result<Vec<u8>, EncodeError> {
    serde_json::to_vec(spec).map_err(|_| EncodeError)
}

#[cfg(test)]
mod tests {
    use protocol::{EndReason, FightEvent, FighterInfo};

    use super::*;

    #[test]
    fn log_is_compact_and_in_declaration_order() {
        let log = FightLog {
            ruleset_version: 1,
            a: FighterInfo {
                slug: "x-a".to_owned(),
                max_hp: 504,
            },
            b: FighterInfo {
                slug: "x-b".to_owned(),
                max_hp: 900,
            },
            events: vec![
                FightEvent::Intro { t: 0 },
                FightEvent::TimeUp { t: 86_000 },
                FightEvent::End {
                    t: 90_000,
                    winner: None,
                    reason: EndReason::Draw,
                },
            ],
        };
        let bytes = canonical_json(&log).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            concat!(
                r#"{"ruleset_version":1,"a":{"slug":"x-a","max_hp":504},"b":{"slug":"x-b","max_hp":900},"#,
                r#""events":[{"type":"intro","t":0},{"type":"time_up","t":86000},"#,
                r#"{"type":"end","t":90000,"winner":null,"reason":"draw"}]}"#
            )
        );
        assert_eq!(EncodeError.code(), "encode_failed");
    }
}
