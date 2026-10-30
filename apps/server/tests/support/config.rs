//! A `Config` for the in-process tests, read through `Config::from_lookup` over a table, so
//! the tests go through the same validation as the binary (008 plan-b decision 3.2).

use std::collections::HashMap;
use std::ffi::OsString;

use server::config::Config;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use super::app::ORIGIN;

/// A database URL nothing listens on: the pool is lazy and only the tests that need the
/// database replace it.
pub const UNUSED_DB: &str = "postgres://torneo:unused@127.0.0.1:1/unused";

/// Test-only OAuth credentials (plan-c decision 3): not real ones, and the fake providers of
/// `support::fake_provider` expect exactly these. Canaries of the log tests too.
pub const GOOGLE_CLIENT_ID: &str = "test-google-client-id";
pub const GOOGLE_CLIENT_SECRET: &str = "test-google-secret-c4n4ry";
pub const DISCORD_CLIENT_ID: &str = "test-discord-client-id";
pub const DISCORD_CLIENT_SECRET: &str = "test-discord-secret-c4n4ry";

/// The four variables that turn Google and Discord on (required outside loopback).
pub const PROVIDER_VARS: [(&str, &str); 4] = [
    ("GOOGLE_CLIENT_ID", GOOGLE_CLIENT_ID),
    ("GOOGLE_CLIENT_SECRET", GOOGLE_CLIENT_SECRET),
    ("DISCORD_CLIENT_ID", DISCORD_CLIENT_ID),
    ("DISCORD_CLIENT_SECRET", DISCORD_CLIENT_SECRET),
];

/// The variables every in-process test starts from. Without the providers: a test that needs
/// them adds [`PROVIDER_VARS`].
pub fn base_vars() -> Vec<(&'static str, String)> {
    vec![
        ("DATABASE_URL", UNUSED_DB.to_owned()),
        ("PUBLIC_ORIGIN", ORIGIN.to_owned()),
        ("ENABLE_WS_ECHO", "1".to_owned()),
        ("BIND_ADDR", "127.0.0.1".to_owned()),
        ("INTERNAL_API_SECRET", super::app::SECRET.to_owned()),
        ("IP_HASH_KEY", super::app::IP_HASH_KEY.to_owned()),
        ("DAILY_GRANT", "100".to_owned()),
    ]
}

/// The config of `base_vars` with `extra` on top (same name: `extra` wins).
pub fn config(extra: &[(&str, &str)]) -> Config {
    let mut map: HashMap<String, OsString> = base_vars()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), OsString::from(v)))
        .collect();
    for (k, v) in extra {
        map.insert((*k).to_owned(), OsString::from(*v));
    }
    Config::from_lookup(|name| map.get(name).cloned()).expect("the test config must be valid")
}

/// A pool that connects on first use (never, for the tests that do not touch the database).
pub fn lazy_pool(config: &Config) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy_with(config.database.clone())
}
