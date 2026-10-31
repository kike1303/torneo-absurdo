use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ErrorCode, FightEvent, FightSummary, FightsCurrent, TimestampMs};

/// Message from the client over `/ws` (RF-B5).
///
/// The development echo, which the server rejects unless `ENABLE_WS_ECHO=1` on a loopback
/// origin (RF-B13), and subscriptions (008 RF-B15). `channel` is free text on the wire: the
/// server converts it with [`Channel::parse`] and refuses anything else with
/// `ws.forbidden_channel`. No message changes server state (ADR-0008 D6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Echo { payload: String },
    Subscribe { channel: String },
    Unsubscribe { channel: String },
}

/// The id of a fight in a channel name: a canonical decimal from 1 to `i32::MAX`, the range of the
/// `fights.id` column (010 RF37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FightId(u32);

impl FightId {
    /// The id, or `None` out of range.
    #[must_use]
    pub const fn new(id: u32) -> Option<Self> {
        if id >= 1 && id <= i32::MAX.unsigned_abs() {
            Some(Self(id))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Only ASCII digits, no sign, no leading zero, in range.
    fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse().ok().and_then(Self::new)
    }
}

/// The channels a socket can subscribe to (ADR-0008 D6, 010 RF37). The only private one is `self`,
/// with no id on the client side: the server resolves it from the session. On the wire it is a
/// string (`self`, `schedule`, `fight:12`), and only [`Channel::parse`] reads it, so there are never
/// two forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, as = "String"))]
pub enum Channel {
    SelfChannel,
    Schedule,
    Fight(FightId),
}

impl Channel {
    /// The channel with exactly this name, or `None` (unknown, malformed or with an invalid id).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "self" => Some(Self::SelfChannel),
            "schedule" => Some(Self::Schedule),
            _ => text
                .strip_prefix("fight:")
                .and_then(FightId::parse)
                .map(Self::Fight),
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfChannel => f.write_str("self"),
            Self::Schedule => f.write_str("schedule"),
            Self::Fight(id) => write!(f, "fight:{}", id.get()),
        }
    }
}

impl Serialize for Channel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Channel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("invalid channel"))
    }
}

/// Events of the `self` channel. The first balance event arrives with TA-27.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelfEvent {
    /// The user renamed themselves (in another tab, phase C).
    ProfileChanged { display_name: String },
}

/// Message from the server over `/ws` (RF-B5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Echo {
        payload: String,
    },
    Subscribed {
        channel: Channel,
    },
    Unsubscribed {
        channel: Channel,
    },
    /// A refused message that does not close the socket (`ws.forbidden_channel`,
    /// `ws.too_many_subscriptions`).
    Error {
        code: ErrorCode,
    },
    SelfEvent {
        event: SelfEvent,
    },
    /// The whole board: on subscribing to `schedule`, on each transition and each time `halted`
    /// changes (010 RF39). The client replaces what it had. Its `server_time` is in `board`.
    ScheduleUpdate {
        board: Box<FightsCurrent>,
    },
    /// All the events already released of a fight: on subscribing to `fight:{id}`, on each
    /// transition and to a socket that fell behind. Never an event that is not due.
    FightSync {
        fight: Box<FightSummary>,
        server_time: TimestampMs,
        events: Vec<FightEvent>,
    },
    /// A batch of released events. `from_index` is the index of the first one: the client discards
    /// what it already has.
    FightEvents {
        fight_id: u32,
        from_index: u32,
        events: Vec<FightEvent>,
        server_time: TimestampMs,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ErrorCode, WsErrorCode};

    const ECHO: &str = r#"{"type":"echo","payload":"hola"}"#;

    #[test]
    fn client_echo_has_the_exact_shape() {
        let value = ClientMessage::Echo {
            payload: "hola".to_owned(),
        };
        assert_eq!(serde_json::to_string(&value).unwrap(), ECHO);
        assert_eq!(serde_json::from_str::<ClientMessage>(ECHO).unwrap(), value);
    }

    #[test]
    fn server_echo_has_the_exact_shape() {
        let value = ServerMessage::Echo {
            payload: "hola".to_owned(),
        };
        assert_eq!(serde_json::to_string(&value).unwrap(), ECHO);
        assert_eq!(serde_json::from_str::<ServerMessage>(ECHO).unwrap(), value);
    }

    #[test]
    fn subscription_messages_have_the_exact_shape() {
        let cases = [
            (
                ClientMessage::Subscribe {
                    channel: "self".to_owned(),
                },
                r#"{"type":"subscribe","channel":"self"}"#,
            ),
            (
                ClientMessage::Unsubscribe {
                    channel: "user:1".to_owned(),
                },
                r#"{"type":"unsubscribe","channel":"user:1"}"#,
            ),
        ];
        for (value, json) in cases {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<ClientMessage>(json).unwrap(), value);
        }
        let cases = [
            (
                ServerMessage::Subscribed {
                    channel: Channel::SelfChannel,
                },
                r#"{"type":"subscribed","channel":"self"}"#,
            ),
            (
                ServerMessage::Unsubscribed {
                    channel: Channel::SelfChannel,
                },
                r#"{"type":"unsubscribed","channel":"self"}"#,
            ),
            (
                ServerMessage::Error {
                    code: ErrorCode::Ws(WsErrorCode::ForbiddenChannel),
                },
                r#"{"type":"error","code":"ws.forbidden_channel"}"#,
            ),
            (
                ServerMessage::SelfEvent {
                    event: SelfEvent::ProfileChanged {
                        display_name: "TigreVeloz42".to_owned(),
                    },
                },
                r#"{"type":"self_event","event":{"kind":"profile_changed","display_name":"TigreVeloz42"}}"#,
            ),
        ];
        for (value, json) in cases {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<ServerMessage>(json).unwrap(), value);
        }
    }

    #[test]
    fn channels_parse_only_from_the_closed_set() {
        let fight = |id| Channel::Fight(FightId::new(id).unwrap());
        for (text, channel) in [
            ("self", Channel::SelfChannel),
            ("schedule", Channel::Schedule),
            ("fight:1", fight(1)),
            ("fight:12", fight(12)),
            ("fight:2147483647", fight(2_147_483_647)),
        ] {
            assert_eq!(Channel::parse(text), Some(channel), "{text:?}");
            assert_eq!(channel.to_string(), text);
            let json = format!("\"{text}\"");
            assert_eq!(serde_json::to_string(&channel).unwrap(), json);
            assert_eq!(serde_json::from_str::<Channel>(&json).unwrap(), channel);
        }
        for text in [
            "",
            "SELF",
            "self ",
            "user:1",
            "user:self",
            "self\0",
            "fight:",
            "fight:0",
            "fight:01",
            "fight:+1",
            "fight:-1",
            "fight: 1",
            "fight:1 ",
            "FIGHT:1",
            "fight:2147483648",
            "fight:99999999999999999999",
            "fight:٣",
            "fight:1.0",
            "fight",
            "schedule:1",
            "Schedule",
        ] {
            assert_eq!(Channel::parse(text), None, "{text:?}");
            let json = serde_json::to_string(text).unwrap();
            assert!(serde_json::from_str::<Channel>(&json).is_err(), "{text:?}");
        }
    }

    #[test]
    fn a_fight_id_is_in_the_range_of_the_column() {
        assert_eq!(FightId::new(0), None);
        assert_eq!(FightId::new(1).map(FightId::get), Some(1));
        assert_eq!(
            FightId::new(2_147_483_647).map(FightId::get),
            Some(2_147_483_647)
        );
        assert_eq!(FightId::new(2_147_483_648), None);
    }

    fn summary() -> FightSummary {
        let card = |slug: &str| crate::FighterCard {
            slug: slug.to_owned(),
            max_hp: 100,
        };
        FightSummary {
            id: 7,
            state: crate::FightState::Live,
            a: card("a"),
            b: card("b"),
            ruleset_version: 1,
            snapshot_sha256: "aa".to_owned(),
            seed_commit: Some("bb".to_owned()),
            opens_at: TimestampMs::from_ms(1),
            closes_at: TimestampMs::from_ms(2),
            live_at: TimestampMs::from_ms(3),
            live_started_at: Some(TimestampMs::from_ms(3)),
            time_limit_ms: 86_000,
            max_duration_ms: 90_000,
            playback_delay_ms: 3500,
            final_result: None,
        }
    }

    /// RF26 and RF39: the exact shape of the new messages, with neither a seed nor a log.
    #[test]
    fn the_messages_of_the_fights_have_the_exact_shape() {
        let intro = || FightEvent::Intro { t: 0 };
        let board = FightsCurrent {
            server_time: TimestampMs::from_ms(9),
            halted: None,
            current: None,
            next: None,
            last: None,
        };
        let cases = [
            (
                ServerMessage::ScheduleUpdate {
                    board: Box::new(board),
                },
                r#"{"type":"schedule_update","board":{"server_time":"9","halted":null,"current":null,"next":null,"last":null}}"#.to_owned(),
            ),
            (
                ServerMessage::FightEvents {
                    fight_id: 7,
                    from_index: 3,
                    events: vec![intro()],
                    server_time: TimestampMs::from_ms(9),
                },
                r#"{"type":"fight_events","fight_id":7,"from_index":3,"events":[{"type":"intro","t":0}],"server_time":"9"}"#.to_owned(),
            ),
            (
                ServerMessage::FightSync {
                    fight: Box::new(summary()),
                    server_time: TimestampMs::from_ms(9),
                    events: vec![intro()],
                },
                format!(
                    r#"{{"type":"fight_sync","fight":{},"server_time":"9","events":[{{"type":"intro","t":0}}]}}"#,
                    serde_json::to_string(&summary()).unwrap()
                ),
            ),
            (
                ServerMessage::Subscribed {
                    channel: Channel::Fight(FightId::new(12).unwrap()),
                },
                r#"{"type":"subscribed","channel":"fight:12"}"#.to_owned(),
            ),
        ];
        for (value, json) in cases {
            let text = serde_json::to_string(&value).unwrap();
            assert_eq!(text, json);
            assert_eq!(serde_json::from_str::<ServerMessage>(&json).unwrap(), value);
            for word in ["seed\"", "\"log\"", "revealed"] {
                assert!(!text.contains(word), "{text}");
            }
        }
    }

    #[test]
    fn invalid_client_messages_do_not_deserialize() {
        for json in [
            r#"{"type":"echo"}"#,
            r#"{"type":"subscribe"}"#,
            r#"{"type":"subscribe","channel":1}"#,
            r#"{"payload":"hola"}"#,
            r#"{"type":"echo","payload":1}"#,
            "hola",
            "",
        ] {
            assert!(
                serde_json::from_str::<ClientMessage>(json).is_err(),
                "{json} should not deserialize"
            );
        }
    }
}
