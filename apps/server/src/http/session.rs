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

/// The valid session of the request, if any. A database error counts as no session (fail
/// closed).
pub async fn current(state: &AppState, headers: &HeaderMap) -> Option<SessionInfo> {
    let hash = token_hash(state, headers)?;
    sessions::validate(&state.db, &hash).await.ok().flatten()
}

/// 401 `auth.unauthenticated`, with the session cookie expired and `private, no-store`.
pub fn unauthenticated(state: &AppState) -> Response {
    let mut response = error_response(StatusCode::UNAUTHORIZED, AuthErrorCode::Unauthenticated);
    let headers = response.headers_mut();
    headers.insert(header::SET_COOKIE, state.auth.cookies.expire_session());
    headers.insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}
