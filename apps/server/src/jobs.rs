//! The retention job (ADR-0008 D8, 008 RF-B16): one pass at startup and then every 24 hours.
//! Each pass, in its own transaction, deletes the `auth_events` older than 38 days (D8:
//! `max(30 days, season + 7 days)`, with a season of up to 31 days, feature 020), the expired
//! `oauth_flows` and the expired sessions; these through `sessions::revoke`, so their sockets close
//! too. Then it sums in their season the settled fights without a mark (`seasons::reconcile`). Logs
//! only a failure, by code.
//!
//! The PvP seasons that are due close in their own pass, at startup and every hour
//! (`pvp::season::close_due`, 021 phase 4): a month ends at any hour, and its soft reset should not
//! wait a day.

use std::time::Duration;

use sqlx::PgPool;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::sessions::{self, RevokeFilter};

pub const RETENTION_INTERVAL: Duration = Duration::from_secs(24 * 3600);
pub const PVP_CLOSE_INTERVAL: Duration = Duration::from_secs(3600);

/// Whether `app::serve_listener` starts [`run`]. Always [`Jobs::Run`] in production; the in-process
/// tests that sow settled fights by hand and then tally or reconcile them turn it off, because the
/// pass at startup would otherwise race them (TA-82). Never an environment variable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Jobs {
    #[default]
    Run,
    Off,
}

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

/// A pass now, then every [`RETENTION_INTERVAL`], until `stop` is cancelled.
pub async fn run(pool: PgPool, stop: CancellationToken) {
    let mut interval = tokio::time::interval(RETENTION_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut pvp = tokio::time::interval(PVP_CLOSE_INTERVAL);
    pvp.set_missed_tick_behavior(MissedTickBehavior::Delay);
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
            }
            // Its first tick is immediate too.
            _ = pvp.tick() => crate::pvp::season::close_due_logged(&pool).await,
        }
    }
}
