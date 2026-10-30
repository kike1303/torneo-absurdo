//! 008 RF-C3 (and its amendment 2026-10-29): unlinking a provider. It needs another provider
//! that is linked **and** enabled, revokes every other session of the user (their sockets close)
//! and writes `auth_events`; two unlinks at once never leave the user without a provider.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use server::app::{AppState, Settings};
use server::sessions::TokenHash;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::app::{AlwaysUp, ORIGIN};
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use support::pg_proxy::PgProxy;
use support::ws::{self, Ending};
use tokio::sync::Barrier;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
}

async fn providers_of(pool: &PgPool, user: Uuid) -> Vec<String> {
    sqlx::query("SELECT provider FROM oauth_accounts WHERE user_id = $1 ORDER BY provider")
        .bind(user)
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect()
}

async fn user_of(pool: &PgPool, token: &str) -> Uuid {
    sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1")
        .bind(TokenHash::of(token).0.as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

fn snowflake() -> String {
    let n = u64::from_le_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    format!("{}", 100_000_000_000_000_000 + n % 900_000_000_000_000_000)
}

fn sub() -> String {
    format!("g-{}", Uuid::new_v4().simple())
}

/// A user with Google and Discord; returns the user and a session.
async fn with_two(app: &oauth::App, pool: &PgPool) -> (Uuid, String) {
    let first = oauth::session_of(&oauth::login(app, Kind::Google, &sub()).await);
    let token = oauth::session_of(&oauth::link(app, Kind::Discord, &snowflake(), &first).await);
    (user_of(pool, &token).await, token)
}

async fn unlink(
    addr: std::net::SocketAddr,
    provider: &str,
    token: &str,
    body: &str,
) -> http::Response {
    let cookie = oauth::session_cookie(token);
    http::send_body(
        addr,
        "POST",
        &format!("/api/auth/unlink/{provider}"),
        &[
            ("origin", ORIGIN),
            ("cookie", cookie.as_str()),
            ("content-type", "application/x-www-form-urlencoded"),
        ],
        body,
    )
    .await
}

async fn alive(app: &oauth::App, token: &str) -> bool {
    oauth::me(app, token).await.status == 200
}

async fn socket(app: &oauth::App, token: &str) -> ws::Client {
    let cookie = oauth::session_cookie(token);
    let mut client = ws::connect_with(app.addr, ORIGIN, &[("cookie", cookie.as_str())])
        .await
        .unwrap();
    ws::send_text(&mut client, r#"{"type":"echo","payload":"ready"}"#).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(5)).await,
        Some(Message::Text(r#"{"type":"echo","payload":"ready"}"#.into()))
    );
    client
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unlinking_keeps_this_session_and_revokes_the_others() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = with_two(&app, &pool).await;
    let other = support::app::session_for(&pool, user).await;
    let mut client = socket(&app, &other).await;
    let entries = count(&pool, "SELECT count(*) FROM ledger_entries").await;

    let done = unlink(app.addr, "google", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.header("location"), Some("/"));
    assert_eq!(done.header("cache-control"), Some("private, no-store"));
    assert!(done.set_cookies().is_empty(), "{:?}", done.set_cookies());
    assert!(alive(&app, &token).await);
    assert!(!alive(&app, &other).await);
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(5)).await,
        Ending::Close(1008, "auth.unauthenticated".into())
    );
    assert_eq!(providers_of(&pool, user).await, ["discord"]);
    let events = sqlx::query(
        "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = 'unlink' \
         AND provider = 'google' AND length(ip_hash) = 32",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get::<i64, _>(0);
    assert_eq!(events, 1);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM ledger_entries").await,
        entries
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_last_provider_cannot_be_unlinked() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let user = user_of(&pool, &token).await;
    let other = support::app::session_for(&pool, user).await;
    let refused = unlink(app.addr, "google", &token, "").await;
    assert_eq!(refused.status, 409);
    assert_eq!(refused.text(), r#"{"error":"auth.last_provider"}"#);
    assert_eq!(refused.header("cache-control"), Some("private, no-store"));
    assert_eq!(providers_of(&pool, user).await, ["google"]);
    assert!(alive(&app, &other).await);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'unlink'"
        )
        .await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_provider_not_linked_or_unknown_is_not_found() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let user = user_of(&pool, &token).await;
    for provider in ["discord", "dev", "twitter", "Google"] {
        let missing = unlink(app.addr, provider, &token, "").await;
        assert_eq!(missing.status, 404, "{provider}");
        assert_eq!(missing.text(), r#"{"error":"http.not_found"}"#);
    }
    assert_eq!(providers_of(&pool, user).await, ["google"]);
}

/// Condition of security-reviewer (spec round 4): `unlink` never reads the body, so a form over
/// 4 KB is the same 303 as no body, and no response carries axum's text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_over_4_kb_is_never_read() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = with_two(&app, &pool).await;
    let body = format!("next={}", "a".repeat(4097 - "next=".len()));
    assert_eq!(body.len(), 4097);
    let done = unlink(app.addr, "google", &token, &body).await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.header("location"), Some("/"));
    let text = done.text();
    assert!(
        !text.contains("Failed to") && !text.contains("length limit"),
        "{text}"
    );
    assert_eq!(providers_of(&pool, user).await, ["discord"]);
}

/// Amendment 2026-10-29 of RF-C3: only providers that are linked **and** enabled count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_enabled_providers_count_as_the_one_that_remains() {
    let db = TestDb::create().await;
    let (with_both, pool) = oauth::start_app(&db).await;
    let (user, token) = with_two(&with_both, &pool).await;
    drop(with_both);
    // The same database with Discord off (loopback, no pair).
    let (app, _pool) = oauth::start_app_with(
        &db,
        &[("DISCORD_CLIENT_ID", ""), ("DISCORD_CLIENT_SECRET", "")],
        Settings::default(),
    )
    .await;
    let refused = unlink(app.addr, "google", &token, "").await;
    assert_eq!(refused.status, 409);
    assert_eq!(refused.text(), r#"{"error":"auth.last_provider"}"#);
    assert_eq!(providers_of(&pool, user).await, ["discord", "google"]);
    // Unlinking the disabled one is allowed: Google stays, and it is enabled.
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(providers_of(&pool, user).await, ["google"]);
}

/// Two unlinks at once (Google and Discord) leave exactly one provider: `FOR UPDATE` on the user
/// puts them in series, and the second sees 0 left. To make "at once" deterministic, the test
/// holds the user's `oauth_accounts` rows until both requests wait on a lock in Postgres, then
/// lets them go together: without the lock on the user, both `DELETE`s would run side by side
/// and each count would still see the other's row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_unlinks_at_once_leave_one_provider() {
    for _ in 0..3 {
        let db = TestDb::create().await;
        let (app, pool) = oauth::start_app(&db).await;
        let (user, token) = with_two(&app, &pool).await;
        let mut hold = pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM oauth_accounts WHERE user_id = $1 FOR UPDATE")
            .bind(user)
            .execute(&mut *hold)
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let tasks: Vec<_> = ["google", "discord"]
            .into_iter()
            .map(|provider| {
                let barrier = barrier.clone();
                let token = token.clone();
                let addr = app.addr;
                tokio::spawn(async move {
                    barrier.wait().await;
                    unlink(addr, provider, &token, "").await.status.as_u16()
                })
            })
            .collect();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let waiting: i64 = sqlx::query(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
            if waiting == 2 {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "{waiting} waiting");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        hold.commit().await.unwrap();
        let mut statuses = Vec::new();
        for task in tasks {
            statuses.push(task.await.unwrap());
        }
        statuses.sort_unstable();
        assert_eq!(statuses, [303, 409]);
        assert_eq!(providers_of(&pool, user).await.len(), 1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unlink_needs_a_session_and_the_database() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let refused = http::send(
        app.addr,
        "POST",
        "/api/auth/unlink/google",
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(refused.status, 401);
    assert_eq!(refused.text(), r#"{"error":"auth.unauthenticated"}"#);
    assert_eq!(refused.cookie(oauth::session_name()), Some(""));

    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let url = db.url_via(proxy.addr());
    let mut vars = vec![("DATABASE_URL", url.as_str())];
    vars.extend(support::config::PROVIDER_VARS);
    let config = support::config::config(&vars);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(config.database.clone());
    let state =
        AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp)).unwrap();
    let down = support::app::start(state).await;
    proxy.cut();
    let failed = unlink(down.addr, "google", &token, "").await;
    assert_eq!(failed.status, 503);
    assert_eq!(failed.text(), r#"{"error":"auth.try_again"}"#);
    assert!(failed.set_cookies().is_empty());
}
