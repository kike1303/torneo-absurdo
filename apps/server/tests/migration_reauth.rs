//! 009 RF1 to RF5 (plan decision 1, step 1): the migration of TA-29 over data with the shape of
//! 008. The rows that exist are numbered by `(created_at, provider, provider_user_id)`, never in
//! the order they were inserted, the sequence goes on above them, the sessions of before keep no
//! authentication, and the new constraints hold.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::db::TestDb;
use uuid::Uuid;

/// The last migration of 008.
const LAST_OF_008: i64 = 20261029100000;

fn constraint(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.constraint())
        .map(str::to_owned)
}

async fn user(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, display_name, display_name_key) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Test{}", id.simple()))
        .bind(format!("test{}", id.simple()))
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn account(pool: &PgPool, user: Uuid, provider: &str, subject: &str, created_at: &str) {
    sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_user_id, user_id, created_at) \
         VALUES ($1, $2, $3, $4::timestamptz)",
    )
    .bind(provider)
    .bind(subject)
    .bind(user)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn id_of(pool: &PgPool, provider: &str, subject: &str) -> i64 {
    sqlx::query("SELECT id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2")
        .bind(provider)
        .bind(subject)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn the_migration_numbers_the_accounts_of_008_and_adds_the_constraints() {
    let db = TestDb::create().await;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(db.options())
        .await
        .unwrap();
    sqlx::migrate!("./migrations")
        .run_to(LAST_OF_008, &pool)
        .await
        .unwrap();

    // Data with the shape of 008, inserted out of the order the migration must give them.
    let a = user(&pool).await;
    let b = user(&pool).await;
    account(&pool, a, "google", "g-last", "2026-01-02 00:00:00+00").await;
    account(&pool, a, "discord", "d-z", "2026-01-01 00:00:00+00").await;
    account(&pool, b, "google", "g-same-day", "2026-01-01 00:00:00+00").await;
    // Same day and same provider as `d-z`, of another user: the subject breaks the tie.
    account(&pool, b, "discord", "d-a", "2026-01-01 00:00:00+00").await;
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, ip_hash) \
         VALUES ('\\x01', $1, now() + interval '1 day', '\\x00')",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();

    server::db::migrate(&pool).await.unwrap();

    assert_eq!(id_of(&pool, "discord", "d-a").await, 1);
    assert_eq!(id_of(&pool, "discord", "d-z").await, 2);
    assert_eq!(id_of(&pool, "google", "g-same-day").await, 3);
    assert_eq!(id_of(&pool, "google", "g-last").await, 4);
    // The sequence goes on above the maximum.
    let c = user(&pool).await;
    account(&pool, c, "google", "g-next", "2025-01-01 00:00:00+00").await;
    assert_eq!(id_of(&pool, "google", "g-next").await, 5);
    // A session of before has no authentication.
    let row = sqlx::query(
        "SELECT auth_account_id IS NULL, auth_link_seq IS NULL FROM sessions WHERE user_id = $1",
    )
    .bind(a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.get::<bool, _>(0));
    assert!(row.get::<bool, _>(1));

    // RF1: the id is never written by hand.
    let error = sqlx::query(
        "INSERT INTO oauth_accounts (id, provider, provider_user_id, user_id) \
         VALUES (99, 'dev', 'x', $1)",
    )
    .bind(c)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(error.as_database_error().is_some(), "{error:?}");
    assert_eq!(
        sqlx::query("SELECT count(*) FROM oauth_accounts WHERE id = 99")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>(0),
        0
    );

    // RF2: an account without its link sequence is refused.
    let error = sqlx::query("UPDATE sessions SET auth_account_id = 4 WHERE user_id = $1")
        .bind(a)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(constraint(&error).as_deref(), Some("sessions_auth_ck"));
    // Deleting the account of the authentication leaves it NULL and keeps the sequence.
    sqlx::query("UPDATE sessions SET auth_account_id = 4, auth_link_seq = 4 WHERE user_id = $1")
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM oauth_accounts WHERE id = 4")
        .execute(&pool)
        .await
        .unwrap();
    let row = sqlx::query("SELECT auth_account_id, auth_link_seq FROM sessions WHERE user_id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<i64>, _>(0), None);
    assert_eq!(row.get::<Option<i64>, _>(1), Some(4));

    // RF4: a `reauth` flow carries its user, and only with one.
    let flow = |purpose: &'static str, user: Option<Uuid>, hash: &'static [u8]| {
        sqlx::query(
            "INSERT INTO oauth_flows (id_hash, provider, purpose, link_user_id, state, \
             code_verifier, next, expires_at) \
             VALUES ($1, 'google', $2, $3, 's', 'v', '/', now() + interval '10 minutes')",
        )
        .bind(hash)
        .bind(purpose)
        .bind(user)
    };
    flow("reauth", Some(a), b"one".as_slice())
        .execute(&pool)
        .await
        .unwrap();
    let error = flow("reauth", None, b"two".as_slice())
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(constraint(&error).as_deref(), Some("oauth_flows_link_ck"));
    let error = flow("other", None, b"three".as_slice())
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        constraint(&error).as_deref(),
        Some("oauth_flows_purpose_ck")
    );

    // RF5.
    sqlx::query(
        "INSERT INTO auth_events (user_id, kind, provider, ip_hash) \
         VALUES ($1, 'reauth', 'google', '\\x00')",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    let error = sqlx::query(
        "INSERT INTO auth_events (user_id, kind, ip_hash) VALUES ($1, 'other', '\\x00')",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(constraint(&error).as_deref(), Some("auth_events_kind_ck"));
}
