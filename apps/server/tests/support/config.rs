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

/// The variables every in-process test starts from.
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
