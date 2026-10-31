//! 008 RF-B8, RF-B10 (T11, part of T10): session validity, the 5-minute touch and the single
//! revoke path with its NOTIFY. 009 RF2, RF6, RF7, RF10 (plan decision 2): what a session
//! authenticated with, `step_up`, `recent_admin`, the copy of a link and `lock_auth`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use server::sessions::{
    self, Authentication, OauthAccountId, RevokeFilter, SessionId, SessionToken, UaFamily,
};
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Row};
use support::db::TestDb;
use uuid::Uuid;

async fn user(pool: &PgPool, role: &str) -> Uuid {
    let id = Uuid::new_v4();
    // `role` only through the helper of the admin CLI's actor (users_privileged_guard).
    support::db::privileged(
        pool,
        sqlx::query(
            "INSERT INTO users (id, display_name, display_name_key, role) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(format!("Test{}", id.simple()))
        .bind(format!("test{}", id.simple()))
        .bind(role),
    )
    .await;
    id
}

async fn issue(
    pool: &PgPool,
    user: Uuid,
    role: protocol::Role,
) -> (sessions::SessionId, SessionToken) {
    let mut tx = pool.begin().await.unwrap();
    let issued = sessions::issue(
        &mut tx,
        user,
        role,
        Authentication::None,
        &[9; 32],
        UaFamily::Firefox,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (issued.id, issued.token)
}

/// Runs `sql` with the session id; through the helper, since one of them sets `banned_at`.
async fn set(pool: &PgPool, sql: &'static str, id: sessions::SessionId) {
    support::db::privileged(pool, sqlx::query(sql).bind(id.uuid())).await;
}

#[tokio::test]
async fn a_fresh_session_is_valid_and_stores_only_the_hash() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let (id, token) = issue(&pool, a, protocol::Role::User).await;
    let info = sessions::validate(&pool, &token.hash())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.user_id, a);
    assert_eq!(info.id, id);
    let row = sqlx::query("SELECT token_hash, user_agent_family, ip_hash FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Vec<u8>, _>(0), token.hash().0.to_vec());
    assert_eq!(row.get::<String, _>(1), "firefox");
    assert_eq!(row.get::<Vec<u8>, _>(2), vec![9; 32]);
    assert!(
        sessions::validate(&pool, &sessions::TokenHash::of("other"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn inactivity_absolute_expiry_and_ban_end_a_session() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    for sql in [
        "UPDATE sessions SET last_seen_at = now() - interval '7 days 1 second' WHERE id = $1",
        "UPDATE sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
        "UPDATE users SET banned_at = now() WHERE id = (SELECT user_id FROM sessions WHERE id = $1)",
    ] {
        let a = user(&pool, "user").await;
        let (id, token) = issue(&pool, a, protocol::Role::User).await;
        set(&pool, sql, id).await;
        assert!(
            sessions::validate(&pool, &token.hash())
                .await
                .unwrap()
                .is_none(),
            "{sql}"
        );
        assert!(
            sessions::validate_by_id(&pool, id).await.unwrap().is_none(),
            "{sql}"
        );
    }
    // Just inside both limits: still valid.
    let a = user(&pool, "user").await;
    let (id, token) = issue(&pool, a, protocol::Role::User).await;
    set(
        &pool,
        "UPDATE sessions SET last_seen_at = now() - interval '6 days 23 hours' WHERE id = $1",
        id,
    )
    .await;
    assert!(
        sessions::validate(&pool, &token.hash())
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn an_admin_session_lasts_twelve_hours() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = user(&pool, "admin").await;
    issue(&pool, admin, protocol::Role::Admin).await;
    let hours: f64 = sqlx::query(
        "SELECT EXTRACT(EPOCH FROM expires_at - created_at)::float8 / 3600 FROM sessions",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert!((hours - 12.0).abs() < 0.01, "{hours}");
}

#[tokio::test]
async fn last_seen_at_is_touched_at_most_every_five_minutes() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let (id, token) = issue(&pool, a, protocol::Role::User).await;
    let age = |pool: PgPool| async move {
        sqlx::query("SELECT EXTRACT(EPOCH FROM now() - last_seen_at)::float8 FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<f64, _>(0)
    };
    set(
        &pool,
        "UPDATE sessions SET last_seen_at = now() - interval '4 minutes' WHERE id = $1",
        id,
    )
    .await;
    sessions::validate(&pool, &token.hash())
        .await
        .unwrap()
        .unwrap();
    assert!(age(pool.clone()).await >= 239.0, "touched within 5 minutes");
    set(
        &pool,
        "UPDATE sessions SET last_seen_at = now() - interval '6 minutes' WHERE id = $1",
        id,
    )
    .await;
    sessions::validate(&pool, &token.hash())
        .await
        .unwrap()
        .unwrap();
    assert!(age(pool.clone()).await < 5.0, "not touched after 5 minutes");
}

async fn listener(pool: &PgPool) -> PgListener {
    let mut listener = PgListener::connect_with(pool).await.unwrap();
    listener.listen(sessions::REVOKED_CHANNEL).await.unwrap();
    listener
}

async fn next_id(listener: &mut PgListener) -> String {
    tokio::time::timeout(Duration::from_secs(5), listener.recv())
        .await
        .unwrap()
        .unwrap()
        .payload()
        .to_owned()
}

#[tokio::test]
async fn revoke_deletes_and_notifies_every_id_in_the_same_transaction() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let mut notes = listener(&pool).await;
    let a = user(&pool, "user").await;
    let b = user(&pool, "user").await;
    let (a1, token_a1) = issue(&pool, a, protocol::Role::User).await;
    let (a2, _) = issue(&pool, a, protocol::Role::User).await;
    let (b1, _) = issue(&pool, b, protocol::Role::User).await;
    let (b2, token_b2) = issue(&pool, b, protocol::Role::User).await;

    // Rolled back: nothing is deleted and nothing is notified.
    let mut tx = pool.begin().await.unwrap();
    sessions::revoke(&mut tx, RevokeFilter::AllOfUser(a))
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        sessions::revoke(&mut tx, RevokeFilter::One(a1))
            .await
            .unwrap(),
        [a1]
    );
    tx.commit().await.unwrap();
    assert_eq!(next_id(&mut notes).await, a1.uuid().to_string());
    assert!(
        sessions::validate(&pool, &token_a1.hash())
            .await
            .unwrap()
            .is_none()
    );

    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        sessions::revoke(&mut tx, RevokeFilter::Token(token_b2.hash()))
            .await
            .unwrap(),
        [b2]
    );
    let mut all = sessions::revoke(&mut tx, RevokeFilter::AllOfUser(a))
        .await
        .unwrap();
    all.extend(
        sessions::revoke(&mut tx, RevokeFilter::AllOfUser(b))
            .await
            .unwrap(),
    );
    tx.commit().await.unwrap();
    assert_eq!(all.len(), 2);
    let mut seen = vec![
        next_id(&mut notes).await,
        next_id(&mut notes).await,
        next_id(&mut notes).await,
    ];
    seen.sort();
    let mut expected = vec![
        b2.uuid().to_string(),
        a2.uuid().to_string(),
        b1.uuid().to_string(),
    ];
    expected.sort();
    assert_eq!(seen, expected);
    let left: i64 = sqlx::query("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(left, 0);
}

#[tokio::test]
async fn revoke_expired_takes_only_dead_sessions() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let (live, _) = issue(&pool, a, protocol::Role::User).await;
    let (old, _) = issue(&pool, a, protocol::Role::User).await;
    let (idle, _) = issue(&pool, a, protocol::Role::User).await;
    set(
        &pool,
        "UPDATE sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
        old,
    )
    .await;
    set(
        &pool,
        "UPDATE sessions SET last_seen_at = now() - interval '8 days' WHERE id = $1",
        idle,
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    let mut gone = sessions::revoke(&mut tx, RevokeFilter::Expired)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    gone.sort_by_key(|id| id.uuid());
    let mut expected = vec![old, idle];
    expected.sort_by_key(|id| id.uuid());
    assert_eq!(gone, expected);
    assert!(
        sessions::validate_by_id(&pool, live)
            .await
            .unwrap()
            .is_some()
    );
}

/// 008 RF-C3 (plan-c decision 6): every other session of the user, each notified, and never the
/// one to keep nor another user's.
#[tokio::test]
async fn revoke_others_of_user_keeps_the_given_session() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let mut notes = listener(&pool).await;
    let a = user(&pool, "user").await;
    let b = user(&pool, "user").await;
    let (keep, keep_token) = issue(&pool, a, protocol::Role::User).await;
    let (a2, _) = issue(&pool, a, protocol::Role::User).await;
    let (a3, _) = issue(&pool, a, protocol::Role::User).await;
    let (b1, _) = issue(&pool, b, protocol::Role::User).await;
    let mut tx = pool.begin().await.unwrap();
    let mut gone = sessions::revoke(&mut tx, RevokeFilter::OthersOfUser { user: a, keep })
        .await
        .unwrap();
    tx.commit().await.unwrap();
    gone.sort_by_key(|id| id.uuid());
    let mut expected = vec![a2, a3];
    expected.sort_by_key(|id| id.uuid());
    assert_eq!(gone, expected);
    let mut seen = vec![next_id(&mut notes).await, next_id(&mut notes).await];
    seen.sort();
    let mut expected: Vec<String> = expected.iter().map(|id| id.uuid().to_string()).collect();
    expected.sort();
    assert_eq!(seen, expected);
    let info = sessions::validate(&pool, &keep_token.hash())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.id, keep);
    assert!(sessions::validate_by_id(&pool, b1).await.unwrap().is_some());
}

// --- 009: what a session authenticated with -----------------------------------------------

/// An account of `user` (no guard on an insert of `oauth_accounts`).
async fn account(pool: &PgPool, user: Uuid, provider: &str) -> OauthAccountId {
    let id: i64 = sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ($1, $2, $3) \
         RETURNING id",
    )
    .bind(provider)
    .bind(format!("{provider}-{}", Uuid::new_v4().simple()))
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap()
    .get(0);
    OauthAccountId::new(id)
}

async fn issue_with(
    pool: &PgPool,
    user: Uuid,
    role: protocol::Role,
    authentication: Authentication,
) -> (SessionId, SessionToken, u64) {
    let mut tx = pool.begin().await.unwrap();
    let issued = sessions::issue(
        &mut tx,
        user,
        role,
        authentication,
        &[9; 32],
        UaFamily::Other,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (issued.id, issued.token, issued.max_age_secs)
}

async fn fresh(
    pool: &PgPool,
    user: Uuid,
    role: protocol::Role,
    account: OauthAccountId,
) -> (SessionId, SessionToken) {
    let (id, token, _) = issue_with(pool, user, role, Authentication::Fresh(account)).await;
    (id, token)
}

/// `authenticated_at` set `minutes` before now.
async fn authenticated_ago(pool: &PgPool, id: SessionId, minutes: i32) {
    sqlx::query(
        "UPDATE sessions SET authenticated_at = now() - make_interval(mins => $2) WHERE id = $1",
    )
    .bind(id.uuid())
    .bind(minutes)
    .execute(pool)
    .await
    .unwrap();
}

async fn info(pool: &PgPool, token: &SessionToken) -> sessions::SessionInfo {
    sessions::validate(pool, &token.hash())
        .await
        .unwrap()
        .unwrap()
}

/// `(step_up, recent_admin)` by every lookup (by hash and by id): the same SQL.
async fn flags(pool: &PgPool, id: SessionId, token: &SessionToken) -> (bool, bool) {
    let by_hash = info(pool, token).await;
    let by_id = sessions::validate_by_id(pool, id).await.unwrap().unwrap();
    assert_eq!(
        (by_hash.step_up, by_hash.recent_admin),
        (by_id.step_up, by_id.recent_admin)
    );
    (by_hash.step_up, by_hash.recent_admin)
}

async fn set_cutoff(pool: &PgPool, user: Uuid, cutoff: Option<OauthAccountId>) {
    support::db::privileged(
        pool,
        sqlx::query("UPDATE users SET admin_account_cutoff = $2 WHERE id = $1")
            .bind(user)
            .bind(cutoff.map(OauthAccountId::get)),
    )
    .await;
}

/// 009 Definitions: a confirmation is a current authentication of the last 10 minutes.
#[tokio::test]
async fn step_up_is_a_current_authentication_of_the_last_ten_minutes() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let google = account(&pool, a, "google").await;
    let (id, token) = fresh(&pool, a, protocol::Role::User, google).await;
    assert_eq!(flags(&pool, id, &token).await, (true, false));
    authenticated_ago(&pool, id, 9).await;
    assert!(flags(&pool, id, &token).await.0);
    authenticated_ago(&pool, id, 11).await;
    assert!(!flags(&pool, id, &token).await.0);
}

/// 009 Definitions: current means the identity is still linked, is the user's, and nothing was
/// linked after. Unlinking another identity does not end it (the maximum only goes down).
#[tokio::test]
async fn step_up_needs_the_identity_linked_and_nothing_linked_after() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let b = user(&pool, "user").await;
    let google = account(&pool, a, "google").await;
    // No authentication at all.
    let (id, token, _) = issue_with(&pool, a, protocol::Role::User, Authentication::None).await;
    assert_eq!(flags(&pool, id, &token).await, (false, false));
    // An account of another user, with a sequence above everything.
    let of_b = account(&pool, b, "google").await;
    let (id, token) = fresh(&pool, a, protocol::Role::User, google).await;
    sqlx::query("UPDATE sessions SET auth_account_id = $2, auth_link_seq = 1000 WHERE id = $1")
        .bind(id.uuid())
        .bind(of_b.get())
        .execute(&pool)
        .await
        .unwrap();
    assert!(!flags(&pool, id, &token).await.0);
    // Something linked after the authentication.
    let (id, token) = fresh(&pool, a, protocol::Role::User, google).await;
    assert!(flags(&pool, id, &token).await.0);
    let discord = account(&pool, a, "discord").await;
    assert!(!flags(&pool, id, &token).await.0);
    // Authenticated with Google after Discord was linked: current. Unlinking Discord lowers the
    // maximum and keeps it current.
    let (id, token) = fresh(&pool, a, protocol::Role::User, google).await;
    assert!(flags(&pool, id, &token).await.0);
    sqlx::query("DELETE FROM oauth_accounts WHERE id = $1")
        .bind(discord.get())
        .execute(&pool)
        .await
        .unwrap();
    assert!(flags(&pool, id, &token).await.0);
    // Unlinking its own identity ends it (the FK leaves it NULL).
    sqlx::query("DELETE FROM oauth_accounts WHERE id = $1")
        .bind(google.get())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(flags(&pool, id, &token).await, (false, false));
}

/// 009 RF7: `recent_admin` is an admin, a current authentication of the last 60 minutes and an
/// account inside the cutoff of the last grant (a NULL cutoff is "no").
#[tokio::test]
async fn recent_admin_needs_the_window_and_the_cutoff() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = user(&pool, "admin").await;
    let google = account(&pool, admin, "google").await;
    let (id, token) = fresh(&pool, admin, protocol::Role::Admin, google).await;
    assert_eq!(flags(&pool, id, &token).await, (true, false));
    set_cutoff(&pool, admin, Some(google)).await;
    assert_eq!(flags(&pool, id, &token).await, (true, true));
    authenticated_ago(&pool, id, 59).await;
    assert_eq!(flags(&pool, id, &token).await, (false, true));
    authenticated_ago(&pool, id, 61).await;
    assert_eq!(flags(&pool, id, &token).await, (false, false));
    // An account linked after the grant: current, recent, but outside the cutoff.
    let discord = account(&pool, admin, "discord").await;
    let (id, token) = fresh(&pool, admin, protocol::Role::Admin, discord).await;
    assert_eq!(flags(&pool, id, &token).await, (true, false));
    // A grant after it brings it in.
    set_cutoff(&pool, admin, Some(discord)).await;
    assert_eq!(flags(&pool, id, &token).await, (true, true));
    set_cutoff(&pool, admin, None).await;
    assert_eq!(flags(&pool, id, &token).await, (true, false));
    // Never for a user, cutoff or not.
    let plain = user(&pool, "user").await;
    let own = account(&pool, plain, "google").await;
    set_cutoff(&pool, plain, Some(own)).await;
    let (id, token) = fresh(&pool, plain, protocol::Role::User, own).await;
    assert_eq!(flags(&pool, id, &token).await, (true, false));
}

/// RF2 (question 6): a session of before 009 has both columns NULL. Both booleans are false,
/// never a NULL that the lookup would read as a database error (a 503 instead of a 403); also
/// in the two cases where the SQL of the rule does give NULL.
#[tokio::test]
async fn a_session_without_an_authentication_is_false_without_an_error() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = user(&pool, "admin").await;
    let google = account(&pool, admin, "google").await;
    set_cutoff(&pool, admin, Some(google)).await;
    let (id, token, _) =
        issue_with(&pool, admin, protocol::Role::Admin, Authentication::None).await;
    let row = sqlx::query(
        "SELECT auth_account_id IS NULL AND auth_link_seq IS NULL FROM sessions WHERE id = $1",
    )
    .bind(id.uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.get::<bool, _>(0));
    assert_eq!(flags(&pool, id, &token).await, (false, false));
    let mut tx = pool.begin().await.unwrap();
    let locked = sessions::lock_auth(&mut tx, id).await.unwrap().unwrap();
    assert!(!locked.step_up && !locked.link_window && locked.account.is_none());
    drop(tx);
    // Where SQL really yields NULL: an account of the authentication (another user's) while the
    // user has no account at all (`bool_or` and `max` over nothing), and a current admin
    // authentication with a NULL cutoff (`<= NULL`). Both are "no", without an error.
    let lonely = user(&pool, "admin").await;
    let (id, token, _) =
        issue_with(&pool, lonely, protocol::Role::Admin, Authentication::None).await;
    sqlx::query("UPDATE sessions SET auth_account_id = $2, auth_link_seq = 1000 WHERE id = $1")
        .bind(id.uuid())
        .bind(google.get())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(flags(&pool, id, &token).await, (false, false));
    let mut tx = pool.begin().await.unwrap();
    let locked = sessions::lock_auth(&mut tx, id).await.unwrap().unwrap();
    assert!(!locked.step_up && !locked.link_window);
    drop(tx);
    set_cutoff(&pool, admin, None).await;
    let (id, token) = fresh(&pool, admin, protocol::Role::Admin, google).await;
    assert_eq!(flags(&pool, id, &token).await, (true, false));
}

/// RF6: a fresh session keeps its account and the `max(id)` of the user's accounts; RF10: a kept
/// one copies the four values of the old, and its `Max-Age` is what is left (minimum 1).
#[tokio::test]
async fn fresh_keeps_the_max_id_and_kept_from_copies_the_authentication() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let google = account(&pool, a, "google").await;
    let discord = account(&pool, a, "discord").await;
    let (old, _, max_age) = issue_with(
        &pool,
        a,
        protocol::Role::User,
        Authentication::Fresh(google),
    )
    .await;
    assert_eq!(max_age, 30 * 24 * 3600);
    let row = sqlx::query("SELECT auth_account_id, auth_link_seq FROM sessions WHERE id = $1")
        .bind(old.uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<i64>, _>(0), Some(google.get()));
    assert_eq!(row.get::<Option<i64>, _>(1), Some(discord.get()));

    sqlx::query(
        "UPDATE sessions SET expires_at = now() + interval '1 hour', \
         authenticated_at = now() - interval '5 minutes', auth_link_seq = auth_link_seq + 7 \
         WHERE id = $1",
    )
    .bind(old.uuid())
    .execute(&pool)
    .await
    .unwrap();
    let (new, _, max_age) = issue_with(
        &pool,
        a,
        protocol::Role::User,
        Authentication::KeptFrom(old),
    )
    .await;
    assert!((3598..=3600).contains(&max_age), "{max_age}");
    let same: bool = sqlx::query(
        "SELECT o.expires_at = n.expires_at AND o.authenticated_at = n.authenticated_at \
         AND o.auth_account_id = n.auth_account_id AND o.auth_link_seq = n.auth_link_seq \
         FROM sessions o, sessions n WHERE o.id = $1 AND n.id = $2",
    )
    .bind(old.uuid())
    .bind(new.uuid())
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert!(same);

    sqlx::query(
        "UPDATE sessions SET expires_at = now() + interval '300 milliseconds' WHERE id = $1",
    )
    .bind(old.uuid())
    .execute(&pool)
    .await
    .unwrap();
    let (_, _, max_age) = issue_with(
        &pool,
        a,
        protocol::Role::User,
        Authentication::KeptFrom(old),
    )
    .await;
    assert_eq!(max_age, 1);
    // An admin's fresh session is 12 hours.
    let (_, _, max_age) = issue_with(
        &pool,
        a,
        protocol::Role::Admin,
        Authentication::Fresh(google),
    )
    .await;
    assert_eq!(max_age, 12 * 3600);
}

/// RF9, RF14: `lock_auth` reads the authentication of a valid session only, with the 20-minute
/// window of a link callback beside the 10 of a confirmation.
#[tokio::test]
async fn lock_auth_reads_only_a_valid_session() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool, "user").await;
    let google = account(&pool, a, "google").await;
    let (id, _) = fresh(&pool, a, protocol::Role::User, google).await;
    let lock = |id| {
        let pool = pool.clone();
        async move {
            let mut tx = pool.begin().await.unwrap();
            sessions::lock_auth(&mut tx, id).await.unwrap()
        }
    };
    let locked = lock(id).await.unwrap();
    assert_eq!(locked.user_id, a);
    assert!(locked.step_up && locked.link_window);
    assert_eq!(locked.account, Some(google));
    authenticated_ago(&pool, id, 15).await;
    let locked = lock(id).await.unwrap();
    assert!(!locked.step_up && locked.link_window);
    authenticated_ago(&pool, id, 21).await;
    let locked = lock(id).await.unwrap();
    assert!(!locked.step_up && !locked.link_window);
    // Expired, idle, banned or revoked: no row.
    for sql in [
        "UPDATE sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
        "UPDATE sessions SET last_seen_at = now() - interval '8 days' WHERE id = $1",
        "UPDATE users SET banned_at = now() WHERE id = (SELECT user_id FROM sessions WHERE id = $1)",
    ] {
        let b = user(&pool, "user").await;
        let own = account(&pool, b, "google").await;
        let (id, _) = fresh(&pool, b, protocol::Role::User, own).await;
        set(&pool, sql, id).await;
        assert!(lock(id).await.is_none(), "{sql}");
    }
    let mut tx = pool.begin().await.unwrap();
    sessions::revoke(&mut tx, RevokeFilter::One(id))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(lock(id).await.is_none());
}
