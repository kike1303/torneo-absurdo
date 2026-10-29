//! 008 RF-B3: 20 first logins of the same identity at the same time make one user, one grant
//! and 20 valid sessions. Its own test binary, so its connections do not compete with other
//! tests for the 20 of the development Postgres (`infra/compose.dev.yml`): the pool is warmed to
//! 16 connections and a barrier releases every login at once, on several threads, so the
//! logins really overlap and the losers take the "retry as an existing login" path.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use protocol::Provider;
use server::app::{AppState, Settings};
use server::auth::login::{self, Identity};
use server::sessions::{self, UaFamily};
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use support::app::AlwaysUp;
use support::db::TestDb;
use tokio::sync::Barrier;

const LOGINS: usize = 20;
const CONNECTIONS: u32 = 16;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_first_logins_make_one_user_one_grant_and_twenty_sessions() {
    let db = TestDb::create().await;
    drop(db.migrated_pool().await);
    let pool = PgPoolOptions::new()
        .max_connections(CONNECTIONS)
        .min_connections(CONNECTIONS)
        .connect_with(db.options())
        .await
        .unwrap();
    // Warm: every connection open before the race.
    let mut warm = Vec::new();
    for _ in 0..CONNECTIONS {
        warm.push(pool.acquire().await.unwrap());
    }
    drop(warm);
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let state = AppState::from_config(
        &config,
        &Settings::default(),
        pool.clone(),
        Arc::new(AlwaysUp),
    );
    let identity = Identity {
        provider: Provider::Dev,
        subject: "ana".to_owned(),
    };
    let barrier = Arc::new(Barrier::new(LOGINS));
    let logins = (0..LOGINS).map(|_| {
        let state = state.clone();
        let identity = identity.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            login::login(
                &state.auth,
                &state.db,
                &identity,
                [1; 32],
                UaFamily::Other,
                None,
            )
            .await
        })
    });
    let results = futures_util::future::join_all(logins).await;
    let tokens: Vec<_> = results
        .into_iter()
        .map(|r| r.unwrap().unwrap().token)
        .collect();
    let count = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get::<i64, _>(0)
        }
    };
    assert_eq!(count("SELECT count(*) FROM users").await, 1);
    assert_eq!(count("SELECT count(*) FROM ledger_entries").await, 1);
    assert_eq!(count("SELECT count(*) FROM oauth_accounts").await, 1);
    assert_eq!(count("SELECT count(*) FROM sessions").await, LOGINS as i64);
    for token in &tokens {
        assert!(
            sessions::validate(&pool, &token.hash())
                .await
                .unwrap()
                .is_some()
        );
    }
    let user: uuid::Uuid = sqlx::query("SELECT id FROM users")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(server::ledger::balance(&pool, user).await.unwrap().0, 100);
}
