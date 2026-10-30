//! The login transaction (008 RF-B3, RF-B4).
//!
//! An existing identity gets a session (or `auth.banned`). A new one, in **one** transaction:
//! the user with a generated alias (up to 10 tries on the unique key), the OAuth account, the
//! ledger account, the `grant` of `DAILY_GRANT`, the `auth_events` and the session. If a
//! concurrent first login of the same identity wins the OAuth account, this one rolls back and
//! retries once as an existing login. A session the request brought is revoked in the same
//! transaction (rotation, D3).

use protocol::{Provider, Role};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

use super::AuthState;
use crate::ledger;
use crate::profile::name;
use crate::sessions::{self, Issued, RevokeFilter, TokenHash, UaFamily};

/// Tries of the generated alias before `auth.try_again` (ADR-0008 D1).
pub const ALIAS_TRIES: usize = 10;

/// `(provider, provider_user_id)`: the identity (D1).
#[derive(Clone)]
pub struct Identity {
    pub provider: Provider,
    pub subject: String,
}

// The subject is a provider id (a canary of RF-B17): never in a `Debug`.
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LoginError {
    #[error("the user is banned")]
    Banned,
    /// The aliases kept colliding, or the database failed: nothing was created.
    #[error("try again")]
    TryAgain,
}

impl From<sqlx::Error> for LoginError {
    fn from(error: sqlx::Error) -> Self {
        tracing::warn!(
            event = "auth.login_failed",
            error_kind = crate::db::error_kind(&error)
        );
        Self::TryAgain
    }
}

fn role(text: &str) -> Role {
    if text == "admin" {
        Role::Admin
    } else {
        Role::User
    }
}

enum Attempt {
    Done(Issued),
    /// A concurrent first login created the identity: retry as an existing login.
    Lost,
}

pub async fn login(
    auth: &AuthState,
    pool: &PgPool,
    identity: &Identity,
    ip_hash: [u8; 32],
    ua: UaFamily,
    previous: Option<TokenHash>,
) -> Result<Issued, LoginError> {
    for _ in 0..2 {
        let mut tx = pool.begin().await?;
        let attempt = attempt(auth, &mut tx, identity, &ip_hash, ua, previous).await;
        match attempt {
            Ok(Attempt::Done(issued)) => {
                tx.commit().await?;
                return Ok(issued);
            }
            Ok(Attempt::Lost) => tx.rollback().await?,
            Err(error) => {
                // Dropping the transaction rolls it back.
                return Err(error);
            }
        }
    }
    Err(LoginError::TryAgain)
}

async fn attempt(
    auth: &AuthState,
    tx: &mut PgConnection,
    identity: &Identity,
    ip_hash: &[u8; 32],
    ua: UaFamily,
    previous: Option<TokenHash>,
) -> Result<Attempt, LoginError> {
    let existing = sqlx::query(
        "SELECT u.id, u.role, u.banned_at IS NOT NULL FROM oauth_accounts o \
         JOIN users u ON u.id = o.user_id WHERE o.provider = $1 AND o.provider_user_id = $2",
    )
    .bind(identity.provider.as_str())
    .bind(&identity.subject)
    .fetch_optional(&mut *tx)
    .await?;

    let (user_id, user_role) = match existing {
        Some(row) => {
            if row.try_get::<bool, _>(2)? {
                return Err(LoginError::Banned);
            }
            (row.try_get::<Uuid, _>(0)?, role(row.try_get(1)?))
        }
        None => {
            let Some(user_id) = create_user(auth, tx).await? else {
                return Err(LoginError::TryAgain);
            };
            let linked = sqlx::query(
                "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) \
                 VALUES ($1, $2, $3) ON CONFLICT (provider, provider_user_id) DO NOTHING \
                 RETURNING user_id",
            )
            .bind(identity.provider.as_str())
            .bind(&identity.subject)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
            if linked.is_none() {
                return Ok(Attempt::Lost);
            }
            ledger::create_user_account(tx, user_id).await?;
            let entry = ledger::grant(tx, user_id, auth.daily_grant).await?;
            if let Some(entry) = entry {
                event(tx, user_id, "grant", None, Some(entry), ip_hash).await?;
            }
            (user_id, Role::User)
        }
    };
    event(tx, user_id, "login", Some(identity.provider), None, ip_hash).await?;
    if let Some(previous) = previous {
        sessions::revoke(tx, RevokeFilter::Token(previous)).await?;
    }
    let issued = sessions::issue(tx, user_id, user_role, ip_hash, ua).await?;
    Ok(Attempt::Done(issued))
}

/// The user row with a generated alias, or `None` after [`ALIAS_TRIES`] collisions. The
/// unique index on the key decides; there is no prior "exists?" check.
async fn create_user(auth: &AuthState, tx: &mut PgConnection) -> Result<Option<Uuid>, sqlx::Error> {
    for _ in 0..ALIAS_TRIES {
        let Some(candidate) = auth.alias.next() else {
            continue;
        };
        let Ok(alias) = name::validate(&candidate) else {
            continue;
        };
        let row = sqlx::query(
            "INSERT INTO users (display_name, display_name_key) VALUES ($1, $2) \
             ON CONFLICT (display_name_key) DO NOTHING RETURNING id",
        )
        .bind(alias.as_str())
        .bind(alias.key())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = row {
            return Ok(Some(row.try_get(0)?));
        }
    }
    Ok(None)
}

async fn event(
    tx: &mut PgConnection,
    user_id: Uuid,
    kind: &'static str,
    provider: Option<Provider>,
    entry: Option<Uuid>,
    ip_hash: &[u8; 32],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, provider, ledger_entry_id, ip_hash) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(kind)
    .bind(provider.map(Provider::as_str))
    .bind(entry)
    .bind(ip_hash.as_slice())
    .execute(&mut *tx)
    .await?;
    Ok(())
}
