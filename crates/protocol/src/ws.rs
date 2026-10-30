use serde::{Deserialize, Serialize};

use crate::ErrorCode;

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

/// The channels a socket can subscribe to (ADR-0008 D6). The only private one is `self`, with
/// no id on the client side: the server resolves it from the session. The public channels
/// arrive with TA-13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Channel {
    #[serde(rename = "self")]
    SelfChannel,
}

impl Channel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelfChannel => "self",
        }
    }

    /// The channel with exactly this name, or `None` (unknown, malformed or with an id).
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "self" => Some(Self::SelfChannel),
            _ => None,
        }
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
        assert_eq!(Channel::parse("self"), Some(Channel::SelfChannel));
        assert_eq!(Channel::SelfChannel.as_str(), "self");
        for text in [
            "",
            "SELF",
            "self ",
            "user:1",
            "user:self",
            "fight:1",
            "schedule",
            "self\0",
        ] {
            assert_eq!(Channel::parse(text), None, "{text:?}");
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
