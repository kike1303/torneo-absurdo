//! 008 RF-C3 (and its amendment 2026-10-29): unlinking a provider. It needs another provider
//! that is linked **and** enabled, revokes every other session of the user (their sockets close)
//! and writes `auth_events`; two unlinks at once never leave the user without a provider. 009
//! RF13 to RF15: it also needs a recent and current authentication with that provider or an
//! older one, read under the lock (test 32, the race), and it consumes it (test 34).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use server::app::{AppState, Settings};
use server::auth::unlink::{UnlinkGate, UnlinkGateConfig};
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
use tokio::sync::{Barrier, Semaphore};
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

/// A user with Google and Discord; returns the user and a session reauthenticated with Google,
/// the older (009 RF13: the session rotated by the link has no confirmation).
async fn with_two(app: &oauth::App, pool: &PgPool) -> (Uuid, String) {
    let google = sub();
    let first = oauth::session_of(&oauth::login(app, Kind::Google, &google).await);
    let rotated = oauth::session_of(&oauth::link(app, Kind::Discord, &snowflake(), &first).await);
    let token = oauth::session_of(&oauth::reauth(app, Kind::Google, &google, &rotated).await);
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
/// puts them in series, and the second sees 0 left (one session) or its session revoked (two
/// sessions). To make "at once" deterministic, the test
/// holds the user's `oauth_accounts` rows until both requests wait on a lock in Postgres, then
/// lets them go together: without the lock on the user, both `DELETE`s would run side by side
/// and each count would still see the other's row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_unlinks_at_once_leave_one_provider() {
    // 009: from one session, the lock of its row (`lock_auth`) already puts them in series; from
    // two sessions, only the lock of the user does. The first one revokes the other session, so
    // that one is a 401 (RF14, step 2).
    for (two_sessions, expected) in [(false, [303, 409]), (true, [303, 401])]
        .into_iter()
        .cycle()
        .take(4)
    {
        let db = TestDb::create().await;
        let (app, pool) = oauth::start_app(&db).await;
        let google = sub();
        let first = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
        let rotated =
            oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
        let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &rotated).await);
        let user = user_of(&pool, &token).await;
        // Another session authenticated with Google, the older: it may unlink either.
        let second = if two_sessions {
            oauth::session_of(&oauth::login(&app, Kind::Google, &google).await)
        } else {
            token.clone()
        };
        let mut hold = pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM oauth_accounts WHERE user_id = $1 FOR UPDATE")
            .bind(user)
            .execute(&mut *hold)
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let tasks: Vec<_> = [("google", token.clone()), ("discord", second)]
            .into_iter()
            .map(|(provider, token)| {
                let barrier = barrier.clone();
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
        assert_eq!(statuses, expected, "two sessions: {two_sessions}");
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

// --- 009 RF13 to RF15 --------------------------------------------------------------------

async fn count_of(pool: &PgPool, sql: &'static str, user: Uuid) -> i64 {
    sqlx::query(sql)
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

async fn unlinks(pool: &PgPool, user: Uuid) -> i64 {
    count_of(
        pool,
        "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = 'unlink'",
        user,
    )
    .await
}

async fn sessions_of(pool: &PgPool, user: Uuid) -> i64 {
    count_of(
        pool,
        "SELECT count(*) FROM sessions WHERE user_id = $1",
        user,
    )
    .await
}

/// An account of `user` inserted by SQL (it raises the `max(id)` of the user).
async fn seed(pool: &PgPool, user: Uuid, provider: &str) {
    sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ($1, $2, $3)",
    )
    .bind(provider)
    .bind(format!("{provider}-{}", Uuid::new_v4().simple()))
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
}

fn assert_reauth_required(response: &http::Response) {
    assert_eq!(response.status, 403, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"auth.reauth_required"}"#);
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    assert!(
        response.set_cookies().is_empty(),
        "{:?}",
        response.set_cookies()
    );
}

/// A user with Google (older) and Discord, and a fresh session authenticated with **Discord**.
async fn with_two_by_discord(app: &oauth::App, pool: &PgPool) -> (Uuid, String) {
    let first = oauth::session_of(&oauth::login(app, Kind::Google, &sub()).await);
    let discord = snowflake();
    let rotated = oauth::session_of(&oauth::link(app, Kind::Discord, &discord, &first).await);
    let token = oauth::session_of(&oauth::reauth(app, Kind::Discord, &discord, &rotated).await);
    (user_of(pool, &token).await, token)
}

/// RF13: a recent and current authentication with the provider itself or one linked before it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unlinking_needs_a_recent_authentication_not_newer_than_the_provider() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;

    // Authenticated with Google, the older: Discord goes.
    let (user, token) = with_two(&app, &pool).await;
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(providers_of(&pool, user).await, ["google"]);

    // Authenticated with Discord, the newer: Google stays, Discord goes.
    let (user, token) = with_two_by_discord(&app, &pool).await;
    let other = support::app::session_for(&pool, user).await;
    let sessions = sessions_of(&pool, user).await;
    let refused = unlink(app.addr, "google", &token, "").await;
    assert_reauth_required(&refused);
    assert_eq!(providers_of(&pool, user).await, ["discord", "google"]);
    assert_eq!(unlinks(&pool, user).await, 0);
    assert_eq!(sessions_of(&pool, user).await, sessions);
    assert!(alive(&app, &other).await);
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(providers_of(&pool, user).await, ["google"]);

    // Older than 10 minutes: refused, and a reauthentication makes it pass.
    let google = sub();
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let rotated = oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &rotated).await);
    let user = user_of(&pool, &token).await;
    oauth::age(&pool, &token, 11).await;
    assert_reauth_required(&unlink(app.addr, "discord", &token, "").await);
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &token).await);
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(providers_of(&pool, user).await, ["google"]);

    // A session without any authentication.
    let (user, _) = with_two(&app, &pool).await;
    let none = support::app::session_for(&pool, user).await;
    assert_reauth_required(&unlink(app.addr, "discord", &none, "").await);
    assert_eq!(unlinks(&pool, user).await, 0);
}

/// RF14 step 4 before step 5: one provider is 409 even with an old authentication, never an
/// invitation to reauthenticate that would not help.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_last_provider_is_409_before_the_rule() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    oauth::age(&pool, &token, 11).await;
    let refused = unlink(app.addr, "google", &token, "").await;
    assert_eq!(refused.status, 409);
    assert_eq!(refused.text(), r#"{"error":"auth.last_provider"}"#);
}

/// Test 34 and RF14 step 6: a successful unlink consumes the confirmation. Linking and a second
/// unlink need another one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unlink_consumes_the_confirmation() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let google = sub();
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let rotated = oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
    let user = user_of(&pool, &rotated).await;
    // A third account (enabled or not, the rule is the same), before the confirmation.
    seed(&pool, user, "dev").await;
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &rotated).await);
    let done = unlink(app.addr, "dev", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    let account: Option<i64> =
        sqlx::query("SELECT auth_account_id FROM sessions WHERE token_hash = $1")
            .bind(TokenHash::of(&token).0.as_slice())
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert_eq!(account, None);
    let flows = "SELECT count(*) FROM oauth_flows";
    let refused = oauth::start_link(&app, Kind::Discord, &token).await;
    assert_eq!(refused.status, 303);
    assert_eq!(
        refused.header("location"),
        Some("/login?error=auth.reauth_required")
    );
    assert_eq!(count(&pool, flows).await, 0);
    assert_reauth_required(&unlink(app.addr, "discord", &token, "").await);
    assert_eq!(providers_of(&pool, user).await, ["discord", "google"]);
    // After another confirmation, it can again.
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &token).await);
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(providers_of(&pool, user).await, ["google"]);
}

/// security-reviewer's scenario: with a recent session with V1, removing V2 passes, and linking
/// an X right after fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_confirmation_is_one_thing_remove_or_link() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = with_two(&app, &pool).await;
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    let refused = oauth::start_link(&app, Kind::Discord, &token).await;
    assert_eq!(
        refused.header("location"),
        Some("/login?error=auth.reauth_required")
    );
    assert_eq!(providers_of(&pool, user).await, ["google"]);
}

/// security-reviewer's note on the spec: a link started with the confirmation, then a
/// successful unlink, then the callback with a Discord X. The callback finds the confirmation
/// consumed: nothing of X, no `link` event, the session alive and the flow cookie expired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_started_before_an_unlink_does_not_link_after_it() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = with_two(&app, &pool).await;
    let started = oauth::start_link(&app, Kind::Discord, &token).await;
    assert_eq!(started.status, 303, "{}", started.text());
    let done = unlink(app.addr, "discord", &token, "").await;
    assert_eq!(done.status, 303, "{}", done.text());
    let x = snowflake();
    let done = oauth::finish(&app, Kind::Discord, &started, &x, Some(&token)).await;
    assert_eq!(oauth::error_of(&done), "auth.reauth_required");
    assert_eq!(done.cookie(oauth::flow_name()), Some(""));
    let of_x: i64 = sqlx::query("SELECT count(*) FROM oauth_accounts WHERE provider_user_id = $1")
        .bind(&x)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(of_x, 0);
    assert_eq!(
        count_of(
            &pool,
            "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = 'link'",
            user
        )
        .await,
        1,
        "only the link of with_two"
    );
    assert!(alive(&app, &token).await);
}

/// Area 5: a provider the config turned off can only go with an older identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disabled_provider_goes_only_with_an_older_identity() {
    let db = TestDb::create().await;
    let (both, pool) = oauth::start_app(&db).await;
    let discord = snowflake();
    let first = oauth::session_of(&oauth::login(&both, Kind::Google, &sub()).await);
    let rotated = oauth::session_of(&oauth::link(&both, Kind::Discord, &discord, &first).await);
    let user = user_of(&pool, &rotated).await;
    drop(both);
    let (app, _) = oauth::start_app_with(
        &db,
        &[("GOOGLE_CLIENT_ID", ""), ("GOOGLE_CLIENT_SECRET", "")],
        Settings::default(),
    )
    .await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Discord, &discord).await);
    assert_reauth_required(&unlink(app.addr, "google", &token, "").await);
    assert_eq!(providers_of(&pool, user).await, ["discord", "google"]);
}

/// The login with X that crosses the unlink of X (architect's suggestion 5): the delete of X is
/// held uncommitted while the login reads the identity; the `INSERT` of its session waits on the
/// row of X and fails on the FK when the delete commits. `auth.try_again`, never a session.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_login_that_crosses_the_unlink_of_its_identity_gets_no_session() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let google = sub();
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let user = user_of(&pool, &first).await;
    oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
    let before = sessions_of(&pool, user).await;
    let started = oauth::start(&app, Kind::Google, None).await;
    let mut delete = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM oauth_accounts WHERE user_id = $1 AND provider = 'google'")
        .bind(user)
        .execute(&mut *delete)
        .await
        .unwrap();
    let login = {
        let query = oauth::query_of(started.header("location").unwrap());
        let code = app.google.code_for(&query["code_challenge"], &google);
        let path = format!(
            "/api/auth/google/callback?code={code}&state={}",
            query["state"]
        );
        let flow = started.cookie(oauth::flow_name()).unwrap().to_owned();
        app.refill();
        let addr = app.addr;
        let cookie = format!("{}={flow}", oauth::flow_name());
        tokio::spawn(
            async move { http::send(addr, "GET", &path, &[("cookie", cookie.as_str())]).await },
        )
    };
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
        if waiting == 1 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "{waiting} waiting");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    delete.commit().await.unwrap();
    let done = login.await.unwrap();
    assert_eq!(oauth::error_of(&done), "auth.try_again");
    assert_eq!(sessions_of(&pool, user).await, before);
    // The kind the login logs (`auth.login_failed`) for that error.
    let error = sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, ip_hash, auth_account_id, \
         auth_link_seq) VALUES ('\\x02', $1, now() + interval '1 day', '\\x00', 999999, 1)",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(server::db::error_kind(&error), "foreign_key");
}

// --- The race of RF15 (test 32) ----------------------------------------------------------

/// Holds the **first** unlink between its session check and its transaction until released;
/// every later one goes through (the interfering requests of the race may be unlinks too).
struct Gate {
    arrived: AtomicUsize,
    permits: Arc<Semaphore>,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            arrived: AtomicUsize::new(0),
            permits: Arc::new(Semaphore::new(0)),
        })
    }
}

impl UnlinkGate for Gate {
    fn before_unlink(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if self.arrived.fetch_add(1, Ordering::SeqCst) == 0 {
                self.permits.acquire().await.unwrap().forget();
            }
        })
    }
}

/// What happens to S1 while its unlink of Discord waits at the gate.
#[derive(Debug, Clone, Copy)]
enum Meanwhile {
    /// (a) S1 logs out.
    Logout,
    /// (b) S2 logs out every session.
    LogoutAll,
    /// (c1) S2 unlinks Google by HTTP, which revokes S1.
    OtherUnlinksTheIdentity,
    /// (c2) the identity of S1 (Google) is deleted by SQL: the FK leaves S1 without it.
    IdentityDeleted,
    /// (d) S1 links a provider (with `dev-auth`: `dev`), which rotates it.
    #[cfg(feature = "dev-auth")]
    LinkRotates,
    /// (e) an account linked by SQL: S1 stays, no longer current.
    AccountLinked,
    /// (e) by HTTP: S2 links `dev`.
    #[cfg(feature = "dev-auth")]
    OtherLinks,
    /// (f) S1 reauthenticates, which rotates it.
    Reauth,
}

/// The dev link of `token` by HTTP (only with `dev-auth`): start, authorize, callback.
#[cfg(feature = "dev-auth")]
async fn link_dev(app: &oauth::App, token: &str) -> http::Response {
    let identity = format!("dev-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let body = format!("dev_identity={identity}&next=%2F");
    let started = oauth::begin(app, "/api/auth/link/dev", &body, Some(token)).await;
    assert_eq!(started.status, 303, "{}", started.text());
    let authorized = http::send(app.addr, "GET", &started.location_path(), &[]).await;
    assert_eq!(authorized.status, 303, "{}", authorized.text());
    let flow = started.cookie(oauth::flow_name()).unwrap().to_owned();
    oauth::callback(app, &authorized.location_path(), &flow, Some(token)).await
}

async fn race(meanwhile: Meanwhile) {
    let db = TestDb::create().await;
    let gate = Gate::new();
    let settings = Settings {
        unlink_gate: UnlinkGateConfig(gate.clone()),
        ..Settings::default()
    };
    let (app, pool) = oauth::start_app_with(&db, &[], settings).await;
    let google = sub();
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let rotated = oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
    let user = user_of(&pool, &rotated).await;
    if matches!(meanwhile, Meanwhile::IdentityDeleted) {
        // A third account, so the paused unlink does not stop at 409 once Google is gone.
        seed(&pool, user, "dev").await;
    }
    let s1 = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &rotated).await);
    // S2: a fresh login with Google (also current: nothing was linked after it).
    let s2 = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let target = if matches!(meanwhile, Meanwhile::IdentityDeleted) {
        "dev"
    } else {
        "discord"
    };
    let pending = {
        let addr = app.addr;
        let s1 = s1.clone();
        tokio::spawn(async move { unlink(addr, target, &s1, "").await })
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while gate.arrived.load(Ordering::SeqCst) < 1 {
        assert!(tokio::time::Instant::now() < deadline, "no unlink arrived");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let post = |path: &'static str, token: &str| {
        let cookie = oauth::session_cookie(token);
        let addr = app.addr;
        async move {
            http::send(
                addr,
                "POST",
                path,
                &[("origin", ORIGIN), ("cookie", cookie.as_str())],
            )
            .await
        }
    };
    let s1_survives = match meanwhile {
        Meanwhile::Logout => {
            assert_eq!(post("/api/auth/logout", &s1).await.status, 303);
            false
        }
        Meanwhile::LogoutAll => {
            assert_eq!(post("/api/auth/logout-all", &s2).await.status, 303);
            false
        }
        Meanwhile::OtherUnlinksTheIdentity => {
            let done = unlink(app.addr, "google", &s2, "").await;
            assert_eq!(done.status, 303, "{}", done.text());
            false
        }
        Meanwhile::IdentityDeleted => {
            sqlx::query("DELETE FROM oauth_accounts WHERE user_id = $1 AND provider = 'google'")
                .bind(user)
                .execute(&pool)
                .await
                .unwrap();
            true
        }
        #[cfg(feature = "dev-auth")]
        Meanwhile::LinkRotates => {
            oauth::session_of(&link_dev(&app, &s1).await);
            false
        }
        Meanwhile::AccountLinked => {
            seed(&pool, user, "dev").await;
            true
        }
        #[cfg(feature = "dev-auth")]
        Meanwhile::OtherLinks => {
            oauth::session_of(&link_dev(&app, &s2).await);
            true
        }
        Meanwhile::Reauth => {
            oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &s1).await);
            false
        }
    };
    let providers = providers_of(&pool, user).await;
    let sessions = sessions_of(&pool, user).await;
    let consumed_before: Option<i64> =
        sqlx::query("SELECT auth_account_id FROM sessions WHERE token_hash = $1")
            .bind(TokenHash::of(&s1).0.as_slice())
            .fetch_optional(&pool)
            .await
            .unwrap()
            .and_then(|row| row.get(0));
    // Every NOTIFY of the paused request would come after this point.
    let mut notes = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .unwrap();
    notes
        .listen(server::sessions::REVOKED_CHANNEL)
        .await
        .unwrap();
    gate.permits.add_permits(1);
    let done = pending.await.unwrap();
    if s1_survives {
        assert_reauth_required(&done);
    } else {
        assert_eq!(done.status, 401, "{meanwhile:?}: {}", done.text());
        assert_eq!(done.text(), r#"{"error":"auth.unauthenticated"}"#);
        assert_eq!(done.cookie(oauth::session_name()), Some(""));
    }
    assert_eq!(providers_of(&pool, user).await, providers, "{meanwhile:?}");
    assert_eq!(sessions_of(&pool, user).await, sessions, "{meanwhile:?}");
    assert_eq!(
        unlinks(&pool, user).await,
        i64::from(matches!(meanwhile, Meanwhile::OtherUnlinksTheIdentity)),
        "{meanwhile:?}"
    );
    let note = tokio::time::timeout(Duration::from_millis(300), notes.recv()).await;
    assert!(
        note.is_err(),
        "{meanwhile:?}: the paused unlink revoked a session"
    );
    if s1_survives {
        let consumed_after: Option<i64> =
            sqlx::query("SELECT auth_account_id FROM sessions WHERE token_hash = $1")
                .bind(TokenHash::of(&s1).0.as_slice())
                .fetch_one(&pool)
                .await
                .unwrap()
                .get(0);
        assert_eq!(consumed_after, consumed_before, "{meanwhile:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_logout_during_the_pause_is_401() {
    race(Meanwhile::Logout).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_logout_all_during_the_pause_is_401() {
    race(Meanwhile::LogoutAll).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_unlink_of_the_identity_during_the_pause_is_401() {
    race(Meanwhile::OtherUnlinksTheIdentity).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_identity_deleted_during_the_pause_is_403() {
    race(Meanwhile::IdentityDeleted).await;
}

#[cfg(feature = "dev-auth")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_link_of_the_session_during_the_pause_is_401() {
    race(Meanwhile::LinkRotates).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_account_linked_during_the_pause_is_403() {
    race(Meanwhile::AccountLinked).await;
}

#[cfg(feature = "dev-auth")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_session_linking_during_the_pause_is_403() {
    race(Meanwhile::OtherLinks).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reauth_of_the_session_during_the_pause_is_401() {
    race(Meanwhile::Reauth).await;
}
