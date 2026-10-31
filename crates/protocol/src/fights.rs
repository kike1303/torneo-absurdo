//! The public shape of a scheduled fight (010): its state, its final and the two read models of
//! the API. Contract: no human-readable text, no 64-bit integers (hours are [`TimestampMs`]), and
//! neither [`FightSummary`] nor [`FightsCurrent`] has a seed or a log. [`FightDetail`] has them
//! only for the server to fill on a settled fight (RF26, RF34).
use serde::{Deserialize, Serialize};

use crate::fight::{EndReason, Side};

/// Milliseconds since the Unix epoch, in decimal: JavaScript loses precision on a 64-bit number,
/// so it travels as a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TimestampMs(String);

impl TimestampMs {
    #[must_use]
    pub fn from_ms(ms: i64) -> Self {
        Self(ms.to_string())
    }

    /// The milliseconds, or `None` if the text is not a decimal number.
    #[must_use]
    pub fn as_ms(&self) -> Option<i64> {
        self.0.parse().ok()
    }
}

/// The public states of a fight (RF8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum FightState {
    Scheduled,
    BettingOpen,
    BettingClosed,
    Live,
    Settled,
}

impl FightState {
    /// The value of the `state` column, the same string serde writes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::BettingOpen => "betting_open",
            Self::BettingClosed => "betting_closed",
            Self::Live => "live",
            Self::Settled => "settled",
        }
    }

    /// The state with exactly this name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "scheduled" => Some(Self::Scheduled),
            "betting_open" => Some(Self::BettingOpen),
            "betting_closed" => Some(Self::BettingClosed),
            "live" => Some(Self::Live),
            "settled" => Some(Self::Settled),
            _ => None,
        }
    }
}

/// Why a fight was cancelled: only ever from `scheduled`, with no seed and no bets (RF16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    MissedWindow,
    FightUnrunnable,
}

impl CancelReason {
    /// The value of the `cancel_reason` column, the same string serde writes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissedWindow => "missed_window",
            Self::FightUnrunnable => "fight_unrunnable",
        }
    }

    /// The reason with exactly this name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "missed_window" => Some(Self::MissedWindow),
            "fight_unrunnable" => Some(Self::FightUnrunnable),
            _ => None,
        }
    }
}

/// Why a fight was voided (RF59). Only the CLI of TA-27 writes it; the server only reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum VoidReason {
    SnapshotInvalid,
    EngineError,
}

impl VoidReason {
    /// The value with exactly this name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "snapshot_invalid" => Some(Self::SnapshotInvalid),
            "engine_error" => Some(Self::EngineError),
            _ => None,
        }
    }
}

/// How a settled fight ended: exactly one of these (RF9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FightFinal {
    Decided {
        winner: Option<Side>,
        reason: EndReason,
    },
    Cancelled {
        reason: CancelReason,
    },
    Voided {
        reason: VoidReason,
    },
}

/// A fighter as a fight announces it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FighterCard {
    pub slug: String,
    pub max_hp: u16,
}

/// What everyone may know of a fight at any moment. It has no duration: only the constant bounds
/// of its ruleset (RF41).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FightSummary {
    pub id: u32,
    pub state: FightState,
    pub a: FighterCard,
    pub b: FighterCard,
    pub ruleset_version: u16,
    /// Lowercase hexadecimal.
    pub snapshot_sha256: String,
    /// Lowercase hexadecimal, from `betting_open` on (never on a cancelled fight).
    pub seed_commit: Option<String>,
    pub opens_at: TimestampMs,
    pub closes_at: TimestampMs,
    pub live_at: TimestampMs,
    pub live_started_at: Option<TimestampMs>,
    pub time_limit_ms: u32,
    pub max_duration_ms: u32,
    pub playback_delay_ms: u32,
    #[serde(rename = "final")]
    pub final_result: Option<FightFinal>,
}

/// `GET /api/fights/{id}`: everything needed to verify a fight (RF7, RF34). `spec_a`, `spec_b` and
/// `log` carry the exact stored bytes, and are `null` only if a corrupt row has bytes that are not
/// UTF-8. `seed` (lowercase hexadecimal) and `log` are only ever filled on a settled fight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FightDetail {
    pub fight: FightSummary,
    pub spec_a: Option<String>,
    pub spec_b: Option<String>,
    pub seed: Option<String>,
    pub log: Option<String>,
}

/// `GET /api/fights/current` (RF35). `halted` is `null` while the server does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FightsCurrent {
    pub server_time: TimestampMs,
    pub halted: Option<bool>,
    pub current: Option<FightSummary>,
    pub next: Option<FightSummary>,
    pub last: Option<FightSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> FightSummary {
        FightSummary {
            id: 7,
            state: FightState::BettingOpen,
            a: FighterCard {
                slug: "x-a".to_owned(),
                max_hp: 504,
            },
            b: FighterCard {
                slug: "x-b".to_owned(),
                max_hp: 900,
            },
            ruleset_version: 1,
            snapshot_sha256: "ab".to_owned(),
            seed_commit: Some("cd".to_owned()),
            opens_at: TimestampMs::from_ms(1),
            closes_at: TimestampMs::from_ms(2),
            live_at: TimestampMs::from_ms(3),
            live_started_at: None,
            time_limit_ms: 86_000,
            max_duration_ms: 90_000,
            playback_delay_ms: 3500,
            final_result: None,
        }
    }

    const SUMMARY: &str = concat!(
        r#"{"id":7,"state":"betting_open","a":{"slug":"x-a","max_hp":504},"#,
        r#""b":{"slug":"x-b","max_hp":900},"ruleset_version":1,"snapshot_sha256":"ab","#,
        r#""seed_commit":"cd","opens_at":"1","closes_at":"2","live_at":"3","live_started_at":null,"#,
        r#""time_limit_ms":86000,"max_duration_ms":90000,"playback_delay_ms":3500,"final":null}"#,
    );

    #[test]
    fn a_summary_has_the_exact_shape_and_neither_seed_nor_log() {
        let value = summary();
        assert_eq!(serde_json::to_string(&value).unwrap(), SUMMARY);
        assert_eq!(
            serde_json::from_str::<FightSummary>(SUMMARY).unwrap(),
            value
        );
        let json: serde_json::Value = serde_json::to_value(&value).unwrap();
        for field in ["seed", "log", "duration_ms", "end", "events"] {
            assert!(json.get(field).is_none(), "{field}");
        }
    }

    #[test]
    fn the_board_has_no_seed_nor_log() {
        let value = FightsCurrent {
            server_time: TimestampMs::from_ms(9),
            halted: None,
            current: Some(summary()),
            next: None,
            last: None,
        };
        let json = serde_json::to_value(&value).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 5);
        for key in ["server_time", "halted", "current", "next", "last"] {
            assert!(keys.contains(&key), "{key}");
        }
    }

    #[test]
    fn the_detail_has_the_exact_shape() {
        let value = FightDetail {
            fight: summary(),
            spec_a: Some("{}".to_owned()),
            spec_b: None,
            seed: None,
            log: None,
        };
        let json = format!(
            r#"{{"fight":{SUMMARY},"spec_a":"{{}}","spec_b":null,"seed":null,"log":null}}"#
        );
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        assert_eq!(serde_json::from_str::<FightDetail>(&json).unwrap(), value);
    }

    #[test]
    fn finals_have_the_exact_shape() {
        let cases = [
            (
                FightFinal::Decided {
                    winner: Some(Side::A),
                    reason: EndReason::Ko,
                },
                r#"{"kind":"decided","winner":"a","reason":"ko"}"#,
            ),
            (
                FightFinal::Decided {
                    winner: None,
                    reason: EndReason::Draw,
                },
                r#"{"kind":"decided","winner":null,"reason":"draw"}"#,
            ),
            (
                FightFinal::Cancelled {
                    reason: CancelReason::MissedWindow,
                },
                r#"{"kind":"cancelled","reason":"missed_window"}"#,
            ),
            (
                FightFinal::Cancelled {
                    reason: CancelReason::FightUnrunnable,
                },
                r#"{"kind":"cancelled","reason":"fight_unrunnable"}"#,
            ),
            (
                FightFinal::Voided {
                    reason: VoidReason::SnapshotInvalid,
                },
                r#"{"kind":"voided","reason":"snapshot_invalid"}"#,
            ),
            (
                FightFinal::Voided {
                    reason: VoidReason::EngineError,
                },
                r#"{"kind":"voided","reason":"engine_error"}"#,
            ),
        ];
        for (value, json) in cases {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<FightFinal>(json).unwrap(), value);
        }
    }

    #[test]
    fn the_state_names_are_those_of_serde_and_of_the_column() {
        for state in [
            FightState::Scheduled,
            FightState::BettingOpen,
            FightState::BettingClosed,
            FightState::Live,
            FightState::Settled,
        ] {
            assert_eq!(
                serde_json::to_string(&state).unwrap(),
                format!("\"{}\"", state.as_str())
            );
            assert_eq!(FightState::parse(state.as_str()), Some(state));
        }
        assert_eq!(FightState::parse("SETTLED"), None);
    }

    #[test]
    fn the_reason_names_parse_only_from_the_closed_set() {
        for reason in [CancelReason::MissedWindow, CancelReason::FightUnrunnable] {
            assert_eq!(CancelReason::parse(reason.as_str()), Some(reason));
        }
        assert_eq!(CancelReason::parse("other"), None);
        assert_eq!(
            VoidReason::parse("snapshot_invalid"),
            Some(VoidReason::SnapshotInvalid)
        );
        assert_eq!(
            VoidReason::parse("engine_error"),
            Some(VoidReason::EngineError)
        );
        assert_eq!(VoidReason::parse("other"), None);
    }

    #[test]
    fn a_timestamp_is_a_decimal_string() {
        let value = TimestampMs::from_ms(1_700_000_000_123);
        assert_eq!(serde_json::to_string(&value).unwrap(), r#""1700000000123""#);
        assert_eq!(value.as_ms(), Some(1_700_000_000_123));
    }
}
