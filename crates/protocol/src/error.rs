use serde::{Deserialize, Serialize};

/// HTTP error codes (`http.*`) of the API (RF-B5, RF-B12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ApiErrorCode {
    #[serde(rename = "http.not_found")]
    NotFound,
    #[serde(rename = "http.method_not_allowed")]
    MethodNotAllowed,
    #[serde(rename = "http.rate_limited")]
    RateLimited,
}

impl ApiErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[Self::NotFound, Self::MethodNotAllowed, Self::RateLimited];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "http.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "http.not_found",
            Self::MethodNotAllowed => "http.method_not_allowed",
            Self::RateLimited => "http.rate_limited",
        }
    }
}

/// WebSocket codes (`ws.*`): HTTP bodies of the handshake (403, 503) and close reasons
/// (RF-B5, RF-B13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum WsErrorCode {
    #[serde(rename = "ws.origin_forbidden")]
    OriginForbidden,
    #[serde(rename = "ws.capacity")]
    Capacity,
    #[serde(rename = "ws.invalid_message")]
    InvalidMessage,
    #[serde(rename = "ws.echo_disabled")]
    EchoDisabled,
    #[serde(rename = "ws.rate_limited")]
    RateLimited,
    #[serde(rename = "ws.pong_timeout")]
    PongTimeout,
    #[serde(rename = "ws.idle_timeout")]
    IdleTimeout,
    #[serde(rename = "ws.ip_capacity")]
    IpCapacity,
}

impl WsErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::OriginForbidden,
        Self::Capacity,
        Self::InvalidMessage,
        Self::EchoDisabled,
        Self::RateLimited,
        Self::PongTimeout,
        Self::IdleTimeout,
        Self::IpCapacity,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "ws.";

    /// The serialized code (the same string serde writes); also the WebSocket close reason.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OriginForbidden => "ws.origin_forbidden",
            Self::Capacity => "ws.capacity",
            Self::InvalidMessage => "ws.invalid_message",
            Self::EchoDisabled => "ws.echo_disabled",
            Self::RateLimited => "ws.rate_limited",
            Self::PongTimeout => "ws.pong_timeout",
            Self::IdleTimeout => "ws.idle_timeout",
            Self::IpCapacity => "ws.ip_capacity",
        }
    }
}

/// Authentication codes (`auth.*`, feature 008). Phase A only has the `Origin` check (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum AuthErrorCode {
    #[serde(rename = "auth.csrf_origin")]
    CsrfOrigin,
}

impl AuthErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[Self::CsrfOrigin];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "auth.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CsrfOrigin => "auth.csrf_origin",
        }
    }
}

/// Any error code the server can put in an HTTP body. Untagged, so in TypeScript it is
/// `ApiErrorCode | WsErrorCode | AuthErrorCode`. The string sets of the variants never overlap (each enum
/// owns one prefix), which keeps deserialization unambiguous. A new domain (for example
/// `auth.*`) gets a new enum and a new variant here, never codes inside the existing ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(untagged)]
pub enum ErrorCode {
    Http(ApiErrorCode),
    Ws(WsErrorCode),
    Auth(AuthErrorCode),
}

impl ErrorCode {
    /// The serialized code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http(code) => code.as_str(),
            Self::Ws(code) => code.as_str(),
            Self::Auth(code) => code.as_str(),
        }
    }
}

impl From<ApiErrorCode> for ErrorCode {
    fn from(code: ApiErrorCode) -> Self {
        Self::Http(code)
    }
}

impl From<WsErrorCode> for ErrorCode {
    fn from(code: WsErrorCode) -> Self {
        Self::Ws(code)
    }
}

impl From<AuthErrorCode> for ErrorCode {
    fn from(code: AuthErrorCode) -> Self {
        Self::Auth(code)
    }
}

/// Body of every HTTP error response of the server (RF-B5): `{"error":"<code>"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ApiError {
    pub error: ErrorCode,
}

impl ApiError {
    pub fn new(code: impl Into<ErrorCode>) -> Self {
        Self { error: code.into() }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Fails to compile when a variant is added without listing it in `ALL`'s test below.
    fn api_index(code: ApiErrorCode) -> usize {
        match code {
            ApiErrorCode::NotFound => 0,
            ApiErrorCode::MethodNotAllowed => 1,
            ApiErrorCode::RateLimited => 2,
        }
    }

    fn ws_index(code: WsErrorCode) -> usize {
        match code {
            WsErrorCode::OriginForbidden => 0,
            WsErrorCode::Capacity => 1,
            WsErrorCode::InvalidMessage => 2,
            WsErrorCode::EchoDisabled => 3,
            WsErrorCode::RateLimited => 4,
            WsErrorCode::PongTimeout => 5,
            WsErrorCode::IdleTimeout => 6,
            WsErrorCode::IpCapacity => 7,
        }
    }

    fn auth_index(code: AuthErrorCode) -> usize {
        match code {
            AuthErrorCode::CsrfOrigin => 0,
        }
    }

    const API_COUNT: usize = 3;
    const WS_COUNT: usize = 8;
    const AUTH_COUNT: usize = 1;

    #[test]
    fn all_lists_every_variant_once() {
        let api: HashSet<usize> = ApiErrorCode::ALL.iter().map(|c| api_index(*c)).collect();
        assert_eq!(api.len(), API_COUNT);
        assert_eq!(ApiErrorCode::ALL.len(), API_COUNT);
        let ws: HashSet<usize> = WsErrorCode::ALL.iter().map(|c| ws_index(*c)).collect();
        assert_eq!(ws.len(), WS_COUNT);
        assert_eq!(WsErrorCode::ALL.len(), WS_COUNT);
        let auth: HashSet<usize> = AuthErrorCode::ALL.iter().map(|c| auth_index(*c)).collect();
        assert_eq!(auth.len(), AUTH_COUNT);
        assert_eq!(AuthErrorCode::ALL.len(), AUTH_COUNT);
    }

    #[test]
    fn api_codes_have_the_exact_strings() {
        let expected = [
            (ApiErrorCode::NotFound, "http.not_found"),
            (ApiErrorCode::MethodNotAllowed, "http.method_not_allowed"),
            (ApiErrorCode::RateLimited, "http.rate_limited"),
        ];
        assert_eq!(expected.len(), API_COUNT);
        for (code, text) in expected {
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<ApiErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn ws_codes_have_the_exact_strings() {
        let expected = [
            (WsErrorCode::OriginForbidden, "ws.origin_forbidden"),
            (WsErrorCode::Capacity, "ws.capacity"),
            (WsErrorCode::InvalidMessage, "ws.invalid_message"),
            (WsErrorCode::EchoDisabled, "ws.echo_disabled"),
            (WsErrorCode::RateLimited, "ws.rate_limited"),
            (WsErrorCode::PongTimeout, "ws.pong_timeout"),
            (WsErrorCode::IdleTimeout, "ws.idle_timeout"),
            (WsErrorCode::IpCapacity, "ws.ip_capacity"),
        ];
        assert_eq!(expected.len(), WS_COUNT);
        for (code, text) in expected {
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<WsErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn auth_codes_have_the_exact_strings() {
        let expected = [(AuthErrorCode::CsrfOrigin, "auth.csrf_origin")];
        assert_eq!(expected.len(), AUTH_COUNT);
        for (code, text) in expected {
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<AuthErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn as_str_matches_serde_for_every_code() {
        for code in ApiErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in WsErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in AuthErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
    }

    #[test]
    fn code_sets_use_their_prefix_and_do_not_overlap() {
        let mut seen = HashSet::new();
        for code in ApiErrorCode::ALL {
            assert!(code.as_str().starts_with(ApiErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in WsErrorCode::ALL {
            assert!(code.as_str().starts_with(WsErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in AuthErrorCode::ALL {
            assert!(code.as_str().starts_with(AuthErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        assert_eq!(seen.len(), API_COUNT + WS_COUNT + AUTH_COUNT);
    }

    #[test]
    fn every_code_round_trips_to_the_right_error_code_variant() {
        for code in ApiErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Http(*code)
            );
        }
        for code in WsErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Ws(*code)
            );
        }
        for code in AuthErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Auth(*code)
            );
        }
        assert_eq!(
            serde_json::from_str::<ErrorCode>("\"auth.csrf_origin\"").unwrap(),
            ErrorCode::Auth(AuthErrorCode::CsrfOrigin)
        );
        assert!(serde_json::from_str::<ErrorCode>("\"auth.unknown\"").is_err());
    }

    #[test]
    fn api_error_has_the_exact_shape() {
        let cases = [
            (
                ApiError::new(ApiErrorCode::NotFound),
                r#"{"error":"http.not_found"}"#,
            ),
            (
                ApiError::new(ApiErrorCode::MethodNotAllowed),
                r#"{"error":"http.method_not_allowed"}"#,
            ),
            (
                ApiError::new(WsErrorCode::OriginForbidden),
                r#"{"error":"ws.origin_forbidden"}"#,
            ),
            (
                ApiError::new(WsErrorCode::Capacity),
                r#"{"error":"ws.capacity"}"#,
            ),
            (
                ApiError::new(AuthErrorCode::CsrfOrigin),
                r#"{"error":"auth.csrf_origin"}"#,
            ),
            (
                ApiError::new(ApiErrorCode::RateLimited),
                r#"{"error":"http.rate_limited"}"#,
            ),
        ];
        for (value, json) in cases {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<ApiError>(json).unwrap(), value);
        }
    }
}
