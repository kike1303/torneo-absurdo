use serde::{Deserialize, Serialize};

/// Largest integer a JavaScript number holds exactly (2^53 − 1). `Me.balance` is exported as
/// `number`, so the server refuses to send anything outside `±MAX_SAFE_INTEGER` (008 plan-b,
/// question 2).
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// `GET /api/me` (008 RF-B12). No `user_id`, no session id and nothing from the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Me {
    pub display_name: String,
    pub role: Role,
    /// Coins (`SUM` of the ledger). The only 64-bit field of the protocol: exported as `number`
    /// and bounded by [`MAX_SAFE_INTEGER`] on the server.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub balance: i64,
    pub providers: Vec<Provider>,
    pub locale: Option<Locale>,
    pub theme: Option<Theme>,
    pub display_name_is_generated: bool,
}

/// `PUT /api/me/display-name` (008 RF-C4). Nothing but the name (RF-C8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct DisplayNameRequest {
    pub display_name: String,
}

/// `PUT /api/me/preferences` (008 RF-C5). A missing field leaves the preference as it is
/// (`None`); `null` clears it (`Some(None)`). Nothing else is accepted (RF-C8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct PreferencesRequest {
    #[serde(default, with = "present", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Locale>", optional = nullable))]
    pub locale: Option<Option<Locale>>,
    #[serde(default, with = "present", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Theme>", optional = nullable))]
    pub theme: Option<Option<Theme>>,
}

/// A field that is present, `null` or not: serde alone reads both a missing field and `null`
/// as `None`. With `default`, a missing field never reaches `deserialize`, and with
/// `skip_serializing_if`, `serialize` never sees an absent one.
mod present {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(
        value: &Option<Option<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .and_then(Option::as_ref)
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Admin,
}

/// An OAuth provider (ADR-0008 D1). `dev` exists only in binaries built with `dev-auth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Google,
    Discord,
    Dev,
}

impl Provider {
    pub const ALL: &'static [Self] = &[Self::Google, Self::Discord, Self::Dev];

    /// The code in URLs and in the database (`oauth_accounts.provider`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Discord => "discord",
            Self::Dev => "dev",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|p| p.as_str() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    En,
    Es,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn me_has_the_exact_shape() {
        let me = Me {
            display_name: "TigreVeloz42".to_owned(),
            role: Role::User,
            balance: 100,
            providers: vec![Provider::Dev],
            locale: None,
            theme: Some(Theme::Light),
            display_name_is_generated: true,
        };
        let json = r#"{"display_name":"TigreVeloz42","role":"user","balance":100,"providers":["dev"],"locale":null,"theme":"light","display_name_is_generated":true}"#;
        assert_eq!(serde_json::to_string(&me).unwrap(), json);
        assert_eq!(serde_json::from_str::<Me>(json).unwrap(), me);
    }

    #[test]
    fn providers_round_trip_their_codes() {
        for provider in Provider::ALL {
            assert_eq!(Provider::parse(provider.as_str()), Some(*provider));
            assert_eq!(
                serde_json::to_value(provider).unwrap(),
                serde_json::Value::from(provider.as_str())
            );
        }
        assert_eq!(Provider::parse("Google"), None);
        assert_eq!(Provider::parse(""), None);
    }

    #[test]
    fn display_name_request_has_the_exact_shape() {
        let body = DisplayNameRequest {
            display_name: "Tigre".to_owned(),
        };
        let json = r#"{"display_name":"Tigre"}"#;
        assert_eq!(serde_json::to_string(&body).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<DisplayNameRequest>(json).unwrap(),
            body
        );
        // RF-C8: no field but the name, and the name is required.
        for bad in [
            r#"{"display_name":"Tigre","role":"admin"}"#,
            r#"{"display_name":"Tigre","banned_at":null}"#,
            r"{}",
            r#"{"display_name":5}"#,
        ] {
            assert!(
                serde_json::from_str::<DisplayNameRequest>(bad).is_err(),
                "{bad}"
            );
        }
    }

    /// Absent is "no change", `null` is "clear", a value is "set" (RF-C5).
    #[test]
    fn preferences_request_tells_absent_from_null() {
        let cases = [
            (r"{}", None, None),
            (r#"{"locale":null}"#, Some(None), None),
            (r#"{"locale":"es"}"#, Some(Some(Locale::Es)), None),
            (
                r#"{"locale":"en","theme":null}"#,
                Some(Some(Locale::En)),
                Some(None),
            ),
            (r#"{"theme":"system"}"#, None, Some(Some(Theme::System))),
        ];
        for (json, locale, theme) in cases {
            let body: PreferencesRequest = serde_json::from_str(json).unwrap();
            assert_eq!(body, PreferencesRequest { locale, theme }, "{json}");
            // Serialization keeps the difference too.
            assert_eq!(serde_json::to_string(&body).unwrap(), json, "{json}");
        }
        for bad in [
            r#"{"locale":"fr"}"#,
            r#"{"theme":"blue"}"#,
            r#"{"role":"admin"}"#,
            r#"{"banned_at":"2026-10-29T00:00:00Z"}"#,
            r#"{"locale":5}"#,
        ] {
            assert!(
                serde_json::from_str::<PreferencesRequest>(bad).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn max_safe_integer_is_two_to_the_53_minus_one() {
        assert_eq!(MAX_SAFE_INTEGER, (1_i64 << 53) - 1);
    }

    /// The single declared exception to "no 64-bit integers in exported types" (lib.rs).
    #[test]
    fn only_me_balance_overrides_its_ts_type() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let needle = ["ts(type", " = "].concat();
            hits += text.matches(&needle).count();
        }
        assert_eq!(hits, 1, "only Me.balance may override its TypeScript type");
    }
}
