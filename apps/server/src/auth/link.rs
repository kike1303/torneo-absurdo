//! Linking a second provider (ADR-0008 D1, D3; 008 RF-C2, plan-c decision 7).
//!
//! `POST /api/auth/link/{provider}` needs a session with a confirmation (009 RF8) and starts the
//! same flow as `start`, with `purpose = 'link'` and the user of the session. The callback (see
//! `callback.rs`) checks that the session is still that user's, talks to the provider and then
//! runs [`link`]: one transaction that locks the user, reads the session with its row locked and
//! needs a current authentication of the last 20 minutes (RF9), issues the new session as a copy
//! of the old one (RF10: a link never counts as a reauthentication and never extends a session),
//! rotates the old one (and refuses if it was closed in the meantime), inserts the identity
//! without ever raising a violation and writes `auth_events`. Nothing ever merges two users, and
//! no error touches the session in use (the transaction did not commit).

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::rejection::FormRejection;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{ApiErrorCode, CallbackErrorCode};
use sqlx::Row;
use uuid::Uuid;

use super::callback::{code_redirect, login_location, try_again_redirect};
use super::login::Identity;
use super::start::{Purpose, StartForm, begin_flow, flow_error};
use crate::app::AppState;
use crate::http::errors::error_response;
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::sessions::{self, Authentication, Issued, RevokeFilter, SessionId, UaFamily};

/// A pause between the session check of a link or reauth callback and its transaction.
/// Production uses [`NoLinkGate`]; the race tests close the session there (plan-c decision 7,
/// 009 plan decision 5).
pub trait LinkGate: Send + Sync {
    fn before_link(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

pub struct NoLinkGate;

impl LinkGate for NoLinkGate {
    fn before_link(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

/// The gate of `Settings` (test code only, never an environment variable).
#[derive(Clone)]
pub struct LinkGateConfig(pub Arc<dyn LinkGate>);

impl Default for LinkGateConfig {
    fn default() -> Self {
        Self(Arc::new(NoLinkGate))
    }
}

impl fmt::Debug for LinkGateConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LinkGateConfig(..)")
    }
}

/// `POST /api/auth/link/{provider}`: an unknown or disabled provider is 404 (JSON); no session
/// is a 303 to the login page and a database error a 303 to `auth.try_again` (008 "Enmienda
/// 2026-10-30" of RF-C2); a session without a confirmation is a 303 to `auth.reauth_required`
/// with no flow and no cookie change (009 RF8); with one, the flow (a rejected form is
/// `auth.invalid_state`).
pub async fn handler(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    headers: HeaderMap,
    form: Result<Form<StartForm>, FormRejection>,
) -> Response {
    let Some(provider) = state.auth.provider(&segment) else {
        return error_response(StatusCode::NOT_FOUND, ApiErrorCode::NotFound);
    };
    // The session first, so a link without one never creates a flow (security-reviewer).
    let info = match session::current(&state, &headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return to_login(&state),
        Err(_) => return try_again_redirect(PRIVATE_NO_STORE),
    };
    if !info.step_up {
        return code_redirect(CallbackErrorCode::ReauthRequired, PRIVATE_NO_STORE);
    }
    match begin_flow(&state, provider, form, Purpose::Link(info.user_id)).await {
        Ok(response) => response,
        Err(code) => flow_error(&state.auth.cookies, code),
    }
}

/// No session (absent, expired or revoked): 303 to the login page with no `error` (its form is
/// the answer; `auth.invalid_state` would mislead), and the session cookie expired (008
/// "Enmienda 2026-10-30" of RF-C2). `private, no-store`: it read the session. Also the answer of
/// a reauth without a session (009 RF11).
pub(super) fn to_login(state: &AppState) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, login_location()),
            (header::SET_COOKIE, state.auth.cookies.expire_session()),
            (header::CACHE_CONTROL, PRIVATE_NO_STORE),
        ],
    )
        .into_response()
}

/// What the transaction of a link needs from the callback.
pub struct LinkRequest<'a> {
    pub user: Uuid,
    /// The session of the callback, already checked to be `user`'s.
    pub session: SessionId,
    pub identity: &'a Identity,
    pub ip_hash: [u8; 32],
    pub ua: UaFamily,
}

enum Refused {
    Code(CallbackErrorCode),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for Refused {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(error)
    }
}

/// The link transaction. On any error nothing commits, so the session in use is intact.
pub async fn link(state: &AppState, request: LinkRequest<'_>) -> Result<Issued, CallbackErrorCode> {
    let mut tx = state.db.begin().await.map_err(failed)?;
    match attempt(&mut tx, &request).await {
        Ok(issued) => {
            tx.commit().await.map_err(failed)?;
            Ok(issued)
        }
        // Dropping the transaction rolls it back (no row, no event, no NOTIFY).
        Err(Refused::Code(code)) => Err(code),
        Err(Refused::Db(error)) => Err(failed(error)),
    }
}

fn failed(error: sqlx::Error) -> CallbackErrorCode {
    tracing::warn!(
        event = "auth.link_failed",
        error_kind = crate::db::error_kind(&error)
    );
    CallbackErrorCode::TryAgain
}

async fn attempt(
    tx: &mut sqlx::PgConnection,
    request: &LinkRequest<'_>,
) -> Result<Issued, Refused> {
    use CallbackErrorCode::{AlreadyLinked, InvalidState, ProviderAlreadyLinked, ReauthRequired};
    // In series with every link, reauth, unlink and logout-all of the same user; a banned user
    // has no row.
    let user = sqlx::query("SELECT role FROM users WHERE id = $1 AND banned_at IS NULL FOR UPDATE")
        .bind(request.user)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Refused::Code(InvalidState))?;
    let role = if user.try_get::<&str, _>(0)? == "admin" {
        protocol::Role::Admin
    } else {
        protocol::Role::User
    };
    // RF9, with the row of the session locked. Closed in the meantime (a logout-all, a ban, a
    // sibling link) is `auth.invalid_state`, as in 008; alive without an authentication that
    // counts (too old, or something linked or unlinked since) is `auth.reauth_required`.
    let Some(auth) = sessions::lock_auth(tx, request.session).await? else {
        return Err(Refused::Code(InvalidState));
    };
    if auth.user_id != request.user {
        return Err(Refused::Code(InvalidState));
    }
    if !auth.link_window {
        return Err(Refused::Code(ReauthRequired));
    }
    // RF10: the new session copies the authentication and the expiry of the one locked above,
    // so it is issued before the rotation deletes it.
    let issued = sessions::issue(
        tx,
        request.user,
        role,
        Authentication::KeptFrom(request.session),
        &request.ip_hash,
        request.ua,
    )
    .await?;
    // The rotation (D3): exactly the session checked before the exchange. With the lock above it
    // cannot fail; the comparison stays as a defense.
    let revoked = sessions::revoke(tx, RevokeFilter::One(request.session)).await?;
    if revoked != [request.session] {
        return Err(Refused::Code(InvalidState));
    }
    let provider = request.identity.provider;
    // No conflict target: it covers both unique keys, and never leaves a violation (with the
    // identity in it) in the Postgres log.
    let inserted = sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ($1, $2, $3) \
         ON CONFLICT DO NOTHING RETURNING user_id",
    )
    .bind(provider.as_str())
    .bind(&request.identity.subject)
    .bind(request.user)
    .fetch_optional(&mut *tx)
    .await?;
    if inserted.is_none() {
        let owner: Option<Uuid> = sqlx::query(
            "SELECT user_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2",
        )
        .bind(provider.as_str())
        .bind(&request.identity.subject)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| row.try_get(0))
        .transpose()?;
        return Err(Refused::Code(match owner {
            Some(owner) if owner != request.user => AlreadyLinked,
            // The same identity, or another account of that provider for this user.
            _ => ProviderAlreadyLinked,
        }));
    }
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, provider, ip_hash) VALUES ($1, 'link', $2, $3)",
    )
    .bind(request.user)
    .bind(provider.as_str())
    .bind(request.ip_hash.as_slice())
    .execute(&mut *tx)
    .await?;
    Ok(issued)
}

#[cfg(test)]
mod tests {
    use protocol::Provider;

    /// `POST /api/auth/link/start` falls in `link` (static segment first) with the provider
    /// `start`, a 404: no provider may be called like a route word (plan-c decision 15).
    #[test]
    fn no_provider_is_called_like_a_route_word() {
        for provider in Provider::ALL {
            assert!(
                ![
                    "link",
                    "reauth",
                    "reauth-status",
                    "unlink",
                    "start",
                    "callback",
                    "providers",
                    "logout",
                    "logout-all"
                ]
                .contains(&provider.as_str()),
                "{provider:?}"
            );
        }
    }
}
