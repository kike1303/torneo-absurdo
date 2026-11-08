//! `GET /api/wallet` (012 RF32): the facts of the account, from the same statement the `POST` routes
//! decide with. Read-only: one statement, so one snapshot and one `now()`.

use axum::Extension;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json, Response};
use protocol::WalletErrorCode;

use super::facts;
use super::http::{Quota, authorize, ip_key, private, unavailable};
use crate::app::AppState;
use crate::http::session;
use crate::net::ClientContext;

pub async fn handler(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.wallet_read_ip,
        user: &state.rate.wallet_read_user,
    };
    let info = match authorize(&state, &quota, &key, &headers, WalletErrorCode::Unavailable).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        facts::read(&mut conn, info.user_id).await
    }
    .await;
    match read {
        Ok(Some(facts)) => match facts.wallet(state.bets) {
            Some(wallet) => private(Json(wallet).into_response()),
            None => unavailable(WalletErrorCode::Unavailable),
        },
        Ok(None) => session::unauthenticated(&state),
        Err(error) => {
            tracing::warn!(
                event = "bets.wallet_failed",
                error_kind = crate::db::error_kind(&error)
            );
            unavailable(WalletErrorCode::Unavailable)
        }
    }
}
