//! Reauthenticating without changing user (009 RF11, RF12; plan decision 5).
//!
//! `POST /api/auth/reauth/{provider}` needs a session and an account of that provider, enabled,
//! and starts the flow of `start` with `purpose = 'reauth'` and the user of the session. The
//! callback (see `callback.rs`) checks that the session is still that user's, talks to the
//! provider and then runs [`reauth`]: one transaction that locks the user, rotates the session
//! (and refuses if it was closed meanwhile) and issues a fresh one, but only if the identity the
//! provider returned is one of the user's. It never creates a user or an account, and never
//! changes user: an identity that does not exist and one of another user are the same
//! `auth.reauth_mismatch`, and the session in use stays as it was (nothing committed).

use axum::extract::rejection::FormRejection;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use protocol::{ApiErrorCode, CallbackErrorCode, Provider, Role};
use sqlx::Row;
use uuid::Uuid;

use super::callback::{code_redirect, try_again_redirect};
use super::link::to_login;
use super::login::Identity;
use super::start::{Purpose, StartForm, begin_flow, flow_error};
use crate::app::AppState;
use crate::http::errors::error_response;
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::sessions::{
    self, Authentication, Issued, OauthAccountId, RevokeFilter, SessionId, UaFamily,
};

/// `POST /api/auth/reauth/{provider}`. A text that is no provider is 404 (JSON), as in `start`
/// and `link`. Then the session: none is a 303 to the login page, a database error a 303 to
/// `auth.try_again`. A provider that is off, or that the user has no account of, is a 303 to
/// `auth.reauth_mismatch`, with no flow and no cookie change. Then the flow.
pub async fn handler(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    headers: HeaderMap,
    form: Result<Form<StartForm>, FormRejection>,
) -> Response {
    let Some(provider) = Provider::parse(&segment) else {
        return error_response(StatusCode::NOT_FOUND, ApiErrorCode::NotFound);
    };
    // The session first, so a reauth without one never creates a flow.
    let info = match session::current(&state, &headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return to_login(&state),
        Err(_) => return try_again_redirect(PRIVATE_NO_STORE),
    };
    if !state.auth.providers.contains(&provider) {
        return code_redirect(CallbackErrorCode::ReauthMismatch, PRIVATE_NO_STORE);
    }
    let linked = sqlx::query("SELECT 1 FROM oauth_accounts WHERE user_id = $1 AND provider = $2")
        .bind(info.user_id)
        .bind(provider.as_str())
        .fetch_optional(&state.db)
        .await;
    match linked {
        Ok(Some(_)) => {}
        Ok(None) => return code_redirect(CallbackErrorCode::ReauthMismatch, PRIVATE_NO_STORE),
        Err(error) => {
            failed(&error);
            return try_again_redirect(PRIVATE_NO_STORE);
        }
    }
    match begin_flow(&state, provider, form, Purpose::Reauth(info.user_id)).await {
        Ok(response) => response,
        Err(code) => flow_error(&state.auth.cookies, code),
    }
}

/// What the transaction of a reauthentication needs from the callback.
pub struct ReauthRequest<'a> {
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

/// The reauth transaction. On any error nothing commits, so the session in use is intact.
pub async fn reauth(
    state: &AppState,
    request: ReauthRequest<'_>,
) -> Result<Issued, CallbackErrorCode> {
    let mut tx = state.db.begin().await.map_err(|error| failed(&error))?;
    match attempt(&mut tx, &request).await {
        Ok(issued) => {
            tx.commit().await.map_err(|error| failed(&error))?;
            Ok(issued)
        }
        // Dropping the transaction rolls it back (no session, no event, no NOTIFY).
        Err(Refused::Code(code)) => Err(code),
        Err(Refused::Db(error)) => Err(failed(&error)),
    }
}

/// `auth.reauth_failed` with the kind of the error only (ADR-0012 A4).
fn failed(error: &sqlx::Error) -> CallbackErrorCode {
    tracing::warn!(
        event = "auth.reauth_failed",
        error_kind = crate::db::error_kind(error)
    );
    CallbackErrorCode::TryAgain
}

async fn attempt(
    tx: &mut sqlx::PgConnection,
    request: &ReauthRequest<'_>,
) -> Result<Issued, Refused> {
    use CallbackErrorCode::{InvalidState, ReauthMismatch};
    // In series with every link, unlink and logout-all of the user; a banned user has no row.
    let user = sqlx::query("SELECT role FROM users WHERE id = $1 AND banned_at IS NULL FOR UPDATE")
        .bind(request.user)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Refused::Code(InvalidState))?;
    let role = if user.try_get::<&str, _>(0)? == "admin" {
        Role::Admin
    } else {
        Role::User
    };
    // The rotation (D3): exactly the session checked before the exchange. If a logout-all, a
    // ban or a sibling flow closed it meanwhile, a reauth now would revive it.
    let revoked = sessions::revoke(tx, RevokeFilter::One(request.session)).await?;
    if revoked != [request.session] {
        return Err(Refused::Code(InvalidState));
    }
    let provider = request.identity.provider;
    let account = sqlx::query(
        "SELECT id, user_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2",
    )
    .bind(provider.as_str())
    .bind(&request.identity.subject)
    .fetch_optional(&mut *tx)
    .await?;
    // Absent and another user's are the same answer: nothing tells whether it exists.
    let account = match account {
        Some(row) if row.try_get::<Uuid, _>(1)? == request.user => {
            OauthAccountId::new(row.try_get(0)?)
        }
        _ => return Err(Refused::Code(ReauthMismatch)),
    };
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, provider, ip_hash) VALUES ($1, 'reauth', $2, $3)",
    )
    .bind(request.user)
    .bind(provider.as_str())
    .bind(request.ip_hash.as_slice())
    .execute(&mut *tx)
    .await?;
    Ok(sessions::issue(
        tx,
        request.user,
        role,
        Authentication::Fresh(account),
        &request.ip_hash,
        request.ua,
    )
    .await?)
}
