//! Reading the session of a request (008 RF-B8). Every response that reads it says
//! `Cache-Control: private, no-store` (ADR-0008 D5); an invalid session is 401
//! `auth.unauthenticated` with the cookie expired.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use protocol::AuthErrorCode;

use super::errors::error_response;
use crate::app::AppState;
use crate::sessions::{self, SessionInfo, TokenHash};

pub const PRIVATE_NO_STORE: HeaderValue = HeaderValue::from_static("private, no-store");

/// The hash of the session cookie of the active policy, valid or not.
pub fn token_hash(state: &AppState, headers: &HeaderMap) -> Option<TokenHash> {
    state.auth.cookies.read_session(headers).map(TokenHash::of)
}

/// The valid session of the request: `Ok(None)` without one, `Err` when the database could not
/// say. The two are never the same: a caller that treated an error as "no session" could log a
/// user out on a hiccup, or a logout-all could revoke only this session and claim success
/// (review of #54).
pub async fn current(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let Some(hash) = token_hash(state, headers) else {
        return Ok(None);
    };
    sessions::validate(&state.db, &hash).await
}

/// 503 `auth.try_again` with `private, no-store` and no cookie change: the session may be fine.
pub fn try_again() -> Response {
    let mut response = error_response(StatusCode::SERVICE_UNAVAILABLE, AuthErrorCode::TryAgain);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}

/// 401 `auth.unauthenticated`, with the session cookie expired and `private, no-store`.
pub fn unauthenticated(state: &AppState) -> Response {
    let mut response = error_response(StatusCode::UNAUTHORIZED, AuthErrorCode::Unauthenticated);
    let headers = response.headers_mut();
    headers.insert(header::SET_COOKIE, state.auth.cookies.expire_session());
    headers.insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}
