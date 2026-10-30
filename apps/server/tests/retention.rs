//! 008 RF-B16: the daily retention pass (and the one at startup) deletes auth events older
//! than 30 days, expired OAuth flows and expired sessions (these through sessions::revoke, so
//! their sockets close).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use server::sessions::REVOKED_CHANNEL;
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Row};
use support::db::TestDb;
use tokio_util::sync::CancellationToken;

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
}

async fn seed(pool: &PgPool) -> uuid::Uuid {
    let (user, _live) = support::app::user_with_session(pool).await;
    // An expired session and one idle for 8 days.
    support::app::session_for(pool, user).await;
    support::app::session_for(pool, user).await;
    sqlx::query(
        "UPDATE sessions SET expires_at = now() - interval '1 second' \
         WHERE id = (SELECT id FROM sessions ORDER BY created_at, id OFFSET 1 LIMIT 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE sessions SET last_seen_at = now() - interval '8 days' \
         WHERE id = (SELECT id FROM sessions WHERE expires_at > now() ORDER BY created_at, id OFFSET 1 LIMIT 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    // auth_events has no immutability trigger: the old one is inserted with its date.
    for age in ["31 days", "29 days"] {
        sqlx::query(
            "INSERT INTO auth_events (user_id, kind, ip_hash, created_at) \
             VALUES ($1, 'login', '\\x00', now() - $2::interval)",
        )
        .bind(user)
        .bind(age)
        .execute(pool)
        .await
        .unwrap();
    }
    for expires in ["-1 second", "5 minutes"] {
        sqlx::query(
            "INSERT INTO oauth_flows (id_hash, provider, purpose, state, code_verifier, next, expires_at) \
             VALUES (gen_random_bytes_compat(), 'dev', 'login', 's', 'v', '/', now() + $1::interval)",
        )
        .bind(expires)
        .execute(pool)
        .await
        .unwrap();
    }
    user
}

async fn assert_retained(pool: &PgPool) {
    assert_eq!(count(pool, "SELECT count(*) FROM sessions").await, 1);
    assert_eq!(count(pool, "SELECT count(*) FROM auth_events").await, 1);
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM auth_events WHERE created_at > now() - interval '30 days'"
        )
        .await,
        1
    );
    assert_eq!(count(pool, "SELECT count(*) FROM oauth_flows").await, 1);
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM oauth_flows WHERE expires_at > now()"
        )
        .await,
        1
    );
}

async fn helper(pool: &PgPool) {
    sqlx::query(
        "CREATE FUNCTION gen_random_bytes_compat() RETURNS bytea LANGUAGE sql AS \
         $$ SELECT decode(md5(random()::text), 'hex') $$",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_pass_deletes_only_what_is_past_retention_and_notifies_the_sessions() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    helper(&pool).await;
    seed(&pool).await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen(REVOKED_CHANNEL).await.unwrap();
    server::jobs::retention_pass(&pool).await.unwrap();
    assert_retained(&pool).await;
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(5), listener.recv())
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn the_job_runs_a_pass_at_startup_and_stops_when_cancelled() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    helper(&pool).await;
    seed(&pool).await;
    let stop = CancellationToken::new();
    let task = tokio::spawn(server::jobs::run(pool.clone(), stop.clone()));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while count(&pool, "SELECT count(*) FROM sessions").await != 1
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_retained(&pool).await;
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}
