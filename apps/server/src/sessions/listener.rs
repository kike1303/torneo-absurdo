//! The revocation listener (008 RF-B10): `LISTEN session_revoked` on a connection of its own (a
//! pool of one, so the app keeps its 8), and every notified id closes its sockets at once.
//!
//! `try_recv()`, not `recv()`: `recv()` reconnects silently and loses what was notified in the
//! gap. On a lost connection the listener revalidates every socket with a session, reconnects
//! (`LISTEN` again) and revalidates once more, so nothing revoked in the gap stays open. Only
//! `sessions.listener_lost` is logged, with no field.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgListener, PgPoolOptions};
use tokio_util::sync::CancellationToken;

use super::{REVOKED_CHANNEL, SessionId, still_valid};
use crate::ws::registry::Registry;

/// `application_name` of the listener connection (the tests find it in `pg_stat_activity`).
pub const APPLICATION_NAME: &str = "torneo-listener";
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Closes the sockets of every registered session that is no longer valid.
pub async fn revalidate_all(pool: &PgPool, registry: &Registry) {
    let ids = registry.sessions();
    if ids.is_empty() {
        return;
    }
    // On a database error keep them: the next revalidation (or the periodic one) decides.
    if let Ok(valid) = still_valid(pool, &ids).await {
        for id in ids.into_iter().filter(|id| !valid.contains(id)) {
            registry.close(id);
        }
    }
}

pub async fn run(pool: PgPool, registry: Arc<Registry>, stop: CancellationToken) {
    let options = (*pool.connect_options())
        .clone()
        .application_name(APPLICATION_NAME);
    let own = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy_with(options);
    let mut backoff = BACKOFF_MIN;
    loop {
        let connected = async {
            let mut listener = PgListener::connect_with(&own).await?;
            listener.listen(REVOKED_CHANNEL).await?;
            Ok::<_, sqlx::Error>(listener)
        };
        let listener = tokio::select! {
            () = stop.cancelled() => return,
            connected = connected => connected,
        };
        let Ok(mut listener) = listener else {
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(backoff) => {}
            }
            backoff = (backoff * 2).min(BACKOFF_MAX);
            continue;
        };
        backoff = BACKOFF_MIN;
        // LISTEN is active: anything revoked from now on arrives, and anything revoked
        // before (while disconnected) is caught here.
        revalidate_all(&pool, &registry).await;
        loop {
            let received = tokio::select! {
                () = stop.cancelled() => return,
                received = listener.try_recv() => received,
            };
            match received {
                Ok(Some(notification)) => {
                    if let Some(id) = SessionId::parse(notification.payload()) {
                        registry.close(id);
                    }
                }
                // The connection is gone: what was notified in the gap is lost.
                Ok(None) | Err(_) => {
                    tracing::warn!(event = "sessions.listener_lost");
                    revalidate_all(&pool, &registry).await;
                    break;
                }
            }
        }
    }
}
