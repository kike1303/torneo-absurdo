//! 008 RF-B8, RF-B10 (T11, part of T10): session validity, the 5-minute touch and the single
//! revoke path with its NOTIFY.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use server::sessions::{self, RevokeFilter, SessionToken, UaFamily};
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Row};
use support::db::TestDb;
use uuid::Uuid;

async fn user(pool: &PgPool, role: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, display_name, display_name_key, role) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(format!("Test{}", id.simple()))
    .bind(format!("test{}", id.simple()))
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn issue(
    pool: &PgPool,
    user: Uuid,
    role: protocol::Role,
) -> (sessions::SessionId, SessionToken) {
    let mut tx = pool.begin().await.unwrap();
    let issued = sessions::issue(&mut tx, user, role, &[9; 32], UaFamily::Firefox)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (issued.id, issued.token)
}

async fn set(pool: &PgPool, sql: &'static str, id: sessions::SessionId) {
    sqlx::query(sql)
        .bind(id.uuid())
        .execute(pool)
        .await
        .unwrap();
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
