//! The retention job (ADR-0008 D8, 008 RF-B16): one pass at startup and then every 24 hours.
//! Each pass, in its own transaction, deletes the `auth_events` older than 30 days (the floor
//! of D8 until the seasons feature), the expired `oauth_flows` and the expired sessions; these
//! through `sessions::revoke`, so their sockets close too. Logs only a failure, by code.

use std::time::Duration;

use sqlx::PgPool;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::sessions::{self, RevokeFilter};

pub const RETENTION_INTERVAL: Duration = Duration::from_secs(24 * 3600);

pub async fn retention_pass(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM auth_events WHERE created_at < now() - interval '30 days'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM oauth_flows WHERE expires_at < now()")
        .execute(&mut *tx)
        .await?;
    sessions::revoke(&mut tx, RevokeFilter::Expired).await?;
    tx.commit().await
}

/// A pass now, then every [`RETENTION_INTERVAL`], until `stop` is cancelled.
pub async fn run(pool: PgPool, stop: CancellationToken) {
    let mut interval = tokio::time::interval(RETENTION_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            // The first tick is immediate: the pass at startup.
            _ = interval.tick() => {
                if let Err(error) = retention_pass(&pool).await {
                    tracing::warn!(
                        event = "jobs.retention_failed",
                        error_kind = crate::db::error_kind(&error)
                    );
                }
            }
        }
    }
}
