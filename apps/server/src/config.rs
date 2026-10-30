//! Configuration from environment variables (RF-B8, plan-b decision 3).
//!
//! Read once at startup through [`Config::from_lookup`], a pure function over a lookup, so the
//! tests use a table instead of the process environment. Errors carry only the variable
//! name: the value is never stored in the error, so it can never be logged or printed.

use std::ffi::OsString;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use sqlx::postgres::PgConnectOptions;
use tracing_subscriber::EnvFilter;
use url::{Host, Url};

use crate::http::cookies::CookiePolicy;
use crate::ledger::Coins;
use crate::net::{InternalSecret, IpHashKey, TrustedProxies};

pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_WS_MAX_CONNECTIONS: u32 = 2000;
pub const DEFAULT_RUST_LOG: &str = "info";
/// Default bind address, what the container needs. `server healthcheck` always calls
/// `127.0.0.1`, so it assumes `BIND_ADDR` is `0.0.0.0` or `127.0.0.1`: with any other address
/// (or `::1`) the healthcheck fails. The production compose never sets `BIND_ADDR` (phase C).
pub const DEFAULT_BIND_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
/// Smallest decoded length of `INTERNAL_API_SECRET` and `IP_HASH_KEY` (008 RF-A6, RF-B18).
pub const INTERNAL_SECRET_MIN_BYTES: usize = 32;
/// Range of `DAILY_GRANT` (008 RF-B18): a typo would inflate the economy from the first login,
/// and entries cannot be undone.
pub const DAILY_GRANT_RANGE: std::ops::RangeInclusive<i64> = 1..=10_000;
/// Public development values of `apps/server/.env.example` (and of `apps/web/.env.example` for
/// the first). Outside a loopback `http` origin the server refuses them (RF-B18).
pub const DEV_INTERNAL_API_SECRET: &str = "Y3NJpi4_9UAo8Kx01a81pv33Dleek_Ax2e1xq4kLJpk";
pub const DEV_IP_HASH_KEY: &str = "mtPdFLwnW5YGomZ3cu58sF-v6WhmO0aQRTksx_Ypr90";

/// base64url, with or without padding.
const BASE64URL: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// Missing or invalid variable. Holds the name only, never the value.
    #[error("invalid or missing environment variable {var}")]
    Invalid { var: &'static str },
}

impl ConfigError {
    pub fn var(&self) -> &'static str {
        match self {
            Self::Invalid { var } => var,
        }
    }
}

/// Whether the development echo is on (plan-b decision 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoDecision {
    /// `ENABLE_WS_ECHO` is not `1`.
    Off,
    /// `ENABLE_WS_ECHO=1` and `PUBLIC_ORIGIN` is `http` on a loopback host.
    Enabled,
    /// `ENABLE_WS_ECHO=1` with any other origin: the echo stays off.
    Refused,
}

impl EchoDecision {
    pub fn is_enabled(self) -> bool {
        self == Self::Enabled
    }

    /// The `warn` event to log at startup, if any.
    pub fn startup_event(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Enabled => Some("ws.echo_enabled"),
            Self::Refused => Some("ws.echo_refused"),
        }
    }
}

/// Whether the origin is `http` on a loopback host (`localhost`, `127.0.0.1` or `[::1]`):
/// the development and e2e origins. The echo, the loopback in `TRUSTED_PROXIES` and, in
/// phase B, the dev provider depend on it.
pub fn is_loopback_http_origin(origin: &Url) -> bool {
    let loopback = match origin.host() {
        Some(Host::Domain(domain)) => domain == "localhost",
        Some(Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        None => false,
    };
    origin.scheme() == "http" && loopback
}

/// Decides the echo from the raw `ENABLE_WS_ECHO` and the already validated origin.
pub fn decide_echo(flag: Option<&str>, origin: &Url) -> EchoDecision {
    if flag != Some("1") {
        return EchoDecision::Off;
    }
    if is_loopback_http_origin(origin) {
        EchoDecision::Enabled
    } else {
        EchoDecision::Refused
    }
}

/// A base64url secret (padding optional) of at least [`INTERNAL_SECRET_MIN_BYTES`] decoded
/// bytes. Outside loopback its decoded bytes must differ from those of the public development
/// value, whatever the padding. Returns the text and the decoded bytes. The comparison is
/// constant-time, so the pattern is safe to copy.
fn secret(text: &str, dev_value: &str, loopback: bool) -> Option<(String, Vec<u8>)> {
    use subtle::ConstantTimeEq;
    let bytes = BASE64URL.decode(text).ok()?;
    if bytes.len() < INTERNAL_SECRET_MIN_BYTES {
        return None;
    }
    // Decoded bytes, not text: with or without padding, the same secret is the same secret.
    let dev_bytes = BASE64URL.decode(dev_value).unwrap_or_default();
    let is_dev_value = bool::from(bytes.as_slice().ct_eq(dev_bytes.as_slice()));
    if is_dev_value && !loopback {
        return None;
    }
    Some((text.to_owned(), bytes))
}

/// `DAILY_GRANT`: ASCII digits only (no sign, spaces or exponent), inside [`DAILY_GRANT_RANGE`].
fn parse_daily_grant(text: &str) -> Option<Coins> {
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    DAILY_GRANT_RANGE.contains(&value).then_some(Coins(value))
}

/// The cookie policy (RF-B9): the development names only with the feature (whose gate already
/// required a loopback origin).
#[cfg(feature = "dev-auth")]
fn cookie_policy(loopback: bool) -> CookiePolicy {
    if loopback {
        CookiePolicy::loopback_dev()
    } else {
        CookiePolicy::production()
    }
}

#[cfg(not(feature = "dev-auth"))]
fn cookie_policy(_loopback: bool) -> CookiePolicy {
    CookiePolicy::production()
}

/// Validates `PUBLIC_ORIGIN`: `http(s)://host[:port]` exactly as a browser serializes the
/// `Origin` header (no path, query, fragment, credentials, trailing slash, uppercase host or
/// default port). Same rule as the `web` validator (plan-a decision 19).
pub fn parse_public_origin(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    (url.origin().ascii_serialization() == value).then_some(url)
}

/// Longest OAuth client id or secret the server accepts (plan-c decision 3).
pub const MAX_CREDENTIAL_BYTES: usize = 256;

/// An OAuth client secret. `Debug` never shows it, and there is no `Display`.
#[derive(Clone)]
pub struct ClientSecret(String);

impl ClientSecret {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The secret, for the token request only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClientSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientSecret(..)")
    }
}

/// The OAuth app of one provider. `Debug` shows neither the id nor the secret.
#[derive(Clone)]
pub struct ClientCredentials {
    pub id: String,
    pub secret: ClientSecret,
}

impl fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientCredentials(..)")
    }
}

/// The real providers with credentials (RF-C1). `None`: the provider is off.
#[derive(Clone, Default)]
pub struct OAuthCredentials {
    pub google: Option<ClientCredentials>,
    pub discord: Option<ClientCredentials>,
}

impl OAuthCredentials {
    /// The names of the configured providers, in the order of the login buttons.
    pub fn configured(&self) -> Vec<&'static str> {
        [
            ("google", self.google.is_some()),
            ("discord", self.discord.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect()
    }
}

/// A client id or secret: visible ASCII (no space, no control), 1 to
/// [`MAX_CREDENTIAL_BYTES`] bytes. The shape of each provider is not checked, so the fillers of
/// `prod-local` pass (plan-c decision 3).
fn is_credential(text: &str) -> bool {
    (1..=MAX_CREDENTIAL_BYTES).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_graphic())
}

#[derive(Clone)]
pub struct Config {
    pub database: PgConnectOptions,
    /// Exact `Origin` the WebSocket handshake accepts.
    pub public_origin: String,
    pub bind_addr: IpAddr,
    pub port: u16,
    pub ws_max_connections: u32,
    pub rust_log: String,
    pub echo: EchoDecision,
    /// Proxies whose `X-Forwarded-For` is read (008 RF-A3). Empty by default.
    pub trusted_proxies: TrustedProxies,
    /// Shared with `web` (ADR-0008 D5). Required since phase B.
    pub internal_secret: InternalSecret,
    /// HMAC key of `ip_hash` (ADR-0008 D8).
    pub ip_hash_key: IpHashKey,
    /// Coins of the first-login `grant` (008 RF-B3).
    pub daily_grant: Coins,
    /// Cookie names and attributes (RF-B9).
    pub cookies: CookiePolicy,
    /// Google and Discord (RF-C1): both always outside loopback.
    pub oauth: OAuthCredentials,
}

// Hand-written so the connection options (with the password), the internal secret and the
// proxy addresses are never formatted.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database", &"<redacted>")
            .field("public_origin", &self.public_origin)
            .field("bind_addr", &self.bind_addr)
            .field("port", &self.port)
            .field("ws_max_connections", &self.ws_max_connections)
            .field("rust_log", &self.rust_log)
            .field("echo", &self.echo)
            .field("trusted_proxies", &self.trusted_proxies.len())
            .field("internal_secret", &"<redacted>")
            .field("ip_hash_key", &"<redacted>")
            .field("daily_grant", &self.daily_grant.0)
            .field("cookies", &self.cookies)
            .field("oauth", &self.oauth.configured())
            .finish()
    }
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    /// Reads the configuration through `lookup`. Variables are checked in a fixed order and
    /// the first invalid one is reported.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Result<Self, ConfigError> {
        let text = |var: &'static str| -> Result<Option<String>, ConfigError> {
            match lookup(var) {
                None => Ok(None),
                Some(raw) => raw
                    .into_string()
                    .map(Some)
                    .map_err(|_| ConfigError::Invalid { var }),
            }
        };
        let required = |var: &'static str| -> Result<String, ConfigError> {
            text(var)?.ok_or(ConfigError::Invalid { var })
        };

        const DATABASE_URL: &str = "DATABASE_URL";
        let database_url = required(DATABASE_URL)?;
        // sqlx parses any scheme; only a Postgres URL is valid here.
        let is_postgres = Url::parse(&database_url)
            .is_ok_and(|url| matches!(url.scheme(), "postgres" | "postgresql"));
        let database = is_postgres
            .then(|| PgConnectOptions::from_str(&database_url).ok())
            .flatten()
            .ok_or(ConfigError::Invalid { var: DATABASE_URL })?;

        const PUBLIC_ORIGIN: &str = "PUBLIC_ORIGIN";
        let origin_text = required(PUBLIC_ORIGIN)?;
        let origin =
            parse_public_origin(&origin_text).ok_or(ConfigError::Invalid { var: PUBLIC_ORIGIN })?;

        const BIND_ADDR: &str = "BIND_ADDR";
        let bind_addr = match text(BIND_ADDR)? {
            None => DEFAULT_BIND_ADDR,
            Some(value) => value
                .parse::<IpAddr>()
                .map_err(|_| ConfigError::Invalid { var: BIND_ADDR })?,
        };

        const PORT: &str = "PORT";
        let port = match text(PORT)? {
            None => DEFAULT_PORT,
            Some(value) => value
                .parse::<u16>()
                .map_err(|_| ConfigError::Invalid { var: PORT })?,
        };

        const WS_MAX_CONNECTIONS: &str = "WS_MAX_CONNECTIONS";
        let ws_max_connections = match text(WS_MAX_CONNECTIONS)? {
            None => DEFAULT_WS_MAX_CONNECTIONS,
            Some(value) => value
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=65_535).contains(n))
                .ok_or(ConfigError::Invalid {
                    var: WS_MAX_CONNECTIONS,
                })?,
        };

        const RUST_LOG: &str = "RUST_LOG";
        let rust_log = match text(RUST_LOG)? {
            None => DEFAULT_RUST_LOG.to_owned(),
            Some(value) if value.trim().is_empty() => DEFAULT_RUST_LOG.to_owned(),
            Some(value) => {
                EnvFilter::try_new(&value).map_err(|_| ConfigError::Invalid { var: RUST_LOG })?;
                value
            }
        };

        const ENABLE_WS_ECHO: &str = "ENABLE_WS_ECHO";
        let echo = decide_echo(text(ENABLE_WS_ECHO)?.as_deref(), &origin);

        const TRUSTED_PROXIES: &str = "TRUSTED_PROXIES";
        let trusted_proxies = match text(TRUSTED_PROXIES)? {
            None => TrustedProxies::default(),
            Some(value) => TrustedProxies::parse(&value, is_loopback_http_origin(&origin))
                .map_err(|_| ConfigError::Invalid {
                    var: TRUSTED_PROXIES,
                })?,
        };

        // "On loopback" is one predicate for every rule that depends on it (008 plan-b
        // decision 5): RF-A3, RF-B6, RF-B9 and RF-B18.
        let loopback = is_loopback_http_origin(&origin);

        const INTERNAL_API_SECRET: &str = "INTERNAL_API_SECRET";
        let internal_text = secret(
            &required(INTERNAL_API_SECRET)?,
            DEV_INTERNAL_API_SECRET,
            loopback,
        )
        .ok_or(ConfigError::Invalid {
            var: INTERNAL_API_SECRET,
        })?;
        let internal_secret = InternalSecret::new(&internal_text.0);

        const IP_HASH_KEY: &str = "IP_HASH_KEY";
        let (_, ip_hash_bytes) = secret(&required(IP_HASH_KEY)?, DEV_IP_HASH_KEY, loopback)
            .ok_or(ConfigError::Invalid { var: IP_HASH_KEY })?;
        let ip_hash_key = IpHashKey::new(ip_hash_bytes);

        const DAILY_GRANT: &str = "DAILY_GRANT";
        let daily_grant = parse_daily_grant(&required(DAILY_GRANT)?)
            .ok_or(ConfigError::Invalid { var: DAILY_GRANT })?;

        // RF-C1 and plan-c decision 3: a provider with both variables is on, one with neither
        // is off (only on loopback: outside it the four are required), and a half pair is a
        // typo that stops the start. An empty value counts as missing.
        let credential = |var: &'static str| -> Result<Option<String>, ConfigError> {
            match text(var)? {
                None => Ok(None),
                Some(value) if value.is_empty() => Ok(None),
                Some(value) if is_credential(&value) => Ok(Some(value)),
                Some(_) => Err(ConfigError::Invalid { var }),
            }
        };
        let pair = |id_var: &'static str,
                    secret_var: &'static str|
         -> Result<Option<ClientCredentials>, ConfigError> {
            match (credential(id_var)?, credential(secret_var)?) {
                (Some(id), Some(secret)) => Ok(Some(ClientCredentials {
                    id,
                    secret: ClientSecret(secret),
                })),
                (None, None) if loopback => Ok(None),
                (None, _) => Err(ConfigError::Invalid { var: id_var }),
                (Some(_), None) => Err(ConfigError::Invalid { var: secret_var }),
            }
        };
        let oauth = OAuthCredentials {
            google: pair("GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET")?,
            discord: pair("DISCORD_CLIENT_ID", "DISCORD_CLIENT_SECRET")?,
        };

        // RF-B6: the dev provider only on a loopback http origin and a loopback BIND_ADDR.
        // `Origin` is a CSRF defense, not authentication: without this gate anyone on the
        // local network could ask for a session of any `dev_identity`.
        #[cfg(feature = "dev-auth")]
        {
            if !loopback {
                return Err(ConfigError::Invalid { var: "DEV_AUTH" });
            }
            if !bind_addr.is_loopback() || bind_addr.is_unspecified() {
                return Err(ConfigError::Invalid { var: BIND_ADDR });
            }
        }
        let cookies = cookie_policy(loopback);

        Ok(Self {
            database,
            public_origin: origin_text,
            bind_addr,
            port,
            ws_max_connections,
            rust_log,
            echo,
            trusted_proxies,
            internal_secret,
            ip_hash_key,
            daily_grant,
            cookies,
            oauth,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const DB: &str = "postgres://torneo:torneo_dev@127.0.0.1:5432/torneo";
    const ORIGIN: &str = "http://localhost:5173";

    fn load(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(*v)))
            .collect();
        Config::from_lookup(|name| map.get(name).cloned())
    }

    /// The variables every valid config needs, on a loopback origin (and, with `dev-auth`, a
    /// loopback `BIND_ADDR`, which the gate requires).
    fn base() -> Vec<(&'static str, &'static str)> {
        let mut vars = vec![
            ("DATABASE_URL", DB),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("INTERNAL_API_SECRET", OTHER_SECRET),
            ("IP_HASH_KEY", OTHER_KEY),
            ("DAILY_GRANT", "100"),
            ("GOOGLE_CLIENT_ID", GOOGLE_ID),
            ("GOOGLE_CLIENT_SECRET", GOOGLE_SECRET),
            ("DISCORD_CLIENT_ID", DISCORD_ID),
            ("DISCORD_CLIENT_SECRET", DISCORD_SECRET),
        ];
        if cfg!(feature = "dev-auth") {
            vars.push(("BIND_ADDR", "127.0.0.1"));
        }
        vars
    }

    /// A 32-byte base64url secret that is not a development value.
    const OTHER_SECRET: &str = "b3RoZXItc2VjcmV0LW90aGVyLXNlY3JldC1vdGhlcis";
    const OTHER_KEY: &str = "b3RoZXIta2V5LW90aGVyLWtleS1vdGhlci1rZXktb3RoZXI";

    fn with<'a>(extra: &[(&'a str, &'a str)]) -> Result<Config, ConfigError> {
        let mut vars = base();
        for (k, v) in extra {
            vars.retain(|(name, _)| name != k);
            vars.push((*k, *v));
        }
        load(&vars)
    }

    /// Test-only OAuth credentials (never real ones): fillers in the shape of nothing.
    const GOOGLE_ID: &str = "test-google-id-canary";
    const GOOGLE_SECRET: &str = "test-google-secret-canary";
    const DISCORD_ID: &str = "test-discord-id-canary";
    const DISCORD_SECRET: &str = "test-discord-secret-canary";
    const PROVIDER_VARS: [&str; 4] = [
        "GOOGLE_CLIENT_ID",
        "GOOGLE_CLIENT_SECRET",
        "DISCORD_CLIENT_ID",
        "DISCORD_CLIENT_SECRET",
    ];

    /// `base` without `removed`, with `extra` on top.
    fn without<'a>(removed: &[&str], extra: &[(&'a str, &'a str)]) -> Result<Config, ConfigError> {
        let mut vars: Vec<_> = base()
            .into_iter()
            .filter(|(k, _)| !removed.contains(k))
            .collect();
        for (k, v) in extra {
            vars.retain(|(name, _)| name != k);
            vars.push((*k, *v));
        }
        load(&vars)
    }

    fn configured(config: &Config) -> (bool, bool) {
        (
            config.oauth.google.is_some(),
            config.oauth.discord.is_some(),
        )
    }

    #[test]
    fn on_loopback_each_provider_follows_its_pair() {
        assert_eq!(configured(&with(&[]).unwrap()), (true, true));
        assert_eq!(
            configured(&without(&PROVIDER_VARS, &[]).unwrap()),
            (false, false)
        );
        assert_eq!(
            configured(&without(&PROVIDER_VARS[2..], &[]).unwrap()),
            (true, false)
        );
        assert_eq!(
            configured(&without(&PROVIDER_VARS[..2], &[]).unwrap()),
            (false, true)
        );
        let config = with(&[]).unwrap();
        let google = config.oauth.google.unwrap();
        assert_eq!(google.id, GOOGLE_ID);
        assert_eq!(google.secret.expose(), GOOGLE_SECRET);
        let discord = config.oauth.discord.unwrap();
        assert_eq!(discord.id, DISCORD_ID);
        assert_eq!(discord.secret.expose(), DISCORD_SECRET);
    }

    /// A half pair is a typo, not "provider off" (plan-c decision 3).
    #[test]
    fn on_loopback_a_half_pair_names_the_missing_variable() {
        for var in PROVIDER_VARS {
            assert_eq!(without(&[var], &[]).unwrap_err().var(), var, "{var}");
            // An empty value is the same as a missing one.
            assert_eq!(with(&[(var, "")]).unwrap_err().var(), var, "{var}");
        }
        // Both empty: the provider is off.
        assert_eq!(
            configured(&with(&[("GOOGLE_CLIENT_ID", ""), ("GOOGLE_CLIENT_SECRET", "")]).unwrap()),
            (false, true)
        );
    }

    // Non-loopback origins never start with `dev-auth` (see `the_dev_auth_gate`).
    #[cfg(not(feature = "dev-auth"))]
    #[test]
    fn outside_loopback_the_four_credentials_are_required() {
        for origin in ["https://torneo.example", "https://localhost"] {
            let public = [("PUBLIC_ORIGIN", origin)];
            assert_eq!(
                configured(&with(&public).unwrap()),
                (true, true),
                "{origin}"
            );
            for var in PROVIDER_VARS {
                assert_eq!(without(&[var], &public).unwrap_err().var(), var, "{origin}");
                let mut empty = public.to_vec();
                empty.push((var, ""));
                assert_eq!(with(&empty).unwrap_err().var(), var, "{origin}");
            }
            // In order: the first missing one is reported.
            assert_eq!(
                without(&PROVIDER_VARS, &public).unwrap_err().var(),
                "GOOGLE_CLIENT_ID"
            );
            assert_eq!(
                without(&PROVIDER_VARS[2..], &public).unwrap_err().var(),
                "DISCORD_CLIENT_ID"
            );
        }
    }

    #[test]
    fn credentials_are_visible_ascii_of_1_to_256_bytes() {
        let longest = "x".repeat(256);
        let too_long = format!("canary{}", "x".repeat(251));
        assert_eq!(too_long.len(), 257);
        for var in PROVIDER_VARS {
            assert!(with(&[(var, &longest)]).is_ok(), "{var}");
            assert!(with(&[(var, "!~")]).is_ok(), "{var}");
            for value in [
                too_long.as_str(),
                "canary with spaces",
                " canary",
                "canary\t",
                "canary\u{7f}",
                "canary\u{f1}",
                "canary\n",
            ] {
                let err = with(&[(var, value)]).unwrap_err();
                assert_eq!(err.var(), var, "{var} {value:?}");
                assert!(!err.to_string().contains("canary"));
                assert!(!format!("{err:?}").contains("canary"));
            }
        }
    }

    /// The provider variables come after `DAILY_GRANT` (plan-c decision 3).
    #[test]
    fn provider_variables_are_read_after_the_phase_b_ones() {
        assert_eq!(
            without(&PROVIDER_VARS[..1], &[("DAILY_GRANT", "0")])
                .unwrap_err()
                .var(),
            "DAILY_GRANT"
        );
    }

    #[test]
    fn debug_shows_only_which_providers_are_configured() {
        let config = with(&[]).unwrap();
        let shown = format!("{config:?}");
        assert!(shown.contains(r#"oauth: ["google", "discord"]"#), "{shown}");
        for value in [GOOGLE_ID, GOOGLE_SECRET, DISCORD_ID, DISCORD_SECRET] {
            assert!(!shown.contains(value), "{value} in {shown}");
        }
        assert!(!shown.contains("canary"), "{shown}");
        let google = config.oauth.google.unwrap();
        let shown = format!("{google:?} {:?}", google.secret);
        assert!(!shown.contains(GOOGLE_SECRET), "{shown}");
        assert!(!shown.contains("canary"), "{shown}");
        let none = without(&PROVIDER_VARS, &[]).unwrap();
        assert!(format!("{none:?}").contains("oauth: []"));
    }

    fn invalid(var: &'static str) -> Result<Config, ConfigError> {
        Err(ConfigError::Invalid { var })
    }

    #[test]
    fn defaults() {
        let config = with(&[]).unwrap();
        assert_eq!(config.public_origin, ORIGIN);
        assert_eq!(config.port, 8080);
        #[cfg(not(feature = "dev-auth"))]
        assert_eq!(config.bind_addr, DEFAULT_BIND_ADDR);
        assert_eq!(config.ws_max_connections, 2000);
        assert_eq!(config.rust_log, "info");
        assert_eq!(config.echo, EchoDecision::Off);
        assert!(config.trusted_proxies.is_empty());
        assert_eq!(config.daily_grant, Coins(100));
    }

    #[test]
    fn phase_b_variables_are_required_in_order() {
        // Each missing variable is reported by name, in the fixed order of `from_lookup`.
        for (missing, var) in [
            ("INTERNAL_API_SECRET", "INTERNAL_API_SECRET"),
            ("IP_HASH_KEY", "IP_HASH_KEY"),
            ("DAILY_GRANT", "DAILY_GRANT"),
        ] {
            let vars: Vec<_> = base().into_iter().filter(|(k, _)| *k != missing).collect();
            assert_eq!(load(&vars).unwrap_err().var(), var);
        }
    }

    #[test]
    fn daily_grant_is_an_integer_from_1_to_10000() {
        for (value, coins) in [("1", 1), ("100", 100), ("10000", 10_000)] {
            assert_eq!(
                with(&[("DAILY_GRANT", value)]).unwrap().daily_grant,
                Coins(coins)
            );
        }
        for value in [
            "0",
            "10001",
            "1.5",
            "+5",
            "-1",
            "",
            " 5",
            "5 ",
            "1e3",
            "abc",
            "99999999999",
        ] {
            assert_eq!(
                with(&[("DAILY_GRANT", value)]).unwrap_err().var(),
                "DAILY_GRANT",
                "{value:?}"
            );
        }
    }

    #[test]
    fn ip_hash_key_follows_the_secret_rule() {
        let config = with(&[("IP_HASH_KEY", OTHER_SECRET)]).unwrap();
        assert_eq!(config.ip_hash_key.len(), 32);
        for value in [
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "+/8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "canary key",
            "",
        ] {
            let err = with(&[("IP_HASH_KEY", value)]).unwrap_err();
            assert_eq!(err.var(), "IP_HASH_KEY", "{value:?}");
            assert!(!err.to_string().contains("canary"));
        }
    }

    #[test]
    fn development_secrets_only_on_a_loopback_origin() {
        // Loopback: the .env.example values are fine.
        assert!(
            with(&[
                ("INTERNAL_API_SECRET", DEV_INTERNAL_API_SECRET),
                ("IP_HASH_KEY", DEV_IP_HASH_KEY),
            ])
            .is_ok()
        );
        for origin in [
            "https://torneo.example",
            "http://torneo.example",
            "https://localhost",
        ] {
            let public = [
                ("PUBLIC_ORIGIN", origin),
                ("INTERNAL_API_SECRET", OTHER_SECRET),
                ("IP_HASH_KEY", OTHER_SECRET),
            ];
            #[cfg(not(feature = "dev-auth"))]
            assert!(with(&public).is_ok(), "{origin}");
            let mut dev_secret = public.to_vec();
            dev_secret[1] = ("INTERNAL_API_SECRET", DEV_INTERNAL_API_SECRET);
            assert_eq!(
                with(&dev_secret).unwrap_err().var(),
                "INTERNAL_API_SECRET",
                "{origin}"
            );
            let mut dev_key = public.to_vec();
            dev_key[2] = ("IP_HASH_KEY", DEV_IP_HASH_KEY);
            assert_eq!(with(&dev_key).unwrap_err().var(), "IP_HASH_KEY", "{origin}");
        }
    }

    /// The padded form of a development value decodes to the same bytes: refused too.
    #[test]
    fn padded_development_secrets_are_refused_outside_loopback() {
        for origin in ["https://torneo.example", "https://localhost"] {
            let internal = format!("{DEV_INTERNAL_API_SECRET}=");
            let err = with(&[
                ("PUBLIC_ORIGIN", origin),
                ("INTERNAL_API_SECRET", &internal),
            ])
            .unwrap_err();
            assert_eq!(err.var(), "INTERNAL_API_SECRET", "{origin}");
            let key = format!("{DEV_IP_HASH_KEY}=");
            let err = with(&[
                ("PUBLIC_ORIGIN", origin),
                ("INTERNAL_API_SECRET", OTHER_SECRET),
                ("IP_HASH_KEY", &key),
            ])
            .unwrap_err();
            assert_eq!(err.var(), "IP_HASH_KEY", "{origin}");
        }
    }

    #[test]
    fn the_env_example_carries_the_development_secrets() {
        let example = include_str!("../.env.example");
        assert!(example.contains(&format!(
            "\nINTERNAL_API_SECRET={DEV_INTERNAL_API_SECRET}\n"
        )));
        assert!(example.contains(&format!("\nIP_HASH_KEY={DEV_IP_HASH_KEY}\n")));
        assert!(example.contains("\nDAILY_GRANT=100\n"));
        // The example must itself be a valid development config.
        for line in example
            .lines()
            .filter(|l| !l.starts_with('#') && l.contains('='))
        {
            let (name, _) = line.split_once('=').unwrap();
            assert!(!name.is_empty(), "{line}");
        }
    }

    #[test]
    fn the_loopback_table_is_shared_with_web() {
        let table: Vec<(String, bool)> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/__fixtures__/loopback-origins.json"
        )))
        .unwrap();
        assert!(table.len() >= 9);
        for (origin, expected) in table {
            // An origin `parse_public_origin` rejects is never "on loopback".
            let actual =
                parse_public_origin(&origin).is_some_and(|url| is_loopback_http_origin(&url));
            assert_eq!(actual, expected, "{origin}");
        }
    }

    /// RF-B18 and the security note of the spec: one predicate decides "on loopback".
    #[test]
    fn no_other_module_reimplements_the_loopback_rule() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src.clone()];
        let needles = [
            ["Host::", "Domain"].concat(),
            ["\"local", "host\""].concat(),
        ];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path == src.join("config.rs") || path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                // Only production code: the test modules may use loopback origins as data.
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                for needle in &needles {
                    assert!(
                        !code.contains(needle.as_str()),
                        "{} uses {needle}",
                        path.display()
                    );
                }
            }
        }
    }

    /// The Debug of the config never shows the decoded key bytes either, in any form.
    #[test]
    fn debug_never_shows_the_raw_key_bytes() {
        let config = with(&[
            ("IP_HASH_KEY", OTHER_KEY),
            ("INTERNAL_API_SECRET", OTHER_SECRET),
        ])
        .unwrap();
        let shown = format!("{config:?}");
        for text in [OTHER_KEY, OTHER_SECRET] {
            let bytes = BASE64URL.decode(text).unwrap();
            let list = format!("{bytes:?}");
            let inner = list.trim_start_matches('[').trim_end_matches(']');
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            // Both test keys decode to ASCII ("other-key-…", "other-secret-…").
            let ascii = String::from_utf8(bytes.clone()).unwrap();
            for raw in [inner.to_owned(), hex, ascii, inner[..12].to_owned()] {
                assert!(!shown.contains(&raw), "{raw} in {shown}");
            }
        }
        assert!(!shown.contains("other-"), "{shown}");
    }

    #[test]
    fn debug_redacts_every_secret() {
        let config = with(&[
            ("INTERNAL_API_SECRET", OTHER_SECRET),
            ("IP_HASH_KEY", OTHER_SECRET),
            ("DATABASE_URL", "postgres://u:pw-canary@127.0.0.1/db"),
        ])
        .unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains(OTHER_SECRET), "{shown}");
        assert!(!shown.contains("pw-canary"), "{shown}");
        assert!(!shown.contains(DEV_IP_HASH_KEY), "{shown}");
        assert!(shown.contains("daily_grant"), "{shown}");
    }

    #[test]
    fn the_cookie_policy_follows_the_feature_and_the_origin() {
        let production = with(&[]).unwrap().cookies;
        #[cfg(not(feature = "dev-auth"))]
        assert_eq!(production, CookiePolicy::production());
        #[cfg(feature = "dev-auth")]
        assert_eq!(production, CookiePolicy::loopback_dev());
        assert_eq!(CookiePolicy::production().session_name(), "__Host-session");
        assert_eq!(CookiePolicy::production().flow_name(), "__Host-oauth");
        assert!(CookiePolicy::production().secure());
    }

    #[cfg(feature = "dev-auth")]
    #[test]
    fn the_dev_auth_gate() {
        // A non-loopback origin stops the start with `var = DEV_AUTH`.
        for origin in [
            "https://torneo.example",
            "http://torneo.example",
            "https://localhost",
        ] {
            let err = with(&[
                ("PUBLIC_ORIGIN", origin),
                ("INTERNAL_API_SECRET", OTHER_SECRET),
                ("IP_HASH_KEY", OTHER_SECRET),
            ])
            .unwrap_err();
            assert_eq!(err.var(), "DEV_AUTH", "{origin}");
        }
        // A non-loopback BIND_ADDR (the default included) stops it with `var = BIND_ADDR`.
        let vars: Vec<_> = base()
            .into_iter()
            .filter(|(k, _)| *k != "BIND_ADDR")
            .collect();
        assert_eq!(load(&vars).unwrap_err().var(), "BIND_ADDR");
        for addr in ["0.0.0.0", "192.0.2.1", "::"] {
            assert_eq!(
                with(&[("BIND_ADDR", addr)]).unwrap_err().var(),
                "BIND_ADDR",
                "{addr}"
            );
        }
        for addr in ["127.0.0.1", "::1"] {
            assert!(with(&[("BIND_ADDR", addr)]).is_ok(), "{addr}");
        }
        let dev = CookiePolicy::loopback_dev();
        assert_eq!(dev.session_name(), "ta_dev_session");
        assert_eq!(dev.flow_name(), "ta_dev_oauth");
        assert!(!dev.secure());
    }

    #[test]
    fn missing_required_variables_name_the_variable() {
        assert_eq!(
            load(&[("PUBLIC_ORIGIN", ORIGIN)]).map(|_| ()),
            invalid("DATABASE_URL").map(|_| ())
        );
        assert_eq!(
            load(&[("DATABASE_URL", DB)]).map(|_| ()),
            invalid("PUBLIC_ORIGIN").map(|_| ())
        );
    }

    #[test]
    fn invalid_database_url() {
        let err = with(&[("DATABASE_URL", "mysql://secret-canary@x/y")]).unwrap_err();
        assert_eq!(err.var(), "DATABASE_URL");
        assert!(!err.to_string().contains("secret-canary"));
    }

    // Non-loopback origins never start with `dev-auth` (see `the_dev_auth_gate`).
    #[cfg(not(feature = "dev-auth"))]
    #[test]
    fn valid_public_origins() {
        for origin in [
            "http://localhost:5173",
            "https://torneo.example",
            "http://127.0.0.1:8080",
            "http://[::1]:5173",
        ] {
            assert_eq!(
                with(&[("PUBLIC_ORIGIN", origin)]).unwrap().public_origin,
                origin
            );
        }
    }

    #[test]
    fn invalid_public_origins_never_appear_in_the_error() {
        for origin in [
            "http://localhost:5173/",
            "http://localhost:5173/canary-path",
            "http://localhost:5173?canary=1",
            "http://localhost:5173#canary",
            "http://user:canary@localhost:5173",
            "ftp://canary.example",
            "",
            "canary",
            "http://CANARY.example",
            "https://canary.example:443",
            "http://canary.example:80",
            "HTTP://canary.example",
        ] {
            let err = with(&[("PUBLIC_ORIGIN", origin)]).unwrap_err();
            assert_eq!(err.var(), "PUBLIC_ORIGIN", "{origin:?}");
            let shown = err.to_string();
            assert!(!shown.contains("canary"), "{shown}");
            assert!(!format!("{err:?}").contains("canary"));
        }
    }

    #[test]
    fn port() {
        assert_eq!(with(&[("PORT", "3000")]).unwrap().port, 3000);
        assert_eq!(with(&[("PORT", "0")]).unwrap().port, 0);
        for value in ["65536", "-1", "abc", ""] {
            assert_eq!(with(&[("PORT", value)]).unwrap_err().var(), "PORT");
        }
    }

    #[test]
    fn ws_max_connections() {
        assert_eq!(
            with(&[("WS_MAX_CONNECTIONS", "1")])
                .unwrap()
                .ws_max_connections,
            1
        );
        assert_eq!(
            with(&[("WS_MAX_CONNECTIONS", "65535")])
                .unwrap()
                .ws_max_connections,
            65_535
        );
        for value in ["0", "65536", "-5", "many"] {
            assert_eq!(
                with(&[("WS_MAX_CONNECTIONS", value)]).unwrap_err().var(),
                "WS_MAX_CONNECTIONS"
            );
        }
    }

    #[test]
    fn bind_addr() {
        assert_eq!(
            with(&[("BIND_ADDR", "127.0.0.1")]).unwrap().bind_addr,
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        );
        assert_eq!(
            with(&[("BIND_ADDR", "::1")]).unwrap().bind_addr,
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        );
        for value in ["no-es-ip", "localhost", "my-machine.local", ""] {
            assert_eq!(
                with(&[("BIND_ADDR", value)]).unwrap_err().var(),
                "BIND_ADDR"
            );
        }
    }

    #[test]
    fn rust_log() {
        assert_eq!(
            with(&[("RUST_LOG", "debug,sqlx=trace")]).unwrap().rust_log,
            "debug,sqlx=trace"
        );
        assert_eq!(with(&[("RUST_LOG", "")]).unwrap().rust_log, "info");
        for value in ["sqlx=loud", "[[[", "info,=,"] {
            assert_eq!(
                with(&[("RUST_LOG", value)]).unwrap_err().var(),
                "RUST_LOG",
                "{value:?}"
            );
        }
    }

    // Non-loopback origins never start with `dev-auth` (see `the_dev_auth_gate`).
    #[cfg(not(feature = "dev-auth"))]
    #[test]
    fn echo_decision_table() {
        let cases: [(&[(&'static str, &'static str)], EchoDecision); 7] = [
            (&[("ENABLE_WS_ECHO", "1")], EchoDecision::Enabled),
            (
                &[
                    ("ENABLE_WS_ECHO", "1"),
                    ("PUBLIC_ORIGIN", "http://127.0.0.1:8080"),
                ],
                EchoDecision::Enabled,
            ),
            (
                &[
                    ("ENABLE_WS_ECHO", "1"),
                    ("PUBLIC_ORIGIN", "http://[::1]:5173"),
                ],
                EchoDecision::Enabled,
            ),
            (
                &[
                    ("ENABLE_WS_ECHO", "1"),
                    ("PUBLIC_ORIGIN", "https://torneo.example"),
                ],
                EchoDecision::Refused,
            ),
            (
                &[
                    ("ENABLE_WS_ECHO", "1"),
                    ("PUBLIC_ORIGIN", "http://torneo.example"),
                ],
                EchoDecision::Refused,
            ),
            (&[("ENABLE_WS_ECHO", "true")], EchoDecision::Off),
            (&[], EchoDecision::Off),
        ];
        for (vars, expected) in cases {
            assert_eq!(with(vars).unwrap().echo, expected, "{vars:?}");
        }
        assert_eq!(
            EchoDecision::Enabled.startup_event(),
            Some("ws.echo_enabled")
        );
        assert_eq!(
            EchoDecision::Refused.startup_event(),
            Some("ws.echo_refused")
        );
        assert_eq!(EchoDecision::Off.startup_event(), None);
        assert!(!EchoDecision::Refused.is_enabled());
    }

    #[test]
    fn trusted_proxies() {
        assert!(
            with(&[("TRUSTED_PROXIES", "")])
                .unwrap()
                .trusted_proxies
                .is_empty()
        );
        let config = with(&[(
            "TRUSTED_PROXIES",
            " 10.250.0.2/32 , 10.250.0.3,2001:db8::1/128 ,2001:db8::2",
        )])
        .unwrap();
        assert_eq!(config.trusted_proxies.len(), 4);
        assert!(
            config
                .trusted_proxies
                .contains("10.250.0.2".parse().unwrap())
        );
        for value in [
            "10.250.0.0/24",
            "2001:db8::/64",
            "10.250.0.2/0",
            "10.250.0.2/032",
            "10.250.0.2/33",
            "10.250.0.2,,10.250.0.3",
            "canary-proxy",
            "0.0.0.0",
            "::",
            "224.0.0.1",
            "ff02::1",
            "255.255.255.255",
        ] {
            let err = with(&[("TRUSTED_PROXIES", value)]).unwrap_err();
            assert_eq!(err.var(), "TRUSTED_PROXIES", "{value:?}");
            assert!(!err.to_string().contains(value), "{value:?}");
            assert!(!format!("{err:?}").contains(value), "{value:?}");
        }
    }

    #[test]
    fn loopback_proxies_need_a_loopback_http_origin() {
        for proxy in ["127.0.0.1/32", "::1"] {
            assert_eq!(
                with(&[("TRUSTED_PROXIES", proxy)])
                    .unwrap()
                    .trusted_proxies
                    .len(),
                1
            );
            for origin in ["https://torneo.example", "http://torneo.example"] {
                let err =
                    with(&[("TRUSTED_PROXIES", proxy), ("PUBLIC_ORIGIN", origin)]).unwrap_err();
                assert_eq!(err.var(), "TRUSTED_PROXIES", "{proxy} {origin}");
            }
        }
    }

    #[test]
    fn loopback_http_origin_predicate() {
        for (origin, expected) in [
            ("http://localhost:5173", true),
            ("http://127.0.0.1:8080", true),
            ("http://[::1]:5173", true),
            ("https://localhost:5173", false),
            ("http://torneo.example", false),
            ("http://127.0.0.2:8080", false),
        ] {
            let url = Url::parse(origin).unwrap();
            assert_eq!(is_loopback_http_origin(&url), expected, "{origin}");
        }
    }

    #[test]
    fn internal_api_secret() {
        // 32 bytes, unpadded and padded; 33 bytes.
        let unpadded = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let padded = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let url_chars = "-_8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for value in [unpadded, padded, url_chars] {
            assert!(with(&[("INTERNAL_API_SECRET", value)]).is_ok(), "{value}");
        }
        for value in [
            // 31 bytes.
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            // Standard alphabet.
            "+/8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "canary secret with spaces and more than enough characters",
            "",
        ] {
            let err = with(&[("INTERNAL_API_SECRET", value)]).unwrap_err();
            assert_eq!(err.var(), "INTERNAL_API_SECRET", "{value:?}");
            assert!(!err.to_string().contains("canary"));
        }
    }

    #[test]
    fn debug_never_shows_the_secret_nor_the_proxies() {
        let secret = "Y2FuYXJ5LWNhbmFyeS1jYW5hcnktY2FuYXJ5LWNhbmFyeQ";
        let config = with(&[
            ("INTERNAL_API_SECRET", secret),
            ("TRUSTED_PROXIES", "10.250.0.2"),
        ])
        .unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains(secret), "{shown}");
        assert!(!shown.contains("10.250"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn debug_never_shows_the_database_password() {
        let config = with(&[("DATABASE_URL", "postgres://u:pw-canary@127.0.0.1/db")]).unwrap();
        assert!(!format!("{config:?}").contains("pw-canary"));
    }
}
