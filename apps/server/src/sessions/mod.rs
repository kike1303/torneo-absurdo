//! Sessions (ADR-0008 D3, 008 RF-B8, RF-B10).
//!
//! - The token is 256 bits of `OsRng` in base64url; the database keeps only `SHA-256(token)`.
//! - Valid while not banned, before `expires_at` (30 days, 12 hours for an admin) and seen in
//!   the last 7 days. `last_seen_at` is touched at most every 5 minutes.
//! - Every time comes from Postgres' `now()`: no clock in the application.
//! - [`revoke`] is the only code that deletes sessions, and it notifies every deleted id on
//!   [`REVOKED_CHANNEL`] in the same transaction, so the sockets close at once (D6).

pub mod listener;

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::Role;
use rand_core::{OsRng, TryRngCore};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

/// The `LISTEN`/`NOTIFY` channel of revoked session ids.
pub const REVOKED_CHANNEL: &str = "session_revoked";

const USER_MAX_AGE_SECS: u64 = 30 * 24 * 3600;
const ADMIN_MAX_AGE_SECS: u64 = 12 * 3600;

/// The absolute lifetime of a session: also the `Max-Age` of its cookie.
pub const fn max_age_secs(role: Role) -> u64 {
    match role {
        Role::User => USER_MAX_AGE_SECS,
        Role::Admin => ADMIN_MAX_AGE_SECS,
    }
}

/// The value of the session cookie. `Debug` never shows it.
pub struct SessionToken(String);

impl SessionToken {
    /// 32 bytes of `OsRng`, base64url without padding. `None` if the OS source fails.
    pub fn generate() -> Option<Self> {
        let mut bytes = [0u8; 32];
        OsRng.try_fill_bytes(&mut bytes).ok()?;
        Some(Self(URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn hash(&self) -> TokenHash {
        TokenHash::of(&self.0)
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(..)")
    }
}

/// `SHA-256` of a session token: what the database stores and is searched by.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokenHash(pub [u8; 32]);

impl TokenHash {
    pub fn of(token: &str) -> Self {
        Self(Sha256::digest(token.as_bytes()).into())
    }
}

impl fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenHash(..)")
    }
}

/// The internal id of a session (socket registry, NOTIFY). Never in a cookie, a body or a log:
/// no `Display`, and `Debug` hides it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub(crate) Uuid);

impl SessionId {
    pub fn uuid(self) -> Uuid {
        self.0
    }

    /// Parses a `NOTIFY` payload.
    pub fn parse(text: &str) -> Option<Self> {
        Uuid::parse_str(text).ok().map(Self)
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId(..)")
    }
}

/// A valid session. `Debug` shows nothing that identifies it.
#[derive(Clone, Copy)]
pub struct SessionInfo {
    pub id: SessionId,
    pub user_id: Uuid,
    pub role: Role,
}

impl fmt::Debug for SessionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionInfo")
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

/// A newly issued session.
#[derive(Debug)]
pub struct Issued {
    pub id: SessionId,
    pub token: SessionToken,
    pub max_age_secs: u64,
}

/// The coarse browser family kept in `sessions.user_agent_family`. Never the `User-Agent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UaFamily {
    Chrome,
    Firefox,
    Safari,
    Edge,
    Other,
}

impl UaFamily {
    /// Substring rules, in order: Edge and Chrome on iOS say "Safari" and Edge says "Chrome".
    pub fn of(user_agent: Option<&str>) -> Self {
        let Some(ua) = user_agent else {
            return Self::Other;
        };
        let ua = ua.to_ascii_lowercase();
        if ua.contains("edg/") || ua.contains("edga/") || ua.contains("edgios/") {
            Self::Edge
        } else if ua.contains("firefox/") || ua.contains("fxios/") {
            Self::Firefox
        } else if ua.contains("chrome/") || ua.contains("crios/") {
            Self::Chrome
        } else if ua.contains("safari/") {
            Self::Safari
        } else {
            Self::Other
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Firefox => "firefox",
            Self::Safari => "safari",
            Self::Edge => "edge",
            Self::Other => "other",
        }
    }
}

fn role_from_db(text: &str) -> Role {
    if text == "admin" {
        Role::Admin
    } else {
        Role::User
    }
}

/// A new session for `user_id`, in the transaction of the login.
pub async fn issue(
    tx: &mut PgConnection,
    user_id: Uuid,
    role: Role,
    ip_hash: &[u8; 32],
    ua: UaFamily,
) -> Result<Issued, sqlx::Error> {
    let token = SessionToken::generate()
        .ok_or_else(|| sqlx::Error::Protocol("the OS random source failed".to_owned()))?;
    let max_age = max_age_secs(role);
    let row = sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, ip_hash, user_agent_family) \
         VALUES ($1, $2, now() + $3::bigint * interval '1 second', $4, $5) RETURNING id",
    )
    .bind(token.hash().0.as_slice())
    .bind(user_id)
    .bind(i64::try_from(max_age).unwrap_or(i64::MAX))
    .bind(ip_hash.as_slice())
    .bind(ua.as_str())
    .fetch_one(&mut *tx)
    .await?;
    Ok(Issued {
        id: SessionId(row.try_get(0)?),
        token,
        max_age_secs: max_age,
    })
}

/// The session of a cookie, if valid; touches `last_seen_at` with the 5-minute rule.
pub async fn validate(pool: &PgPool, hash: &TokenHash) -> Result<Option<SessionInfo>, sqlx::Error> {
    let row = sqlx::query(const_format_valid::BY_HASH)
        .bind(hash.0.as_slice())
        .fetch_optional(pool)
        .await?;
    finish(pool, row).await
}

/// The same check by id (the revalidation of a socket).
pub async fn validate_by_id(
    pool: &PgPool,
    id: SessionId,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let row = sqlx::query(const_format_valid::BY_ID)
        .bind(id.0)
        .fetch_optional(pool)
        .await?;
    finish(pool, row).await
}

/// The ids among `ids` that are still valid (the revalidation after the listener reconnects).
pub async fn still_valid(pool: &PgPool, ids: &[SessionId]) -> Result<Vec<SessionId>, sqlx::Error> {
    let uuids: Vec<Uuid> = ids.iter().map(|id| id.0).collect();
    let rows = sqlx::query(const_format_valid::ANY_ID)
        .bind(uuids)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| row.try_get(0).map(SessionId))
        .collect()
}

/// The SQL of the lookups: one set of validity conditions, joined at compile time (no value is
/// ever formatted in).
mod const_format_valid {
    macro_rules! lookup {
        ($by:literal) => {
            concat!(
                "SELECT s.id, s.user_id, u.role FROM sessions s JOIN users u ON u.id = s.user_id \
                 WHERE ",
                $by,
                " AND u.banned_at IS NULL AND s.expires_at > now() \
                 AND s.last_seen_at > now() - interval '7 days'"
            )
        };
    }
    pub const BY_HASH: &str = lookup!("s.token_hash = $1");
    pub const BY_ID: &str = lookup!("s.id = $1");
    pub const ANY_ID: &str = lookup!("s.id = ANY($1)");
}

async fn finish(
    pool: &PgPool,
    row: Option<sqlx::postgres::PgRow>,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let Some(row) = row else {
        return Ok(None);
    };
    let info = SessionInfo {
        id: SessionId(row.try_get(0)?),
        user_id: row.try_get(1)?,
        role: role_from_db(row.try_get::<&str, _>(2)?),
    };
    sqlx::query(
        "UPDATE sessions SET last_seen_at = now() \
         WHERE id = $1 AND last_seen_at < now() - interval '5 minutes'",
    )
    .bind(info.id.0)
    .execute(pool)
    .await?;
    Ok(Some(info))
}

/// Which sessions [`revoke`] deletes.
#[derive(Debug, Clone, Copy)]
pub enum RevokeFilter {
    One(SessionId),
    AllOfUser(Uuid),
    Token(TokenHash),
    /// Past `expires_at` or idle for 7 days (the retention job).
    Expired,
}

/// The only way to delete sessions (ADR-0008 D3): deletes and notifies each id on
/// [`REVOKED_CHANNEL`], both in `tx`. A rolled-back transaction deletes and notifies nothing.
pub async fn revoke(
    tx: &mut PgConnection,
    filter: RevokeFilter,
) -> Result<Vec<SessionId>, sqlx::Error> {
    let rows = match filter {
        RevokeFilter::One(id) => {
            sqlx::query("DELETE FROM sessions WHERE id = $1 RETURNING id")
                .bind(id.0)
                .fetch_all(&mut *tx)
                .await?
        }
        RevokeFilter::AllOfUser(user) => {
            sqlx::query("DELETE FROM sessions WHERE user_id = $1 RETURNING id")
                .bind(user)
                .fetch_all(&mut *tx)
                .await?
        }
        RevokeFilter::Token(hash) => {
            sqlx::query("DELETE FROM sessions WHERE token_hash = $1 RETURNING id")
                .bind(hash.0.as_slice())
                .fetch_all(&mut *tx)
                .await?
        }
        RevokeFilter::Expired => {
            sqlx::query(
                "DELETE FROM sessions WHERE expires_at <= now() \
                 OR last_seen_at <= now() - interval '7 days' RETURNING id",
            )
            .fetch_all(&mut *tx)
            .await?
        }
    };
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get(0))
        .collect::<Result<_, _>>()?;
    if !ids.is_empty() {
        sqlx::query("SELECT pg_notify($1, id::text) FROM unnest($2::uuid[]) AS id")
            .bind(REVOKED_CHANNEL)
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
    }
    Ok(ids.into_iter().map(SessionId).collect())
}

#[cfg(test)]
mod tests;
