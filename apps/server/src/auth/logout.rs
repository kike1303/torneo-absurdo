//! `POST /api/auth/logout` and `POST /api/auth/logout-all` (ADR-0008 D10, 008 RF-B11). Both
//! revoke (one session, or every session of the user), expire the cookie and answer 303 to
//! `/`. `logout` does so with or without a session: logging out is idempotent. `logout-all`
//! without a valid session closes at most the row of its own cookie, so it answers 303 to the
//! login page instead, and never looks like the closing it did not do (008 "Enmienda 2026-10-30
//! (TA-30)" of RF-B5). If the revoke fails, a 303 to the login page with `auth.try_again` and the
//! cookie kept; over the "other mutations" quota of D7, the same 303 (008 "Enmienda 2026-10-30"
//! of RF-B5).

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::callback::{login_location, try_again_redirect};

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
    let landing = match decide(scope, lookup, hash) {
        Plan::CannotRevoke => return cannot_revoke(),
        Plan::Nothing(landing) => landing,
        Plan::Revoke(filter, landing) => {
            let revoked = async {
                let mut tx = state.db.begin().await?;
                // logout-all first takes the lock of the user, as link, reauth, unlink and the
                // CLI do: every write of the sessions of a user runs in series, and an unlink
                // holding one session while it revokes the others never forms a cycle with it
                // (009 plan decision 6). No `banned_at IS NULL`: a ban that commits while this
                // waits for the lock must not fail the logout. A cookie with no valid session
                // never gets here with `AllOfUser`: `decide` gives it `Token`, one row with no
                // lock.
                if let RevokeFilter::AllOfUser(user) = filter {
                    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
                        .bind(user)
                        .fetch_one(&mut *tx)
                        .await?;
                }
                sessions::revoke(&mut tx, filter).await?;
                tx.commit().await
            };
            if revoked.await.is_err() {
                return cannot_revoke();
            }
            landing
        }
    };
    let location = match landing {
        Landing::Home => HeaderValue::from_static("/"),
        Landing::Login => login_location(),
    };
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::SET_COOKIE, state.auth.cookies.expire_session()),
            (header::CACHE_CONTROL, PRIVATE_NO_STORE),
        ],
    )
        .into_response()
}

/// Where a logout that ran ends up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Landing {
    /// All that was asked for is closed.
    Home,
    /// A `logout-all` that proved no session: the others, if any, are still alive.
    Login,
}

enum Plan {
    Revoke(RevokeFilter, Landing),
    /// No cookie: nothing to revoke.
    Nothing(Landing),
    /// The session lookup failed: no revoke is safe to report.
    CannotRevoke,
}

/// What a logout does with the session lookup. A lookup that failed is never read as "no
/// session": logout-all would fall back to revoking only this cookie and answer success while
/// the other sessions stay alive (review of #54). A cookie that is dead, unknown or missing proves
/// nothing, so it never closes the other sessions, and `logout-all` then never answers as if it
/// had (TA-30). No cookie counts too: the cookie has `Max-Age` up to the absolute expiry, so the
/// browser usually arrives at that death without it.
fn decide(
    scope: Scope,
    lookup: Result<Option<crate::sessions::SessionInfo>, ()>,
    hash: Option<crate::sessions::TokenHash>,
) -> Plan {
    // With `All`, this is only for the arms below, where no session was proved.
    let landing = match scope {
        Scope::This => Landing::Home,
        Scope::All => Landing::Login,
    };
    match (scope, lookup, hash) {
        (_, Err(()), _) => Plan::CannotRevoke,
        (Scope::All, Ok(Some(info)), _) => {
            Plan::Revoke(RevokeFilter::AllOfUser(info.user_id), Landing::Home)
        }
        (_, Ok(_), Some(hash)) => Plan::Revoke(RevokeFilter::Token(hash), landing),
        (_, Ok(_), None) => Plan::Nothing(landing),
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
            Plan::Revoke(RevokeFilter::AllOfUser(_), Landing::Home)
        ));
        assert!(matches!(
            decide(Scope::This, Ok(Some(info())), hash),
            Plan::Revoke(RevokeFilter::Token(_), Landing::Home)
        ));
    }

    /// A dead session still has its cookie row revoked by token, and never the other sessions.
    /// `logout` is idempotent; `logout-all` does not answer as if it had closed them (TA-30).
    #[test]
    fn without_a_valid_session_only_logout_all_goes_to_the_login_page() {
        let hash = Some(TokenHash::of("t"));
        for (scope, expected) in [(Scope::This, Landing::Home), (Scope::All, Landing::Login)] {
            assert!(matches!(
                decide(scope, Ok(None), hash),
                Plan::Revoke(RevokeFilter::Token(_), landing) if landing == expected
            ));
            assert!(matches!(
                decide(scope, Ok(None), None),
                Plan::Nothing(landing) if landing == expected
            ));
        }
    }
}
