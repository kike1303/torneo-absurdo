//! `POST /api/bonus/claim` and `POST /api/bailout/claim` (012 RF29 to RF31). Each one: its quota, the
//! session, one transaction with `lock_timeout` and the user locked if not banned, the claim by its
//! unique index, its `auth_events`, and the wallet as the answer. [`claim_bonus`] and
//! [`claim_bailout`] take the transaction, so tests 7 and 8 call them without the limiter.

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use protocol::{
    BailoutErrorCode, BailoutState, BonusErrorCode, ErrorCode, Wallet, WalletErrorCode,
};
use sqlx::PgConnection;
use uuid::Uuid;

use super::audit::{self, Event};
use super::facts;
use super::http::{Quota, authorize, ip_key, lock_user, private, refused, unavailable};
use crate::app::AppState;
use crate::http::session;
use crate::ledger;
use crate::net::ClientContext;

/// What a claim ends in. `Gone`: no user, or a banned one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    Done(Wallet),
    Refused(ErrorCode),
    Gone,
}

/// The bonus of today (RF29), inside `tx`. The unique index decides, never a prior check.
///
/// # Errors
/// The database error.
pub async fn claim_bonus(
    tx: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
    ip_hash: &[u8; 32],
) -> Result<Claim, sqlx::Error> {
    if !lock_user(&mut *tx, user_id).await? {
        return Ok(Claim::Gone);
    }
    let Some(entry) = ledger::grant(&mut *tx, user_id, state.bets.grant).await? else {
        return Ok(Claim::Refused(BonusErrorCode::AlreadyClaimed.into()));
    };
    audit::record(&mut *tx, user_id, Event::Grant { entry }, ip_hash).await?;
    wallet_of(tx, state, user_id).await
}

/// The bailout of today (RF30), inside `tx`, in its order: today's grant first, then
/// `balance + open stakes < m` (from one statement), then the unique index.
///
/// # Errors
/// The database error.
pub async fn claim_bailout(
    tx: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
    ip_hash: &[u8; 32],
) -> Result<Claim, sqlx::Error> {
    if !lock_user(&mut *tx, user_id).await? {
        return Ok(Claim::Gone);
    }
    let Some(facts) = facts::read(&mut *tx, user_id).await? else {
        return Ok(Claim::Gone);
    };
    let refusal = match facts.bailout(state.bets) {
        BailoutState::GrantFirst => Some(BailoutErrorCode::GrantAvailable),
        BailoutState::NotEligible => Some(BailoutErrorCode::NotEligible),
        BailoutState::AlreadyClaimed => Some(BailoutErrorCode::AlreadyClaimed),
        BailoutState::Available => None,
    };
    if let Some(code) = refusal {
        return Ok(Claim::Refused(code.into()));
    }
    let Some(entry) = ledger::bailout(&mut *tx, user_id, state.bets.bailout()).await? else {
        return Ok(Claim::Refused(BailoutErrorCode::AlreadyClaimed.into()));
    };
    audit::record(&mut *tx, user_id, Event::Bailout { entry }, ip_hash).await?;
    wallet_of(tx, state, user_id).await
}

/// The wallet after the change, in the same transaction.
async fn wallet_of(
    tx: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
) -> Result<Claim, sqlx::Error> {
    let wallet = facts::read(&mut *tx, user_id)
        .await?
        .and_then(|facts| facts.wallet(state.bets));
    // A balance a JavaScript number would lose: refused with the domain's 503, nothing written.
    Ok(wallet.map_or(
        Claim::Refused(WalletErrorCode::Unavailable.into()),
        Claim::Done,
    ))
}

#[derive(Clone, Copy)]
enum Which {
    Bonus,
    Bailout,
}

async fn handle(
    state: AppState,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    which: Which,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.claim_ip,
        user: &state.rate.claim_user,
    };
    let info = match authorize(&state, &quota, &key, &headers, WalletErrorCode::Unavailable).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let ip_hash = key.hash(&state.auth.ip_hash_key);
    let claimed = async {
        let mut tx = state.db.begin().await?;
        let claim = match which {
            Which::Bonus => claim_bonus(&mut tx, &state, info.user_id, &ip_hash).await?,
            Which::Bailout => claim_bailout(&mut tx, &state, info.user_id, &ip_hash).await?,
        };
        if matches!(claim, Claim::Done(_)) {
            tx.commit().await?;
        }
        Ok::<_, sqlx::Error>(claim)
    }
    .await;
    match claimed {
        Ok(Claim::Done(wallet)) => private(Json(wallet).into_response()),
        Ok(Claim::Refused(ErrorCode::Wallet(code))) => unavailable(code),
        Ok(Claim::Refused(code)) => refused(StatusCode::CONFLICT, code),
        Ok(Claim::Gone) => session::unauthenticated(&state),
        Err(error) => {
            tracing::warn!(
                event = "bets.claim_failed",
                error_kind = crate::db::error_kind(&error)
            );
            unavailable(WalletErrorCode::Unavailable)
        }
    }
}

pub async fn bonus(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    handle(state, client, headers, Which::Bonus).await
}

pub async fn bailout(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    handle(state, client, headers, Which::Bailout).await
}
