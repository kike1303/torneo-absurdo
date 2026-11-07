//! `GET /api/fights/void-stats` (012 RF23): the public counter of the voided fights. A read with no
//! session, in a file of its own, over a partial index. It detects: it prevents nothing.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use protocol::{FightErrorCode, VoidStats};
use sqlx::{PgPool, Row};

use super::store;
use crate::app::AppState;
use crate::db::ACQUIRE_TIMEOUT;
use crate::http::errors::error_response;

/// `voided` includes `secret_lost`. It does not wait for a lock longer than `lock_timeout_ms`.
async fn read(pool: &PgPool, lock_timeout_ms: u64) -> Result<(i64, i64), sqlx::Error> {
    let mut tx = pool.begin().await?;
    store::set_lock_timeout(&mut tx, lock_timeout_ms).await?;
    let row = sqlx::query(
        "SELECT count(*), count(*) FILTER (WHERE void_reason = 'secret_lost') \
         FROM fights WHERE void_reason IS NOT NULL",
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((row.try_get(0)?, row.try_get(1)?))
}

/// The route sets `no-store`, also on the 429 of its limiter.
pub async fn handler(State(state): State<AppState>) -> Response {
    let lock_timeout_ms = u64::try_from(ACQUIRE_TIMEOUT.as_millis()).unwrap_or(u64::MAX);
    let read = tokio::time::timeout(
        ACQUIRE_TIMEOUT.saturating_mul(3),
        read(&state.db, lock_timeout_ms),
    );
    let counts = read.await.unwrap_or(Err(sqlx::Error::PoolTimedOut));
    let stats = counts.ok().and_then(|(voided, secret_lost)| {
        Some(VoidStats {
            voided: u32::try_from(voided).ok()?,
            secret_lost: u32::try_from(secret_lost).ok()?,
        })
    });
    match stats {
        Some(stats) => Json(stats).into_response(),
        None => error_response(StatusCode::SERVICE_UNAVAILABLE, FightErrorCode::Unavailable),
    }
}
