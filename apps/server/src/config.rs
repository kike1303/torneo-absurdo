//! Configuration from environment variables (RF-B8, plan-b decision 3).
//!
//! Read once at startup through [`Config::from_lookup`], a pure function over a lookup, so the
//! tests use a table instead of the process environment. Errors carry only the variable
//! name: the value is never stored in the error, so it can never be logged or printed.

use std::ffi::OsString;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use sqlx::postgres::PgConnectOptions;
use tracing_subscriber::EnvFilter;
use url::{Host, Url};

pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_WS_MAX_CONNECTIONS: u32 = 2000;
pub const DEFAULT_RUST_LOG: &str = "info";
/// Default bind address, what the container needs. `server healthcheck` always calls
/// `127.0.0.1`, so it assumes `BIND_ADDR` is `0.0.0.0` or `127.0.0.1`: with any other address
/// (or `::1`) the healthcheck fails. The production compose never sets `BIND_ADDR` (phase C).
pub const DEFAULT_BIND_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

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

/// Decides the echo from the raw `ENABLE_WS_ECHO` and the already validated origin.
pub fn decide_echo(flag: Option<&str>, origin: &Url) -> EchoDecision {
    if flag != Some("1") {
        return EchoDecision::Off;
    }
    let loopback = match origin.host() {
        Some(Host::Domain(domain)) => domain == "localhost",
        Some(Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        None => false,
    };
    if origin.scheme() == "http" && loopback {
        EchoDecision::Enabled
    } else {
        EchoDecision::Refused
    }
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
}

// Hand-written so the connection options (with the password) are never formatted.
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

        Ok(Self {
            database,
            public_origin: origin_text,
            bind_addr,
            port,
            ws_max_connections,
            rust_log,
            echo,
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

    fn with<'a>(extra: &[(&'a str, &'a str)]) -> Result<Config, ConfigError> {
        let mut vars = vec![("DATABASE_URL", DB), ("PUBLIC_ORIGIN", ORIGIN)];
        for (k, v) in extra {
            vars.retain(|(name, _)| name != k);
            vars.push((*k, *v));
        }
        load(&vars)
    }

    fn invalid(var: &'static str) -> Result<Config, ConfigError> {
        Err(ConfigError::Invalid { var })
    }

    #[test]
    fn defaults() {
        let config = with(&[]).unwrap();
        assert_eq!(config.public_origin, ORIGIN);
        assert_eq!(config.port, 8080);
        assert_eq!(config.bind_addr, DEFAULT_BIND_ADDR);
        assert_eq!(config.ws_max_connections, 2000);
        assert_eq!(config.rust_log, "info");
        assert_eq!(config.echo, EchoDecision::Off);
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
    fn debug_never_shows_the_database_password() {
        let config = with(&[("DATABASE_URL", "postgres://u:pw-canary@127.0.0.1/db")]).unwrap();
        assert!(!format!("{config:?}").contains("pw-canary"));
    }
}
