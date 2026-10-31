//! What TA-27 can assume, proved here without any bet (RF29, RF32, RF33).

use protocol::FightState;
use sqlx::{PgConnection, PgPool};

use super::{FightsShared, store};

/// The window was open, and it stays so until the caller's transaction ends: the close waits for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenWindow {
    pub closes_at_ms: i64,
}

/// No bet can be accepted, for whatever reason. It says nothing of why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the betting window is closed")]
pub struct WindowClosed;

impl FightsShared {
    /// The serializable close (RF29). `WindowClosed` during a halt, without the lock of RF10 and
    /// until the first evaluation ends, without touching the database. Otherwise a `FOR SHARE` that
    /// filters by the hour: past `closes_at` it finds no row and takes no lock and waits for none.
    /// It must be `FOR SHARE`, because the `FOR KEY SHARE` of a foreign key does not conflict with
    /// the `UPDATE` of the close. A database error is `WindowClosed` too: the transaction is lost.
    ///
    /// # Errors
    /// [`WindowClosed`].
    pub async fn lock_open_for_bet(
        &self,
        tx: &mut PgConnection,
        id: i32,
        now_ms: i64,
    ) -> Result<OpenWindow, WindowClosed> {
        if !self.accepts_bets() {
            return Err(WindowClosed);
        }
        match store::lock_for_bet(tx, id, now_ms).await {
            Ok(Some((FightState::BettingOpen, closes_at_ms))) => Ok(OpenWindow { closes_at_ms }),
            Ok(_) => Err(WindowClosed),
            Err(error) => {
                tracing::warn!(
                    event = "fights.db_unavailable",
                    error_kind = crate::db::error_kind(&error)
                );
                Err(WindowClosed)
            }
        }
    }
}

/// The unsettled fights with `closes_at` in `[from_ms, to_ms)`: the query of `farm-report` (RF32).
/// A voided fight counts as settled.
///
/// # Errors
/// The database error.
pub async fn unsettled_in(
    pool: &PgPool,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<i32>, sqlx::Error> {
    store::unsettled_ids_in(pool, from_ms, to_ms).await
}
