//! Google and Discord (ADR-0008 D2, 008 RF-C1, plan-c decision 5): their URLs, their single
//! scope and how each one says who the user is.
//!
//! The URLs are constants of the code. Only `Settings` (test code) can replace them, never an
//! environment variable: a wrong value in production would send the `client_secret` and the
//! `code` to another host, which could also choose the identity.

use protocol::Provider;
use serde::Deserialize;

/// The three endpoints of a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUrls {
    pub authorize: String,
    pub token: String,
    pub userinfo: String,
}

/// Replacement URLs for the tests (the fake providers). `None`: the production constants.
#[derive(Debug, Clone, Default)]
pub struct OAuthSettings {
    pub google: Option<ProviderUrls>,
    pub discord: Option<ProviderUrls>,
}

pub const GOOGLE_AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_USERINFO: &str = "https://openidconnect.googleapis.com/v1/userinfo";
pub const DISCORD_AUTHORIZE: &str = "https://discord.com/oauth2/authorize";
pub const DISCORD_TOKEN: &str = "https://discord.com/api/oauth2/token";
pub const DISCORD_USERINFO: &str = "https://discord.com/api/v10/users/@me";

/// The longest identity the server accepts from a provider.
pub const MAX_SUBJECT_LEN: usize = 255;
/// A Discord id is a snowflake: a decimal `u64`, at most 20 digits.
pub const MAX_SNOWFLAKE_DIGITS: usize = 20;

/// The production URLs of a real provider.
pub fn production(provider: Provider) -> Option<ProviderUrls> {
    let (authorize, token, userinfo) = match provider {
        Provider::Google => (GOOGLE_AUTHORIZE, GOOGLE_TOKEN, GOOGLE_USERINFO),
        Provider::Discord => (DISCORD_AUTHORIZE, DISCORD_TOKEN, DISCORD_USERINFO),
        Provider::Dev => return None,
    };
    Some(ProviderUrls {
        authorize: authorize.to_owned(),
        token: token.to_owned(),
        userinfo: userinfo.to_owned(),
    })
}

/// The only scope each provider is asked for (RF-C1). `dev` asks for none.
pub const fn scope(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::Google => Some("openid"),
        Provider::Discord => Some("identify"),
        Provider::Dev => None,
    }
}

/// `userinfo` of Google and of the dev provider: only `sub` is declared, so `name`, `email` and
/// the rest never reach a type of the server.
#[derive(Deserialize)]
struct Subject {
    sub: String,
}

/// `users/@me` of Discord: only `id`.
#[derive(Deserialize)]
struct DiscordUser {
    id: String,
}

/// The identity in a `userinfo` body, or `None` if it is missing or malformed.
pub fn identity(provider: Provider, body: &[u8]) -> Option<String> {
    let subject = match provider {
        Provider::Google | Provider::Dev => serde_json::from_slice::<Subject>(body).ok()?.sub,
        Provider::Discord => {
            let id = serde_json::from_slice::<DiscordUser>(body).ok()?.id;
            let snowflake = (1..=MAX_SNOWFLAKE_DIGITS).contains(&id.len())
                && id.bytes().all(|b| b.is_ascii_digit());
            if !snowflake {
                return None;
            }
            id
        }
    };
    let valid = !subject.is_empty()
        && subject.len() <= MAX_SUBJECT_LEN
        && !subject.chars().any(char::is_control);
    valid.then_some(subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_urls_are_https_constants_of_the_spec() {
        for provider in [Provider::Google, Provider::Discord] {
            let urls = production(provider).unwrap();
            for url in [&urls.authorize, &urls.token, &urls.userinfo] {
                assert!(url.starts_with("https://"), "{url}");
            }
        }
        assert_eq!(production(Provider::Dev), None);
        assert_eq!(scope(Provider::Google), Some("openid"));
        assert_eq!(scope(Provider::Discord), Some("identify"));
        assert_eq!(scope(Provider::Dev), None);
    }

    #[test]
    fn each_provider_reads_only_its_identity_field() {
        let google = br#"{"sub":"1234","name":"N","email":"e@x","id":"99"}"#;
        assert_eq!(identity(Provider::Google, google).as_deref(), Some("1234"));
        let discord = br#"{"id":"80351110224678912","username":"u","global_name":"g","sub":"x"}"#;
        assert_eq!(
            identity(Provider::Discord, discord).as_deref(),
            Some("80351110224678912")
        );
        // Each one ignores the other's field.
        assert_eq!(identity(Provider::Google, br#"{"id":"1"}"#), None);
        assert_eq!(identity(Provider::Discord, br#"{"sub":"1"}"#), None);
    }

    #[test]
    fn malformed_identities_are_refused() {
        let long = format!(r#"{{"sub":"{}"}}"#, "x".repeat(MAX_SUBJECT_LEN + 1));
        let longest = format!(r#"{{"sub":"{}"}}"#, "x".repeat(MAX_SUBJECT_LEN));
        assert!(identity(Provider::Google, longest.as_bytes()).is_some());
        for body in [
            long.as_str(),
            r#"{"sub":""}"#,
            r#"{"sub":"a\u0007"}"#,
            r#"{"sub":5}"#,
            "not json",
            "",
        ] {
            assert_eq!(identity(Provider::Google, body.as_bytes()), None, "{body}");
        }
        for body in [
            r#"{"id":""}"#,
            r#"{"id":"12a"}"#,
            r#"{"id":"-1"}"#,
            r#"{"id":"123456789012345678901"}"#,
            r#"{"id":80351110224678912}"#,
        ] {
            assert_eq!(identity(Provider::Discord, body.as_bytes()), None, "{body}");
        }
        assert!(identity(Provider::Discord, br#"{"id":"12345678901234567890"}"#).is_some());
    }
}
