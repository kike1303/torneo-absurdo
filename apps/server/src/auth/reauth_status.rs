//! `GET /api/auth/reauth-status` (009 RF18, plan decision 8): what `/account` needs to explain
//! a confirmation before the user taps, computed by the server so the web never repeats the
//! rule. A read: every action evaluates the rule again in its own transaction. No dates, no ids
//! and not the provider of the authentication.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use protocol::{Provider, ReauthStatus, UnlinkStatus};

use super::unlink::unlink_allowed;
use crate::app::AppState;
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::sessions::{self, AuthView};

pub async fn handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let info = match session::current(&state, &headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return session::unauthenticated(&state),
        Err(_) => return session::try_again(),
    };
    let view = match sessions::auth_view(&state.db, info.id).await {
        Ok(Some(view)) => view,
        // Closed between the two reads.
        Ok(None) => return session::unauthenticated(&state),
        Err(_) => return session::try_again(),
    };
    let mut response = Json(status_of(&view, &state.auth.providers)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}

/// The status of `view`, with `enabled` in the order of the login buttons. The ids never leave
/// this function.
pub fn status_of(view: &AuthView, enabled: &[Provider]) -> ReauthStatus {
    let unlink = view
        .linked
        .iter()
        .map(|&(provider, id)| UnlinkStatus {
            provider,
            ready: unlink_allowed(view.step_up, view.account, id),
            // The enabled ones a reauthentication could use: P itself and those linked before.
            reauth_with: enabled
                .iter()
                .copied()
                .filter(|wanted| {
                    view.linked
                        .iter()
                        .any(|&(other, other_id)| other == *wanted && other_id <= id)
                })
                .collect(),
        })
        .collect();
    ReauthStatus {
        link_ready: view.step_up,
        unlink,
    }
}
