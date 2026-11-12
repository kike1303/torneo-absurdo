//! The retention job (ADR-0008 D8, 008 RF-B16): one pass at startup and then every 24 hours.
//! Each pass, in its own transaction, deletes the `auth_events` older than 38 days (D8:
//! `max(30 days, season + 7 days)`, with a season of up to 31 days, feature 020), the expired
//! `oauth_flows` and the expired sessions; these through `sessions::revoke`, so their sockets close
//! too. Then it sums in their season the settled fights without a mark (`seasons::reconcile`). Logs
//! only a failure, by code.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::Notify;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::sessions::{self, RevokeFilter};

pub const RETENTION_INTERVAL: Duration = Duration::from_secs(24 * 3600);

pub async fn retention_pass(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM auth_events WHERE created_at < now() - interval '38 days'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM oauth_flows WHERE expires_at < now()")
        .execute(&mut *tx)
        .await?;
    sessions::revoke(&mut tx, RevokeFilter::Expired).await?;
    tx.commit().await
}

/// Told once the pass at startup has ended (test code only, never an environment variable): the
/// tests that sum seasons by hand wait for it, so its reconciliation never marks their fights first
/// (TA-81). `None` in production.
#[derive(Clone, Debug, Default)]
pub struct StartupPassSignal(pub Option<Arc<Notify>>);

/// A pass now, then every [`RETENTION_INTERVAL`], until `stop` is cancelled. `started` is told when
/// the first pass ends.
pub async fn run(pool: PgPool, stop: CancellationToken, started: StartupPassSignal) {
    let mut interval = tokio::time::interval(RETENTION_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut started = started.0;
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
                crate::seasons::reconcile_logged(&pool).await;
                if let Some(started) = started.take() {
                    // A stored permit: a waiter that arrives later still sees it.
                    started.notify_one();
                }
            }
        }
    }
}
