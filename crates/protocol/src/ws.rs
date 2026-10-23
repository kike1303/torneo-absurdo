use serde::{Deserialize, Serialize};

/// Message from the client over `/ws` (RF-B5).
///
/// In 001 the only variant is the development echo, which the server rejects unless
/// `ENABLE_WS_ECHO=1` on a loopback origin (RF-B13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Echo { payload: String },
}

/// Message from the server over `/ws` (RF-B5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Echo { payload: String },
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn invalid_client_messages_do_not_deserialize() {
        for json in [
            r#"{"type":"echo"}"#,
            r#"{"type":"subscribe"}"#,
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
