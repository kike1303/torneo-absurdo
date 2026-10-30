//! 009 RF11 and RF12 (plan decision 5, test 33 of ADR-0008's amendment 2026-10-30):
//! reauthenticating without changing user. A `reauth` flow is bound to the user of the session
//! that started it; its callback rotates that session into a fresh one only if the provider
//! returned one of the user's identities. Otherwise `auth.reauth_mismatch`, and nothing changes:
//! no user, no account, no event, and the session in use stays.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use server::app::Settings;
use server::auth::link::{LinkGate, LinkGateConfig};
use server::sessions::TokenHash;
use sqlx::{PgPool, Row};
use support::app::ORIGIN;
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use tokio::sync::Semaphore;
use uuid::Uuid;

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
}

async fn user_of(pool: &PgPool, token: &str) -> Uuid {
    sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1")
        .bind(TokenHash::of(token).0.as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

async fn alive(app: &oauth::App, token: &str) -> bool {
    let me = oauth::me(app, token).await;
    assert!(me.status == 200 || me.status == 401, "{}", me.status);
    me.status == 200
}

async fn step_up(pool: &PgPool, token: &str) -> bool {
    server::sessions::validate(pool, &TokenHash::of(token))
        .await
        .unwrap()
        .unwrap()
        .step_up
}

fn sub() -> String {
    format!("g-{}", Uuid::new_v4().simple())
}

fn snowflake() -> String {
    let n = u64::from_le_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    format!("{}", 100_000_000_000_000_000 + n % 900_000_000_000_000_000)
}

fn max_age_of(response: &http::Response) -> u64 {
    let prefix = format!("{}=", oauth::session_name());
    let line = response
        .set_cookies()
        .into_iter()
        .find(|l| l.starts_with(&prefix) && !l.starts_with(&format!("{prefix};")))
        .unwrap();
    line.split(';')
        .find_map(|part| part.trim().strip_prefix("Max-Age="))
        .unwrap()
        .parse()
        .unwrap()
}

/// What a refused reauth must leave as it was.
async fn snapshot(pool: &PgPool) -> (i64, i64, i64) {
    (
        count(pool, "SELECT count(*) FROM users").await,
        count(pool, "SELECT count(*) FROM oauth_accounts").await,
        count(
            pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'reauth'",
        )
        .await,
    )
}

/// RF12.4: the same user, a fresh authentication with that identity and a new expiry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reauth_rotates_into_a_fresh_session_of_the_same_user() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let identity = sub();
    let old = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let user = user_of(&pool, &old).await;
    oauth::age(&pool, &old, 30).await;
    sqlx::query("UPDATE sessions SET expires_at = now() + interval '1 hour' WHERE token_hash = $1")
        .bind(TokenHash::of(&old).0.as_slice())
        .execute(&pool)
        .await
        .unwrap();
    assert!(!step_up(&pool, &old).await);
    let entries = count(&pool, "SELECT count(*) FROM ledger_entries").await;

    let done = oauth::reauth(&app, Kind::Google, &identity, &old).await;
    assert_eq!(done.header("location"), Some("/es/account?reauth=1"));
    assert_eq!(done.header("cache-control"), Some("no-store"));
    assert_eq!(done.cookie(oauth::flow_name()), Some(""));
    let new = oauth::session_of(&done);
    assert!(!alive(&app, &old).await);
    assert_eq!(user_of(&pool, &new).await, user);
    assert!(step_up(&pool, &new).await);
    assert_eq!(max_age_of(&done), 30 * 24 * 3600);
    let events = sqlx::query(
        "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = 'reauth' \
         AND provider = 'google' AND length(ip_hash) = 32",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get::<i64, _>(0);
    assert_eq!(events, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM users").await, 1);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM ledger_entries").await,
        entries
    );

    // An admin's is 12 hours, like a login.
    let command = server::admin::AdminCommand {
        action: server::admin::AdminAction::Grant,
        user,
        reason: None,
    };
    server::admin::run(&db.options(), &command).await.unwrap();
    let admin = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let done = oauth::reauth(&app, Kind::Google, &identity, &admin).await;
    assert_eq!(max_age_of(&done), 12 * 3600);
}

/// Test 33: the provider returns another user's identity, or one that does not exist. The same
/// `auth.reauth_mismatch`, nothing is created or changed, and the session in use stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_identity_is_a_mismatch_and_changes_nothing() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let a = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let of_b = sub();
    let b = oauth::session_of(&oauth::login(&app, Kind::Google, &of_b).await);
    let (user_a, user_b) = (user_of(&pool, &a).await, user_of(&pool, &b).await);
    let before = snapshot(&pool).await;
    for identity in [of_b, sub()] {
        let done = oauth::reauth(&app, Kind::Google, &identity, &a).await;
        assert_eq!(oauth::error_of(&done), "auth.reauth_mismatch");
        assert_eq!(done.cookie(oauth::flow_name()), Some(""));
        assert_eq!(snapshot(&pool).await, before);
        assert!(alive(&app, &a).await);
        assert!(alive(&app, &b).await);
        assert_eq!(user_of(&pool, &a).await, user_a);
        assert_eq!(user_of(&pool, &b).await, user_b);
    }
}

/// RF12.1: the flow finishes only with a valid session of its user; nothing rotates otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reauth_flow_of_another_session_or_without_one_is_invalid_state() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let identity = sub();
    let a = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let b = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let before = snapshot(&pool).await;
    for session in [Some(b.as_str()), None] {
        let started = oauth::start_reauth(&app, Kind::Google, &a).await;
        let done = oauth::finish(&app, Kind::Google, &started, &identity, session).await;
        assert_eq!(oauth::error_of(&done), "auth.invalid_state", "{session:?}");
        assert!(alive(&app, &a).await);
        assert!(alive(&app, &b).await);
    }
    assert_eq!(snapshot(&pool).await, before);
}

/// Login CSRF on the round trip: a callback with the code and `state` of someone else's reauth
/// flow, but the victim's own flow cookie, never logs the victim into anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_callback_with_the_cookie_of_another_flow_changes_nothing() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let victim_identity = sub();
    let victim = oauth::session_of(&oauth::login(&app, Kind::Google, &victim_identity).await);
    let attacker_identity = sub();
    let attacker = oauth::session_of(&oauth::login(&app, Kind::Google, &attacker_identity).await);
    let user = user_of(&pool, &victim).await;
    let theirs = oauth::start_reauth(&app, Kind::Google, &attacker).await;
    let mine = oauth::start_reauth(&app, Kind::Google, &victim).await;
    let query = oauth::query_of(theirs.header("location").unwrap());
    let code = app
        .google
        .code_for(&query["code_challenge"], &attacker_identity);
    let path = format!(
        "/api/auth/google/callback?code={code}&state={}",
        query["state"]
    );
    let flow = mine.cookie(oauth::flow_name()).unwrap().to_owned();
    let done = oauth::callback(&app, &path, &flow, Some(&victim)).await;
    assert_eq!(oauth::error_of(&done), "auth.invalid_state");
    assert!(alive(&app, &victim).await);
    assert_eq!(user_of(&pool, &victim).await, user);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'reauth'"
        )
        .await,
        0
    );
}

/// RF11: the start needs a session and an enabled provider the user has an account of; a text
/// that is no provider is a 404, and a rejected form is `auth.invalid_state`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_start_needs_a_session_and_an_account_of_an_enabled_provider() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let refused = oauth::begin(&app, "/api/auth/reauth/google", "next=%2F", None).await;
    assert_eq!(refused.status, 303);
    assert_eq!(refused.header("location"), Some("/login"));
    assert_eq!(refused.cookie(oauth::session_name()), Some(""));
    assert_eq!(refused.header("cache-control"), Some("private, no-store"));

    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let mismatch = oauth::start_reauth(&app, Kind::Discord, &token).await;
    assert_eq!(mismatch.status, 303);
    assert_eq!(
        mismatch.header("location"),
        Some("/login?error=auth.reauth_mismatch")
    );
    assert_eq!(mismatch.header("cache-control"), Some("private, no-store"));
    assert!(
        mismatch.set_cookies().is_empty(),
        "{:?}",
        mismatch.set_cookies()
    );
    for provider in ["twitter", "start", "Google"] {
        let missing = oauth::begin(
            &app,
            &format!("/api/auth/reauth/{provider}"),
            "",
            Some(&token),
        )
        .await;
        assert_eq!(missing.status, 404, "{provider}");
        assert_eq!(missing.text(), r#"{"error":"http.not_found"}"#);
    }
    let too_large = format!("next={}", "a".repeat(4097 - "next=".len()));
    for body in [too_large.as_str(), "next=%2Fa&next=%2Fb"] {
        let done = oauth::begin(&app, "/api/auth/reauth/google", body, Some(&token)).await;
        assert_eq!(done.status, 303, "{}", done.text());
        assert_eq!(
            done.header("location"),
            Some("/login?error=auth.invalid_state")
        );
        let text = done.text();
        assert!(
            !text.contains("Failed to") && !text.contains("length limit"),
            "{text}"
        );
        assert!(alive(&app, &token).await);
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM oauth_flows").await, 0);

    // A provider the config turned off, even if linked, is the same mismatch.
    let both = oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &token).await);
    drop(app);
    let (off, _) = oauth::start_app_with(
        &db,
        &[("DISCORD_CLIENT_ID", ""), ("DISCORD_CLIENT_SECRET", "")],
        Settings::default(),
    )
    .await;
    let mismatch = oauth::start_reauth(&off, Kind::Discord, &both).await;
    assert_eq!(
        mismatch.header("location"),
        Some("/login?error=auth.reauth_mismatch")
    );
    assert!(mismatch.set_cookies().is_empty());
    assert_eq!(count(&pool, "SELECT count(*) FROM oauth_flows").await, 0);
}

/// Holds every link and reauth callback between its session check and its transaction.
struct Gate {
    arrived: AtomicUsize,
    permits: Arc<Semaphore>,
}

impl LinkGate for Gate {
    fn before_link(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.arrived.fetch_add(1, Ordering::SeqCst);
            self.permits.acquire().await.unwrap().forget();
        })
    }
}

/// A `logout-all` that commits while the callback talks to the provider is never undone by the
/// reauth: the rotation finds the session gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_logout_all_during_the_exchange_wins_over_the_reauth() {
    let db = TestDb::create().await;
    let gate = Arc::new(Gate {
        arrived: AtomicUsize::new(0),
        permits: Arc::new(Semaphore::new(0)),
    });
    let settings = Settings {
        link_gate: LinkGateConfig(gate.clone()),
        ..Settings::default()
    };
    let (app, pool) = oauth::start_app_with(&db, &[], settings).await;
    let identity = sub();
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let user = user_of(&pool, &token).await;
    let other = support::app::session_for(&pool, user).await;
    let started = oauth::start_reauth(&app, Kind::Google, &token).await;
    let pending = {
        let query = oauth::query_of(started.header("location").unwrap());
        let code = app.google.code_for(&query["code_challenge"], &identity);
        let path = format!(
            "/api/auth/google/callback?code={code}&state={}",
            query["state"]
        );
        let flow = started.cookie(oauth::flow_name()).unwrap().to_owned();
        app.refill();
        let addr = app.addr;
        let cookies = format!(
            "{}={flow}; {}",
            oauth::flow_name(),
            oauth::session_cookie(&token)
        );
        tokio::spawn(async move {
            http::send(addr, "GET", &path, &[("cookie", cookies.as_str())]).await
        })
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while gate.arrived.load(Ordering::SeqCst) < 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no callback arrived"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let cookie = oauth::session_cookie(&other);
    let out = http::send(
        app.addr,
        "POST",
        "/api/auth/logout-all",
        &[("origin", ORIGIN), ("cookie", cookie.as_str())],
    )
    .await;
    assert_eq!(out.status, 303);
    gate.permits.add_permits(1);
    let done = pending.await.unwrap();
    assert_eq!(oauth::error_of(&done), "auth.invalid_state");
    let sessions = sqlx::query("SELECT count(*) FROM sessions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(sessions, 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'reauth'"
        )
        .await,
        0
    );
}
