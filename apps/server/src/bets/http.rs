//! What the HTTP routes of the bets share: the quotas (IP before the session, user after, like
//! `profile::update`), the session, and the answers, always `private, no-store`. A session that
//! cannot be read gives the domain's 503, so the web has one way out (plan-b D8).

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use protocol::{ApiErrorCode, ErrorCode};
use uuid::Uuid;

use crate::app::AppState;
use crate::http::errors::{error_response, too_many_requests};
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::net::{ClientContext, IpKey};
use crate::rate::KeyedLimiter;
use crate::sessions::SessionInfo;

/// `private, no-store` and `Vary: Cookie` (012 RF32, RF33).
pub fn private(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    headers.insert(header::VARY, HeaderValue::from_static("Cookie"));
    response
}

pub fn refused(status: StatusCode, code: impl Into<ErrorCode>) -> Response {
    private(error_response(status, code))
}

/// 503 with the domain's code.
pub fn unavailable(code: impl Into<ErrorCode>) -> Response {
    refused(StatusCode::SERVICE_UNAVAILABLE, code)
}

/// The two buckets of one row of the quotas.
pub struct Quota<'a> {
    pub ip: &'a KeyedLimiter,
    pub user: &'a KeyedLimiter<Uuid>,
}

/// The client's `IpKey`; without a context (tests that build a bare router), the unspecified one.
pub fn ip_key(client: Option<&Extension<ClientContext>>) -> IpKey {
    client.map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.0.key)
}

/// The quotas and the session, in that order: IP, session, user.
pub async fn authorize(
    state: &AppState,
    quota: &Quota<'_>,
    key: &IpKey,
    headers: &HeaderMap,
    unavailable_code: impl Into<ErrorCode> + Copy,
) -> Result<SessionInfo, Box<Response>> {
    if let Some(secs) = quota.ip.check(key).retry_after_secs() {
        return Err(Box::new(private(too_many_requests(
            ApiErrorCode::RateLimited,
            secs,
        ))));
    }
    let info = match session::current(state, headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return Err(Box::new(session::unauthenticated(state))),
        Err(_) => return Err(Box::new(unavailable(unavailable_code))),
    };
    if let Some(secs) = quota.user.check(&info.user_id).retry_after_secs() {
        return Err(Box::new(private(too_many_requests(
            ApiErrorCode::RateLimited,
            secs,
        ))));
    }
    Ok(info)
}

/// `SET LOCAL lock_timeout = '1s'` (010 RF29, 012 RF25), and the user locked with `FOR UPDATE` only
/// if not banned. `false`: there is no such user, or it is banned: 401, as `banned_writes` expects.
pub async fn lock_user(tx: &mut sqlx::PgConnection, user_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query("SET LOCAL lock_timeout = '1s'")
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query("SELECT 1 FROM users WHERE id = $1 AND banned_at IS NULL FOR UPDATE")
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
    Ok(row.is_some())
}
