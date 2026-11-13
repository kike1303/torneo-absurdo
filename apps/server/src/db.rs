//! Database startup and the health probe (RF-B9, RF-B10; plan-b decisions 8 and 13).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tokio::time::Instant;

use crate::app::StartupError;

/// Size of the pool (RF-B9).
pub const MAX_CONNECTIONS: u32 = 8;
/// How long an acquire waits for a connection before failing.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(1);

/// Retry policy of [`connect_with_retry`].
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub interval: Duration,
    pub deadline: Duration,
}

impl Default for RetryConfig {
    /// Every 2 s for up to 60 s (ADR-0006 D3 and D10).
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(2),
            deadline: Duration::from_secs(60),
        }
    }
}

pub fn pool_options() -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
}

/// The kind of a database error, as a code from a closed set. Never the error's `Display`,
/// which may contain the host, the user or the connection options.
pub fn error_kind(error: &sqlx::Error) -> &'static str {
    // The fixed SQLSTATEs of the ledger triggers (008 RF-B2): codes, never Postgres' text.
    if let Some(code) = error.as_database_error().and_then(|e| e.code()) {
        match code.as_ref() {
            "TA001" => return "ledger_immutable",
            "TA002" => return "ledger_unbalanced",
            "TA003" => return "ledger_closed",
            // `users_privileged_guard` (008 plan-c decision 10).
            "TA004" => return "privileged_write",
            // `oauth_accounts_identity_guard` (009 plan decision 1).
            "TA005" => return "oauth_identity_immutable",
            // `fights_guard` and `fight_secrets_guard` (010 RF4).
            "TA006" => return "fight_guard",
            // `bets_open_guard` (012 RF4).
            "TA007" => return "bet_window_closed",
            // `ledger_posting_rules` (012 RF2).
            "TA008" => return "ledger_posting_rule",
            // `seasons_guard` and `season_stats_guard` (020).
            "TA009" => return "season_guard",
            // `admin_audit_season_review_guard` (020, phase B).
            "TA010" => return "season_review_guard",
            // `botarga_draws_guard` (021, phase 3).
            "TA012" => return "botarga_draw_guard",
            // `SET LOCAL lock_timeout` of the fight transitions (010 RF8).
            "55P03" => return "lock_timeout",
            // A row that a foreign key needs was deleted meanwhile: a login whose identity an
            // unlink removed (009 plan decision 6).
            "23503" => return "foreign_key",
            _ => {}
        }
    }
    match error {
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        // A row that does not decode, not a database that fails (010 `store::BadFightRow`).
        sqlx::Error::Decode(_) | sqlx::Error::ColumnDecode { .. } => "row_invalid",
        sqlx::Error::PoolTimedOut => "timeout",
        sqlx::Error::PoolClosed => "pool",
        _ => "other",
    }
}

/// Whether `error` is a unique violation of the constraint `name` (by its fixed name, never by
/// the text of the error, which carries the key).
pub fn is_unique_violation(error: &sqlx::Error, name: &str) -> bool {
    error.as_database_error().is_some_and(|e| {
        e.kind() == sqlx::error::ErrorKind::UniqueViolation && e.constraint() == Some(name)
    })
}

/// Connects the pool, retrying every `retry.interval` until `retry.deadline` has passed.
/// Each failure logs `db.unavailable` with the error kind only.
pub async fn connect_with_retry(
    options: PgConnectOptions,
    retry: RetryConfig,
) -> Result<PgPool, StartupError> {
    retry_every(retry, || async {
        pool_options()
            .connect_with(options.clone())
            .await
            .map_err(|error| error_kind(&error))
    })
    .await
}

/// Runs `attempt` until it succeeds, starting one every `retry.interval` (measured from the
/// start of the previous attempt), up to `retry.deadline`. Each failure logs its kind.
pub async fn retry_every<T, F, Fut>(retry: RetryConfig, mut attempt: F) -> Result<T, StartupError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, &'static str>>,
{
    let deadline = Instant::now() + retry.deadline;
    loop {
        let started = Instant::now();
        match tokio::time::timeout_at(deadline, attempt()).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(kind)) => {
                tracing::warn!(event = "db.unavailable", error_kind = kind);
            }
            Err(_) => {
                tracing::warn!(event = "db.unavailable", error_kind = "timeout");
                return Err(StartupError::DbUnavailable);
            }
        }
        let next = started + retry.interval;
        if next >= deadline {
            tokio::time::sleep_until(deadline).await;
            return Err(StartupError::DbUnavailable);
        }
        tokio::time::sleep_until(next).await;
    }
}

/// Runs the embedded migrations (`apps/server/migrations`). A version this image does not know
/// is what a rollback leaves behind, so it does not stop the start (ADR-0006 D10, TA-43): it is
/// logged at every start. Another checksum in a known version, or a row marked as failed, still
/// stop it.
pub async fn migrate(pool: &PgPool) -> Result<(), StartupError> {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    let migrated = match migrator.run(pool).await {
        Ok(()) => warn_if_ahead(pool, &migrator)
            .await
            .map_err(|error| error_kind(&error)),
        Err(error) => Err(migrate_error_kind(&error)),
    };
    if let Err(kind) = migrated {
        tracing::error!(event = "db.migrate_failed", error_kind = kind);
        return Err(StartupError::Migrate);
    }
    Ok(())
}

/// The kind of a migration error, as a code from a closed set (TA-47). Never its `Display`,
/// which carries the versions and, for a failed migration, Postgres' text.
pub fn migrate_error_kind(error: &MigrateError) -> &'static str {
    match error {
        // The database itself failed (down, closed pool, …): the kind of the inner error.
        MigrateError::Execute(error) => error_kind(error),
        // The SQL of a migration failed while it was applied.
        MigrateError::ExecuteMigration(..) => "migration_failed",
        // A version this image knows was applied with other content.
        MigrateError::VersionMismatch(_) => "checksum_mismatch",
        // A row marked as failed in `_sqlx_migrations`.
        MigrateError::Dirty(_) => "migration_dirty",
        MigrateError::VersionMissing(_) => "version_missing",
        MigrateError::Source(_) => "migration_source",
        _ => "other",
    }
}

/// Logs `db.migrations_ahead` if the database has versions that `migrator` does not know.
async fn warn_if_ahead(pool: &PgPool, migrator: &Migrator) -> Result<(), sqlx::Error> {
    let known: Vec<i64> = migrator.iter().map(|m| m.version).collect();
    let ahead: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version <> ALL($1))",
    )
    .bind(&known)
    .fetch_one(pool)
    .await?;
    if ahead {
        tracing::warn!(event = "db.migrations_ahead");
    }
    Ok(())
}

/// Boxed future returned by [`DbProbe::ping`].
pub type PingFuture<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

/// The health query, behind a trait so tests can count or slow it down.
pub trait DbProbe: Send + Sync {
    /// Whether the database answered `SELECT 1`. The caller applies the timeout.
    fn ping(&self) -> PingFuture<'_>;
}

/// [`DbProbe`] over the real pool.
#[derive(Debug, Clone)]
pub struct PgProbe {
    pool: PgPool,
}

impl PgProbe {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DbProbe for PgProbe {
    fn ping(&self) -> PingFuture<'_> {
        Box::pin(async move { sqlx::query("SELECT 1").execute(&self.pool).await.is_ok() })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    /// Attempts start every `interval` from the start of the previous one, not `interval`
    /// after it fails (each attempt here takes 1.5 s, like a slow refused connection).
    #[tokio::test(start_paused = true)]
    async fn attempts_start_every_interval() {
        let origin = Instant::now();
        let starts = Arc::new(Mutex::new(Vec::new()));
        let recorded = starts.clone();
        let result: Result<(), StartupError> = retry_every(
            RetryConfig {
                interval: Duration::from_secs(2),
                deadline: Duration::from_secs(9),
            },
            || {
                let recorded = recorded.clone();
                async move {
                    recorded.lock().unwrap().push(origin.elapsed().as_millis());
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    Err("io")
                }
            },
        )
        .await;
        assert_eq!(result, Err(StartupError::DbUnavailable));
        assert_eq!(origin.elapsed(), Duration::from_secs(9));
        assert_eq!(*starts.lock().unwrap(), [0, 2000, 4000, 6000, 8000]);
    }

    #[test]
    fn migrate_errors_have_a_kind_each() {
        let cases = [
            (MigrateError::Execute(sqlx::Error::PoolClosed), "pool"),
            (MigrateError::Execute(sqlx::Error::PoolTimedOut), "timeout"),
            (
                MigrateError::ExecuteMigration(sqlx::Error::Protocol("x".into()), 1),
                "migration_failed",
            ),
            (MigrateError::VersionMismatch(1), "checksum_mismatch"),
            (MigrateError::Dirty(1), "migration_dirty"),
            (MigrateError::VersionMissing(1), "version_missing"),
            (MigrateError::Source("x".into()), "migration_source"),
            (MigrateError::VersionNotPresent(1), "other"),
        ];
        for (error, kind) in cases {
            assert_eq!(migrate_error_kind(&error), kind, "{error:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_success_returns_at_once() {
        let mut calls = 0;
        let result = retry_every(RetryConfig::default(), || {
            calls += 1;
            let outcome = if calls < 3 { Err("io") } else { Ok(calls) };
            async move { outcome }
        })
        .await;
        assert_eq!(result, Ok(3));
    }
}

/// Every SQL of the server binds its values (008 plan-b decision 7): sqlx's assert-safe
/// wrapper, the only way to run a formatted string, never appears in the server sources.
#[cfg(test)]
mod sql_guard {
    #[test]
    fn no_formatted_sql_in_the_server() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let needle = ["Assert", "SqlSafe"].concat();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if std::fs::read_to_string(&path).unwrap().contains(&needle) {
                    panic!("{} formats SQL", path.display());
                }
            }
        }
    }
}
