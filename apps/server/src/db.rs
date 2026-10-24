//! Database startup and the health probe (RF-B9, RF-B10; plan-b decisions 8 and 13).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use sqlx::PgPool;
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
    match error {
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::PoolTimedOut => "timeout",
        sqlx::Error::PoolClosed => "pool",
        _ => "other",
    }
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

/// Runs the embedded migrations (`apps/server/migrations`).
pub async fn migrate(pool: &PgPool) -> Result<(), StartupError> {
    sqlx::migrate!("./migrations").run(pool).await.map_err(|_| {
        tracing::error!(event = "db.migrate_failed");
        StartupError::Migrate
    })
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
