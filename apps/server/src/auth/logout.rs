//! `POST /api/auth/logout` and `POST /api/auth/logout-all` (ADR-0008 D10, 008 RF-B11). Both
//! revoke (one session, or every session of the user), expire the cookie and answer 303 to
//! `/`, with or without a session: logging out is idempotent. If the revoke fails, a 303 to the
//! login page with `auth.try_again` and the cookie kept; over the "other mutations" quota of D7,
//! the same 303 (008 "Enmienda 2026-10-30" of RF-B5).

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::callback::try_again_redirect;

use crate::app::AppState;
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
    // Over a quota: the session is untouched, so the answer of a logout that could not revoke
    // (008 "Enmienda 2026-10-30" of RF-B5), never a JSON 429.
    if state
        .rate
        .mutation_ip
        .check(&key)
        .retry_after_secs()
        .is_some()
    {
        return cannot_revoke();
    }
    let hash = session::token_hash(state, headers);
    let lookup = session::current(state, headers).await.map_err(|_| ());
    if let Ok(Some(info)) = lookup
        && state
            .rate
            .mutation_user
            .check(&info.user_id)
            .retry_after_secs()
            .is_some()
    {
        return cannot_revoke();
    }
    match decide(scope, lookup, hash) {
        Plan::CannotRevoke => return cannot_revoke(),
        Plan::Nothing => {}
        Plan::Revoke(filter) => {
            let revoked = async {
                let mut tx = state.db.begin().await?;
                sessions::revoke(&mut tx, filter).await?;
                tx.commit().await
            };
            if revoked.await.is_err() {
                return cannot_revoke();
            }
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

enum Plan {
    Revoke(RevokeFilter),
    /// No cookie: nothing to revoke, and the logout is still a success (idempotent).
    Nothing,
    /// The session lookup failed: no revoke is safe to report.
    CannotRevoke,
}

/// What a logout does with the session lookup. A lookup that failed is never read as "no
/// session": logout-all would fall back to revoking only this cookie and answer success while
/// the other sessions stay alive (review of #54).
fn decide(
    scope: Scope,
    lookup: Result<Option<crate::sessions::SessionInfo>, ()>,
    hash: Option<crate::sessions::TokenHash>,
) -> Plan {
    match (scope, lookup, hash) {
        (_, Err(()), _) => Plan::CannotRevoke,
        (Scope::All, Ok(Some(info)), _) => Plan::Revoke(RevokeFilter::AllOfUser(info.user_id)),
        (_, Ok(_), Some(hash)) => Plan::Revoke(RevokeFilter::Token(hash)),
        (_, Ok(_), None) => Plan::Nothing,
    }
}

/// The session may still be alive, so never the "logged out" answer: no cookie expiry, and the
/// login page with `auth.try_again` (a form-post navigation must not land on raw JSON, as the
/// callback amendment says). With the session still valid, that page shows the error and a
/// link to the account. Also the answer over a quota (008 "Enmienda 2026-10-30" of RF-B5).
fn cannot_revoke() -> Response {
    try_again_redirect(PRIVATE_NO_STORE)
}

#[cfg(test)]
mod tests {
    use protocol::Role;
    use uuid::Uuid;

    use super::*;
    use crate::sessions::{SessionId, SessionInfo, TokenHash};

    fn info() -> SessionInfo {
        SessionInfo {
            id: SessionId(Uuid::from_u128(7)),
            user_id: Uuid::from_u128(1),
            role: Role::User,
            step_up: true,
            recent_admin: false,
        }
    }

    #[test]
    fn a_failed_lookup_never_revokes_and_never_claims_success() {
        let hash = Some(TokenHash::of("t"));
        for scope in [Scope::This, Scope::All] {
            assert!(matches!(decide(scope, Err(()), hash), Plan::CannotRevoke));
            assert!(matches!(decide(scope, Err(()), None), Plan::CannotRevoke));
        }
    }

    #[test]
    fn the_filter_follows_the_scope_and_the_session() {
        let hash = Some(TokenHash::of("t"));
        assert!(matches!(
            decide(Scope::All, Ok(Some(info())), hash),
            Plan::Revoke(RevokeFilter::AllOfUser(_))
        ));
        assert!(matches!(
            decide(Scope::This, Ok(Some(info())), hash),
            Plan::Revoke(RevokeFilter::Token(_))
        ));
        // A dead session still has its cookie row revoked by token, idempotently.
        for scope in [Scope::This, Scope::All] {
            assert!(matches!(
                decide(scope, Ok(None), hash),
                Plan::Revoke(RevokeFilter::Token(_))
            ));
            assert!(matches!(decide(scope, Ok(None), None), Plan::Nothing));
        }
    }
}
