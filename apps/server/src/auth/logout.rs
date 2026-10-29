//! `POST /api/auth/logout` and `POST /api/auth/logout-all` (ADR-0008 D10, 008 RF-B11). Both
//! revoke (one session, or every session of the user), expire the cookie and answer 303 to
//! `/`, with or without a session: logging out is idempotent. "Other mutations" quota of D7.

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::ApiErrorCode;

use crate::app::AppState;
use crate::http::errors::too_many_requests;
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::net::{ClientContext, IpKey};
use crate::sessions::{self, RevokeFilter};

#[derive(Clone, Copy)]
enum Scope {
    This,
    All,
}

pub async fn logout(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    run(&state, client, &headers, Scope::This).await
}

pub async fn logout_all(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    run(&state, client, &headers, Scope::All).await
}

async fn run(
    state: &AppState,
    client: Option<Extension<ClientContext>>,
    headers: &HeaderMap,
    scope: Scope,
) -> Response {
    let key = client.map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.0.key);
    if let Some(secs) = state.rate.mutation_ip.check(&key).retry_after_secs() {
        return too_many_requests(ApiErrorCode::RateLimited, secs);
    }
    let hash = session::token_hash(state, headers);
    let current = session::current(state, headers).await;
    if let Some(info) = current
        && let Some(secs) = state
            .rate
            .mutation_user
            .check(&info.user_id)
            .retry_after_secs()
    {
        return too_many_requests(ApiErrorCode::RateLimited, secs);
    }
    let filter = match (scope, current, hash) {
        (Scope::All, Some(info), _) => Some(RevokeFilter::AllOfUser(info.user_id)),
        (_, _, Some(hash)) => Some(RevokeFilter::Token(hash)),
        _ => None,
    };
    if let Some(filter) = filter {
        let revoked = async {
            let mut tx = state.db.begin().await?;
            sessions::revoke(&mut tx, filter).await?;
            tx.commit().await
        };
        if revoked.await.is_err() {
            // The session may still be alive: say so instead of pretending.
            return crate::http::errors::error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                protocol::AuthErrorCode::TryAgain,
            );
        }
    }
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, HeaderValue::from_static("/")),
            (header::SET_COOKIE, state.auth.cookies.expire_session()),
            (header::CACHE_CONTROL, PRIVATE_NO_STORE),
        ],
    )
        .into_response()
}
