//! Sessions (ADR-0008 D3, 008 RF-B8, RF-B10).
//!
//! - The token is 256 bits of `OsRng` in base64url; the database keeps only `SHA-256(token)`.
//! - Valid while not banned, before `expires_at` (30 days, 12 hours for an admin) and seen in
//!   the last 7 days. `last_seen_at` is touched at most every 5 minutes.
//! - Every time comes from Postgres' `now()`: no clock in the application.
//! - [`revoke`] is the only code that deletes sessions, and it notifies every deleted id on
//!   [`REVOKED_CHANNEL`] in the same transaction, so the sockets close at once (D6).
//! - A session knows what it authenticated with (009 RF2): `auth_account_id` (the row of
//!   `oauth_accounts` of the login or the reauthentication) and `auth_link_seq` (the `max(id)` of
//!   the user's accounts then). The rule over them lives in one fragment of SQL, `auth_state!`,
//!   used by the lookup, [`lock_auth`] and `reauth-status` (009 plan decision 2).

pub mod listener;

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::{Provider, Role};
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

/// The id of a row of `oauth_accounts`: its place in the link order (009 RF1). Never in a body,
/// a cookie or a log: no `Display`, no `Serialize`, and `Debug` hides it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OauthAccountId(i64);

impl OauthAccountId {
    /// The id as read from `oauth_accounts.id`.
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Debug for OauthAccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OauthAccountId(..)")
    }
}

/// What a new session authenticated with (009 RF6, RF10, RF12).
#[derive(Debug, Clone, Copy)]
pub enum Authentication {
    /// A login or a reauthentication: `now()`, that account and the `max(id)` of the user's
    /// accounts, read in the `INSERT`.
    Fresh(OauthAccountId),
    /// A link: copies `authenticated_at`, `expires_at`, `auth_account_id` and `auth_link_seq` of
    /// this session, which the caller holds `FOR UPDATE`. A link never counts as a
    /// reauthentication and never extends the session.
    KeptFrom(SessionId),
    /// No authentication that counts (test helpers; never `step_up` nor `recent_admin`).
    None,
}

/// The window of a confirmation (`step_up`, 009 Definitions).
pub const STEP_UP_WINDOW_MINUTES: i64 = 10;
/// The window of a link callback: the confirmation plus the 10 minutes of a flow (009 RF9).
pub const LINK_CALLBACK_WINDOW_MINUTES: i64 = 20;
/// The window of `RecentAdmin` (ADR-0008 D9, 009 RF7).
pub const RECENT_ADMIN_WINDOW_MINUTES: i64 = 60;

/// A valid session. `Debug` shows nothing that identifies it.
#[derive(Clone, Copy)]
pub struct SessionInfo {
    pub id: SessionId,
    pub user_id: Uuid,
    pub role: Role,
    /// A confirmation: the authentication is current and of the last 10 minutes (009 RF7).
    pub step_up: bool,
    /// An admin whose authentication is current, of the last 60 minutes and with an account
    /// inside the cutoff of the last `server admin grant` (009 RF7, `RecentAdmin`).
    pub recent_admin: bool,
}

impl fmt::Debug for SessionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionInfo")
            .field("role", &self.role)
            .field("step_up", &self.step_up)
            .field("recent_admin", &self.recent_admin)
            .finish_non_exhaustive()
    }
}

/// The authentication of a session, read with its row locked (009 RF9, RF14). `Debug` shows
/// only the booleans.
#[derive(Clone, Copy)]
pub struct LockedAuth {
    pub user_id: Uuid,
    /// A confirmation (current and of the last 10 minutes).
    pub step_up: bool,
    /// Current and of the last 20 minutes: what a link callback needs (RF9).
    pub link_window: bool,
    /// `auth_account_id`: `None` when it never had one, or after an unlink consumed it.
    pub account: Option<OauthAccountId>,
}

impl fmt::Debug for LockedAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockedAuth")
            .field("step_up", &self.step_up)
            .field("link_window", &self.link_window)
            .finish_non_exhaustive()
    }
}

/// What `reauth-status` reads of a session (009 RF18): its confirmation, the account of its
/// authentication and the accounts of the user, in link order. A plain read: every action reads
/// the rule again under the lock. `Debug` shows only the confirmation.
pub struct AuthView {
    pub step_up: bool,
    pub account: Option<OauthAccountId>,
    pub linked: Vec<(Provider, OauthAccountId)>,
}

impl fmt::Debug for AuthView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthView")
            .field("step_up", &self.step_up)
            .finish_non_exhaustive()
    }
}

/// A newly issued session.
#[derive(Debug)]
pub struct Issued {
    pub id: SessionId,
    pub token: SessionToken,
    /// What is left until `expires_at`, in whole seconds computed by Postgres (minimum 1): the
    /// `Max-Age` of the cookie (009 RF10).
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

/// The `RETURNING` of [`issue`]: the id and the `Max-Age`, both from Postgres.
macro_rules! issued_returning {
    () => {
        " RETURNING id, GREATEST(1, ceil(extract(epoch FROM expires_at - now())))::bigint"
    };
}

/// A new session for `user_id`, in the transaction of a login, a reauthentication or a link
/// (`authentication`). Every instant comes from Postgres: [`Authentication::KeptFrom`] copies
/// them with an `INSERT … SELECT` and never reads them out.
pub async fn issue(
    tx: &mut PgConnection,
    user_id: Uuid,
    role: Role,
    authentication: Authentication,
    ip_hash: &[u8; 32],
    ua: UaFamily,
) -> Result<Issued, sqlx::Error> {
    let token = SessionToken::generate()
        .ok_or_else(|| sqlx::Error::Protocol("the OS random source failed".to_owned()))?;
    let row = match authentication {
        Authentication::Fresh(_) | Authentication::None => {
            let account = match authentication {
                Authentication::Fresh(account) => Some(account.0),
                _ => None,
            };
            // The `max(id)` only with an account: without one both stay NULL (`sessions_auth_ck`).
            sqlx::query(concat!(
                "INSERT INTO sessions (token_hash, user_id, expires_at, ip_hash, \
                 user_agent_family, auth_account_id, auth_link_seq) \
                 VALUES ($1, $2, now() + $3::bigint * interval '1 second', $4, $5, $6, \
                 (SELECT max(id) FROM oauth_accounts WHERE user_id = $2 AND $6::bigint IS NOT NULL))",
                issued_returning!()
            ))
            .bind(token.hash().0.as_slice())
            .bind(user_id)
            .bind(i64::try_from(max_age_secs(role)).unwrap_or(i64::MAX))
            .bind(ip_hash.as_slice())
            .bind(ua.as_str())
            .bind(account)
            .fetch_one(&mut *tx)
            .await?
        }
        Authentication::KeptFrom(old) => {
            sqlx::query(concat!(
                "INSERT INTO sessions (token_hash, user_id, expires_at, ip_hash, \
                 user_agent_family, authenticated_at, auth_account_id, auth_link_seq) \
                 SELECT $1, $2, o.expires_at, $3, $4, o.authenticated_at, o.auth_account_id, \
                 o.auth_link_seq FROM sessions o WHERE o.id = $5 AND o.user_id = $2",
                issued_returning!()
            ))
            .bind(token.hash().0.as_slice())
            .bind(user_id)
            .bind(ip_hash.as_slice())
            .bind(ua.as_str())
            .bind(old.0)
            .fetch_one(&mut *tx)
            .await?
        }
    };
    let max_age: i64 = row.try_get(1)?;
    Ok(Issued {
        id: SessionId(row.try_get(0)?),
        token,
        max_age_secs: u64::try_from(max_age).unwrap_or(1),
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

/// The authentication rule of 009, in one place (plan decision 2). Over `sessions s` joined to
/// `users u`, `join` adds `acc`: the `max(id)` of the user's accounts and whether the account of
/// the authentication is one of them. Every column is wrapped in `COALESCE(…, false)`: a session
/// of before 009 (both columns NULL) is "no", never a NULL that reads as a database error.
/// Only literals: the windows are the `*_WINDOW_MINUTES` constants, pinned by a unit test.
macro_rules! auth_state {
    (join) => {
        " LEFT JOIN LATERAL (SELECT max(a.id) AS seq, bool_or(a.id = s.auth_account_id) AS own \
         FROM oauth_accounts a WHERE a.user_id = s.user_id) acc ON true "
    };
    // Current: the identity is still linked, it is the user's, and nothing was linked after.
    (current) => {
        "s.auth_account_id IS NOT NULL AND acc.own AND s.auth_link_seq >= acc.seq"
    };
    (step_up) => {
        concat!(
            "COALESCE(",
            auth_state!(current),
            " AND s.authenticated_at > now() - interval '10 minutes', false)"
        )
    };
    (link_window) => {
        concat!(
            "COALESCE(",
            auth_state!(current),
            " AND s.authenticated_at > now() - interval '20 minutes', false)"
        )
    };
    (recent_admin) => {
        concat!(
            "COALESCE(u.role = 'admin' AND ",
            auth_state!(current),
            " AND s.authenticated_at > now() - interval '60 minutes' \
             AND s.auth_account_id <= u.admin_account_cutoff, false)"
        )
    };
}

/// The validity of a session (ban, absolute expiry, 7 days idle), the same for every read.
macro_rules! valid {
    () => {
        " AND u.banned_at IS NULL AND s.expires_at > now() \
         AND s.last_seen_at > now() - interval '7 days'"
    };
}

/// The SQL of the lookups: one set of validity conditions, joined at compile time (no value is
/// ever formatted in).
mod const_format_valid {
    macro_rules! lookup {
        ($by:literal) => {
            concat!(
                "SELECT s.id, s.user_id, u.role, ",
                auth_state!(step_up),
                ", ",
                auth_state!(recent_admin),
                " FROM sessions s JOIN users u ON u.id = s.user_id",
                auth_state!(join),
                "WHERE ",
                $by,
                valid!()
            )
        };
    }
    pub const BY_HASH: &str = lookup!("s.token_hash = $1");
    pub const BY_ID: &str = lookup!("s.id = $1");
    pub const ANY_ID: &str = lookup!("s.id = ANY($1)");

    /// [`super::lock_auth`]: the row of the session locked (`FOR UPDATE OF s`: an aggregate
    /// cannot be locked, and `users` is the caller's lock).
    pub const LOCK_AUTH: &str = concat!(
        "SELECT s.user_id, ",
        auth_state!(step_up),
        ", ",
        auth_state!(link_window),
        ", s.auth_account_id FROM sessions s JOIN users u ON u.id = s.user_id",
        auth_state!(join),
        "WHERE s.id = $1",
        valid!(),
        " FOR UPDATE OF s"
    );

    /// [`super::auth_view`]: one row per account, or a row of NULLs for a user without one.
    pub const AUTH_VIEW: &str = concat!(
        "SELECT ",
        auth_state!(step_up),
        ", s.auth_account_id, a.provider, a.id FROM sessions s JOIN users u ON u.id = s.user_id",
        auth_state!(join),
        "LEFT JOIN oauth_accounts a ON a.user_id = s.user_id WHERE s.id = $1",
        valid!(),
        " ORDER BY a.id"
    );
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
        step_up: row.try_get(3)?,
        recent_admin: row.try_get(4)?,
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

/// The authentication of the session `id`, with its row locked until `tx` ends (009 RF9, RF14):
/// `None` if it is no longer valid (closed, rotated, expired, banned). The caller holds the lock
/// of the user, so every link, reauth, unlink and logout-all of that user that committed before
/// is seen here.
pub async fn lock_auth(
    tx: &mut PgConnection,
    id: SessionId,
) -> Result<Option<LockedAuth>, sqlx::Error> {
    let Some(row) = sqlx::query(const_format_valid::LOCK_AUTH)
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(LockedAuth {
        user_id: row.try_get(0)?,
        step_up: row.try_get(1)?,
        link_window: row.try_get(2)?,
        account: row.try_get::<Option<i64>, _>(3)?.map(OauthAccountId),
    }))
}

/// What `reauth-status` needs of the session `id`, or `None` if it is no longer valid.
pub async fn auth_view(pool: &PgPool, id: SessionId) -> Result<Option<AuthView>, sqlx::Error> {
    let rows = sqlx::query(const_format_valid::AUTH_VIEW)
        .bind(id.0)
        .fetch_all(pool)
        .await?;
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let mut linked = Vec::with_capacity(rows.len());
    for row in &rows {
        let provider = row.try_get::<Option<&str>, _>(2)?.and_then(Provider::parse);
        let account = row.try_get::<Option<i64>, _>(3)?.map(OauthAccountId);
        if let (Some(provider), Some(account)) = (provider, account) {
            linked.push((provider, account));
        }
    }
    Ok(Some(AuthView {
        step_up: first.try_get(0)?,
        account: first.try_get::<Option<i64>, _>(1)?.map(OauthAccountId),
        linked,
    }))
}

/// Consumes the confirmation of the session `id` (009 RF14, step 6): one unlink per
/// confirmation. An `UPDATE`, never a delete (only [`revoke`] deletes sessions).
pub async fn consume_step_up(tx: &mut PgConnection, id: SessionId) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET auth_account_id = NULL WHERE id = $1")
        .bind(id.0)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Which sessions [`revoke`] deletes.
#[derive(Debug, Clone, Copy)]
pub enum RevokeFilter {
    One(SessionId),
    AllOfUser(Uuid),
    /// Every session of `user` but `keep` (an unlink, RF-C3).
    OthersOfUser {
        user: Uuid,
        keep: SessionId,
    },
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
        RevokeFilter::OthersOfUser { user, keep } => {
            sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND id <> $2 RETURNING id")
                .bind(user)
                .bind(keep.0)
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
