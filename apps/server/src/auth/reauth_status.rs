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
            // That is the rule of unlink for a confirmation just made with each of them.
            reauth_with: enabled
                .iter()
                .copied()
                .filter(|wanted| {
                    view.linked.iter().any(|&(other, other_id)| {
                        other == *wanted && unlink_allowed(true, Some(other_id), id)
                    })
                })
                .collect(),
        })
        .collect();
    ReauthStatus {
        link_ready: view.step_up,
        unlink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::OauthAccountId;

    /// RF18: the rule of `unlink`, the order of the login buttons, and no id ever comes out.
    #[test]
    fn the_status_says_only_what_the_page_needs() {
        let (google, discord) = (
            OauthAccountId::new(4_242_424_241),
            OauthAccountId::new(4_242_424_242),
        );
        let view = AuthView {
            step_up: true,
            account: Some(discord),
            linked: vec![(Provider::Google, google), (Provider::Discord, discord)],
        };
        let status = status_of(&view, &[Provider::Google, Provider::Discord]);
        assert!(status.link_ready);
        assert_eq!(status.unlink.len(), 2);
        assert!(!status.unlink[0].ready);
        assert_eq!(status.unlink[0].reauth_with, [Provider::Google]);
        assert!(status.unlink[1].ready);
        assert_eq!(
            status.unlink[1].reauth_with,
            [Provider::Google, Provider::Discord]
        );
        // Google off by config: only Discord could confirm, and only its own removal.
        let off = status_of(&view, &[Provider::Discord]);
        assert!(off.unlink[0].reauth_with.is_empty());
        assert_eq!(off.unlink[1].reauth_with, [Provider::Discord]);
        // RNF of logs: neither `Debug` shows the ids, and the body has no number.
        for shown in [format!("{status:?}"), format!("{view:?}")] {
            assert!(!shown.contains("424242"), "{shown}");
        }
        let body = serde_json::to_string(&status).unwrap();
        assert!(!body.chars().any(|c| c.is_ascii_digit()), "{body}");
    }
}
