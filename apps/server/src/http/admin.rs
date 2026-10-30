//! `RecentAdmin` (ADR-0008 D9, 008 RF-C7, plan-c decision 9): the extractor of the admin routes
//! of TA-13. It lets through a session whose user is `admin` **now** and authenticated in the
//! last 60 minutes; anything else is 403 `auth.reauth_required`, a user who is not an admin too
//! (literal as RF-C7). No session is 401 and a database error is 503, never "no session". Every
//! rejection says `Cache-Control: private, no-store`. This feature has no admin route: the
//! extractor is tested with a route that only exists in `tests/`.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::header;
use axum::http::request::Parts;
use axum::response::Response;
use protocol::{AuthErrorCode, Role};

use super::errors::error_response;
use super::session::{self, PRIVATE_NO_STORE};
use crate::app::AppState;
use crate::sessions::SessionInfo;

/// A recently authenticated admin.
#[derive(Debug, Clone, Copy)]
pub struct RecentAdmin(pub SessionInfo);

fn reauth_required() -> Response {
    let mut response = error_response(StatusCode::FORBIDDEN, AuthErrorCode::ReauthRequired);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}

impl FromRequestParts<AppState> for RecentAdmin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        match session::current(state, &parts.headers).await {
            Ok(Some(info)) if info.role == Role::Admin && info.recently_authenticated => {
                Ok(Self(info))
            }
            Ok(Some(_)) => Err(reauth_required()),
            Ok(None) => Err(session::unauthenticated(state)),
            Err(_) => Err(session::try_again()),
        }
    }
}
