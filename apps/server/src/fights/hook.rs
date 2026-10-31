//! The settlement hook (RF31): where TA-27 pays and refunds, inside the transaction of the reveal.
//! A failure reverts the reveal too. It may only fail for infrastructure reasons: nothing a user can
//! do may make it fail, or a bet or a ban would stop the live fights.

use std::future::Future;
use std::pin::Pin;

use protocol::FightFinal;
use sqlx::PgConnection;

/// What a settlement needs to know of its fight. `closes_at` gives its season (RF30).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlingFight {
    pub id: i32,
    pub final_result: FightFinal,
    pub closes_at: i64,
}

/// A failed settlement, by code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the settlement failed")]
pub struct SettlementError {
    pub kind: &'static str,
}

impl From<sqlx::Error> for SettlementError {
    fn from(error: sqlx::Error) -> Self {
        Self {
            kind: crate::db::error_kind(&error),
        }
    }
}

/// `futures-util` is only a development dependency (amendment M1).
pub type SettleFuture<'a> = Pin<Box<dyn Future<Output = Result<(), SettlementError>> + Send + 'a>>;

pub trait SettlementHook: Send + Sync {
    fn settle<'a>(&'a self, tx: &'a mut PgConnection, fight: &'a SettlingFight)
    -> SettleFuture<'a>;
}

/// The hook until TA-27: no bets exist.
pub struct NoBets;

impl SettlementHook for NoBets {
    fn settle<'a>(
        &'a self,
        _tx: &'a mut PgConnection,
        _fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        Box::pin(std::future::ready(Ok(())))
    }
}
