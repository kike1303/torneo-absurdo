//! `POST /api/auth/unlink/{provider}` (ADR-0008 D1, 008 RF-C3 and its amendment 2026-10-29).
//!
//! In one transaction: lock the user, delete that provider's account (404 if there is none),
//! count what is left among the **enabled** providers (409 `auth.last_provider` if nothing), write
//! `auth_events` and revoke every other session of the user. The session in use stays. It never
//! reads the body, like `logout`: no form, so nothing for `DefaultBodyLimit` to reject. "Other
//! mutations" quota of D7.

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{ApiErrorCode, AuthErrorCode, ErrorCode, Provider};
use sqlx::Row;

use crate::app::AppState;
use crate::http::errors::{error_response, too_many_requests};
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::net::{ClientContext, IpKey};
use crate::sessions::{self, RevokeFilter, SessionInfo};

enum Refused {
    NotLinked,
    LastProvider,
    Db,
}

impl From<sqlx::Error> for Refused {
    fn from(error: sqlx::Error) -> Self {
        tracing::warn!(
            event = "auth.unlink_failed",
            error_kind = crate::db::error_kind(&error)
        );
        Self::Db
    }
}

fn private(status: StatusCode, code: impl Into<ErrorCode>) -> Response {
    let mut response = error_response(status, code);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}

pub async fn handler(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let key = client.map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.0.key);
    if let Some(secs) = state.rate.mutation_ip.check(&key).retry_after_secs() {
        return too_many_requests(ApiErrorCode::RateLimited, secs);
    }
    // Any known provider, enabled or not: a provider the config turned off can be unlinked.
    let Some(provider) = Provider::parse(&segment) else {
        return private(StatusCode::NOT_FOUND, ApiErrorCode::NotFound);
    };
    let info = match session::current(&state, &headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return session::unauthenticated(&state),
        Err(_) => return session::try_again(),
    };
    if let Some(secs) = state
        .rate
        .mutation_user
        .check(&info.user_id)
        .retry_after_secs()
    {
        return too_many_requests(ApiErrorCode::RateLimited, secs);
    }
    let ip_hash = key.hash(&state.auth.ip_hash_key);
    match unlink(&state, &info, provider, &ip_hash).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, HeaderValue::from_static("/")),
                (header::CACHE_CONTROL, PRIVATE_NO_STORE),
            ],
        )
            .into_response(),
        Err(Refused::NotLinked) => private(StatusCode::NOT_FOUND, ApiErrorCode::NotFound),
        Err(Refused::LastProvider) => private(StatusCode::CONFLICT, AuthErrorCode::LastProvider),
        Err(Refused::Db) => session::try_again(),
    }
}

async fn unlink(
    state: &AppState,
    info: &SessionInfo,
    provider: Provider,
    ip_hash: &[u8; 32],
) -> Result<(), Refused> {
    let mut tx = state.db.begin().await?;
    // In series with another unlink and with a link of the same user.
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(info.user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Refused::NotLinked)?;
    sqlx::query(
        "DELETE FROM oauth_accounts WHERE user_id = $1 AND provider = $2 RETURNING provider",
    )
    .bind(info.user_id)
    .bind(provider.as_str())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Refused::NotLinked)?;
    // Only an enabled provider lets the user back in (amendment 2026-10-29 of RF-C3).
    let enabled: Vec<&str> = state.auth.providers.iter().map(|p| p.as_str()).collect();
    let left: i64 = sqlx::query(
        "SELECT count(*) FROM oauth_accounts WHERE user_id = $1 AND provider = ANY($2)",
    )
    .bind(info.user_id)
    .bind(&enabled)
    .fetch_one(&mut *tx)
    .await?
    .try_get(0)?;
    if left == 0 {
        // Dropping the transaction rolls the delete back.
        return Err(Refused::LastProvider);
    }
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, provider, ip_hash) VALUES ($1, 'unlink', $2, $3)",
    )
    .bind(info.user_id)
    .bind(provider.as_str())
    .bind(ip_hash.as_slice())
    .execute(&mut *tx)
    .await?;
    sessions::revoke(
        &mut tx,
        RevokeFilter::OthersOfUser {
            user: info.user_id,
            keep: info.id,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
