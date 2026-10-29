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
