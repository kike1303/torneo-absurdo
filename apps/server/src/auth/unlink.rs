//! `POST /api/auth/unlink/{provider}` (ADR-0008 D1, 008 RF-C3 and its amendment 2026-10-29, 009
//! RF13 to RF15).
//!
//! In one transaction, in the order of 009 RF14: lock the user, read the session with its row
//! locked (401 if it closed meanwhile), find that provider's account (404), count what would be
//! left among the **enabled** providers (409 `auth.last_provider`), apply the rule of RF13 with
//! what the lock read (403 `auth.reauth_required`), then delete the account, write
//! `auth_events`, revoke every other session of the user and consume the confirmation of this
//! one. It never reads the body, like `logout`: no form, so nothing for `DefaultBodyLimit` to
//! reject. "Other mutations" quota of D7.

use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{ApiErrorCode, AuthErrorCode, ErrorCode, Provider};
use sqlx::Row;

use crate::app::AppState;
use crate::http::errors::{error_response, too_many_requests};
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::net::{ClientContext, IpKey};
use crate::sessions::{self, OauthAccountId, RevokeFilter, SessionInfo};

/// A pause between the session check of an unlink and its transaction (009 RF15). Production
/// uses [`NoUnlinkGate`]; the race tests close or change the session there.
pub trait UnlinkGate: Send + Sync {
    fn before_unlink(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

pub struct NoUnlinkGate;

impl UnlinkGate for NoUnlinkGate {
    fn before_unlink(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

/// The gate of `Settings` (test code only, never an environment variable).
#[derive(Clone)]
pub struct UnlinkGateConfig(pub Arc<dyn UnlinkGate>);

impl Default for UnlinkGateConfig {
    fn default() -> Self {
        Self(Arc::new(NoUnlinkGate))
    }
}

impl fmt::Debug for UnlinkGateConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UnlinkGateConfig(..)")
    }
}

/// RF13: a confirmation (current and of the last 10 minutes) whose account is `target` itself or
/// one linked before it. `account` is `None` without an authentication or after an unlink
/// consumed it. The one rule of unlink: the transaction and `reauth-status` both use it.
pub fn unlink_allowed(
    step_up: bool,
    account: Option<OauthAccountId>,
    target: OauthAccountId,
) -> bool {
    step_up && account.is_some_and(|account| account <= target)
}

enum Refused {
    /// Banned after the session check (the lock waited for the ban to commit).
    Banned,
    /// The session closed or rotated after the check (RF14, step 2).
    Gone,
    NotLinked,
    LastProvider,
    ReauthRequired,
    Db,
}

impl From<sqlx::Error> for Refused {
    fn from(error: sqlx::Error) -> Self {
        tracing::warn!(
            event = "auth.unlink_failed",
            error_kind = crate::db::error_kind(&error)
        );
        Self::Db
    }
}

fn private(status: StatusCode, code: impl Into<ErrorCode>) -> Response {
    let mut response = error_response(status, code);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}

pub async fn handler(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let key = client.map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.0.key);
    if let Some(secs) = state.rate.mutation_ip.check(&key).retry_after_secs() {
        return too_many_requests(ApiErrorCode::RateLimited, secs);
    }
    // Any known provider, enabled or not: a provider the config turned off can be unlinked.
    let Some(provider) = Provider::parse(&segment) else {
        return private(StatusCode::NOT_FOUND, ApiErrorCode::NotFound);
    };
    let info = match session::current(&state, &headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return session::unauthenticated(&state),
        Err(_) => return session::try_again(),
    };
    if let Some(secs) = state
        .rate
        .mutation_user
        .check(&info.user_id)
        .retry_after_secs()
    {
        return too_many_requests(ApiErrorCode::RateLimited, secs);
    }
    state.auth.unlink_gate.0.before_unlink().await;
    let ip_hash = key.hash(&state.auth.ip_hash_key);
    match unlink(&state, &info, provider, &ip_hash).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, HeaderValue::from_static("/")),
                (header::CACHE_CONTROL, PRIVATE_NO_STORE),
            ],
        )
            .into_response(),
        Err(Refused::Banned | Refused::Gone) => session::unauthenticated(&state),
        Err(Refused::NotLinked) => private(StatusCode::NOT_FOUND, ApiErrorCode::NotFound),
        Err(Refused::LastProvider) => private(StatusCode::CONFLICT, AuthErrorCode::LastProvider),
        Err(Refused::ReauthRequired) => {
            private(StatusCode::FORBIDDEN, AuthErrorCode::ReauthRequired)
        }
        Err(Refused::Db) => session::try_again(),
    }
}

async fn unlink(
    state: &AppState,
    info: &SessionInfo,
    provider: Provider,
    ip_hash: &[u8; 32],
) -> Result<(), Refused> {
    let mut tx = state.db.begin().await?;
    // 1. In series with every link, reauth, unlink and logout-all of the same user. `banned_at`
    // again here, as in `link`: a ban that committed after `current()` stops the unlink (under
    // READ COMMITTED the lock waits for the ban and re-reads the row).
    sqlx::query("SELECT 1 FROM users WHERE id = $1 AND banned_at IS NULL FOR UPDATE")
        .bind(info.user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Refused::Banned)?;
    // 2. The session again, under the lock: what `current()` read may be stale.
    let auth = sessions::lock_auth(&mut tx, info.id)
        .await?
        .filter(|auth| auth.user_id == info.user_id)
        .ok_or(Refused::Gone)?;
    // 3.
    let target = sqlx::query("SELECT id FROM oauth_accounts WHERE user_id = $1 AND provider = $2")
        .bind(info.user_id)
        .bind(provider.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Refused::NotLinked)?;
    let target = OauthAccountId::new(target.try_get(0)?);
    // 4. Before the rule: a user with one provider gets no invitation to reauthenticate that
    // would not help. Only an enabled provider lets the user back in (amendment 2026-10-29).
    let enabled: Vec<&str> = state.auth.providers.iter().map(|p| p.as_str()).collect();
    let left: i64 = sqlx::query(
        "SELECT count(*) FROM oauth_accounts WHERE user_id = $1 AND provider = ANY($2) \
         AND id <> $3",
    )
    .bind(info.user_id)
    .bind(&enabled)
    .bind(target.get())
    .fetch_one(&mut *tx)
    .await?
    .try_get(0)?;
    if left == 0 {
        return Err(Refused::LastProvider);
    }
    // 5. RF13, with what step 2 read.
    if !unlink_allowed(auth.step_up, auth.account, target) {
        return Err(Refused::ReauthRequired);
    }
    // 6.
    sqlx::query("DELETE FROM oauth_accounts WHERE id = $1 RETURNING id")
        .bind(target.get())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Refused::NotLinked)?;
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, provider, ip_hash) VALUES ($1, 'unlink', $2, $3)",
    )
    .bind(info.user_id)
    .bind(provider.as_str())
    .bind(ip_hash.as_slice())
    .execute(&mut *tx)
    .await?;
    sessions::revoke(
        &mut tx,
        RevokeFilter::OthersOfUser {
            user: info.user_id,
            keep: info.id,
        },
    )
    .await?;
    // One thing per confirmation: unlinking again, or linking, needs another one.
    sessions::consume_step_up(&mut tx, info.id).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_of_unlink() {
        let id = OauthAccountId::new;
        // (step_up, account, target, allowed)
        for (step_up, account, target, allowed) in [
            (true, Some(id(2)), id(2), true),
            (true, Some(id(1)), id(2), true),
            (true, Some(id(3)), id(2), false),
            (true, None, id(2), false),
            (false, Some(id(1)), id(2), false),
            (false, Some(id(2)), id(2), false),
            (false, None, id(2), false),
        ] {
            assert_eq!(
                unlink_allowed(step_up, account, target),
                allowed,
                "{step_up} {:?} {}",
                account.map(OauthAccountId::get),
                target.get()
            );
        }
    }
}
