//! The takeover of TA-29 (tests 29 and 30 of ADR-0008's amendment 2026-10-30, criteria 1 and 3
//! of the intent of 009). An attacker with a stolen session of the victim links an identity of
//! their own (Discord X) and tries to remove the victim's (Google V): never, whatever they do
//! after. The victim, with Google, removes X and throws the attacker out. No coin moves.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use server::sessions::TokenHash;
use sqlx::{PgPool, Row};
use support::app::ORIGIN;
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use support::ws::{self, Ending};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn user_of(pool: &PgPool, token: &str) -> Uuid {
    sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1")
        .bind(TokenHash::of(token).0.as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
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

async fn unlinks(pool: &PgPool) -> i64 {
    sqlx::query("SELECT count(*) FROM auth_events WHERE kind = 'unlink'")
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

async fn post(app: &oauth::App, path: &str, token: &str) -> http::Response {
    let cookie = oauth::session_cookie(token);
    http::send(
        app.addr,
        "POST",
        path,
        &[("origin", ORIGIN), ("cookie", cookie.as_str())],
    )
    .await
}

async fn alive(app: &oauth::App, token: &str) -> bool {
    oauth::me(app, token).await.status == 200
}

fn sub() -> String {
    format!("g-{}", Uuid::new_v4().simple())
}

fn snowflake() -> String {
    let n = u64::from_le_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    format!("{}", 100_000_000_000_000_000 + n % 900_000_000_000_000_000)
}

/// The ledger only has the grants of the first logins: one entry per user, all `grant`.
async fn only_first_grants(pool: &PgPool) {
    let row = sqlx::query(
        "SELECT (SELECT count(*) FROM ledger_entries), \
         (SELECT count(*) FROM ledger_entries WHERE kind = 'grant'), \
         (SELECT count(*) FROM users), \
         (SELECT count(*) FROM ledger_postings)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let (entries, grants, users, postings): (i64, i64, i64, i64) =
        (row.get(0), row.get(1), row.get(2), row.get(3));
    assert_eq!(entries, users);
    assert_eq!(grants, entries);
    assert_eq!(postings, 2 * entries);
}

/// The attacker's try at removing Google: refused, nothing changes, and the victim still gets
/// in with Google, to the same account.
async fn refused(
    app: &oauth::App,
    pool: &PgPool,
    token: &str,
    victim: Uuid,
    google: &str,
    case: &str,
) {
    let done = post(app, "/api/auth/unlink/google", token).await;
    assert_eq!(done.status, 403, "{case}: {}", done.text());
    assert_eq!(done.text(), r#"{"error":"auth.reauth_required"}"#, "{case}");
    assert_eq!(
        providers_of(pool, victim).await,
        ["discord", "google"],
        "{case}"
    );
    assert_eq!(unlinks(pool).await, 0, "{case}");
    let back = oauth::session_of(&oauth::login(app, Kind::Google, google).await);
    assert_eq!(user_of(pool, &back).await, victim, "{case}");
}

/// Test 29: a recent stolen session. The attacker links X and tries to remove V right away,
/// after a logout and a login with X, after a logout-all and a login with X, and after "waiting
/// 400 days" and reauthenticating with X. Always 403.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_attacker_never_removes_the_victims_identity() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let google = sub();
    let stolen = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let victim = user_of(&pool, &stolen).await;
    let x = snowflake();

    // The link of X passes (the stolen session is recent).
    let linked = oauth::session_of(&oauth::link(&app, Kind::Discord, &x, &stolen).await);
    assert_eq!(providers_of(&pool, victim).await, ["discord", "google"]);
    // (a) Right away.
    refused(&app, &pool, &linked, victim, &google, "(a) right away").await;
    // (b) A logout and a login with X.
    assert_eq!(post(&app, "/api/auth/logout", &linked).await.status, 303);
    let with_x = oauth::session_of(&oauth::login(&app, Kind::Discord, &x).await);
    assert_eq!(user_of(&pool, &with_x).await, victim);
    refused(
        &app,
        &pool,
        &with_x,
        victim,
        &google,
        "(b) logout and login with X",
    )
    .await;
    // (c) A logout-all and a login with X.
    assert_eq!(
        post(&app, "/api/auth/logout-all", &with_x).await.status,
        303
    );
    let with_x = oauth::session_of(&oauth::login(&app, Kind::Discord, &x).await);
    refused(
        &app,
        &pool,
        &with_x,
        victim,
        &google,
        "(c) logout-all and login with X",
    )
    .await;
    // (d) "400 days later": every date moved back, a login with X and a reauth with X.
    for sql in [
        "UPDATE sessions SET authenticated_at = authenticated_at - interval '400 days'",
        "UPDATE oauth_accounts SET created_at = created_at - interval '400 days'",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let with_x = oauth::session_of(&oauth::login(&app, Kind::Discord, &x).await);
    let with_x = oauth::session_of(&oauth::reauth(&app, Kind::Discord, &x, &with_x).await);
    refused(
        &app,
        &pool,
        &with_x,
        victim,
        &google,
        "(d) 400 days and a reauth with X",
    )
    .await;
    only_first_grants(&pool).await;
}

/// Test 29, the old stolen session: the attack ends at its first step, with no flow and no row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_stolen_session_cannot_even_link() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let stolen = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let victim = user_of(&pool, &stolen).await;
    oauth::age(&pool, &stolen, 11).await;
    let refused = oauth::start_link(&app, Kind::Discord, &stolen).await;
    assert_eq!(refused.status, 303);
    assert_eq!(
        refused.header("location"),
        Some("/login?error=auth.reauth_required")
    );
    assert!(refused.set_cookies().is_empty());
    let flows: i64 = sqlx::query("SELECT count(*) FROM oauth_flows")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(flows, 0);
    assert_eq!(providers_of(&pool, victim).await, ["google"]);
    only_first_grants(&pool).await;
}

/// Test 30: after the link of X, the victim confirms with Google and removes X. Every session
/// of the attacker gives 401 and its socket closes at once, and a login with X is a new user,
/// without the victim's coins or alias.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_victim_removes_the_attacker() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let google = sub();
    let stolen = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let own = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    let victim = user_of(&pool, &own).await;
    let x = snowflake();
    let attacker = oauth::session_of(&oauth::link(&app, Kind::Discord, &x, &stolen).await);
    let other = oauth::session_of(&oauth::login(&app, Kind::Discord, &x).await);
    let cookie = oauth::session_cookie(&other);
    let mut socket = ws::connect_with(app.addr, ORIGIN, &[("cookie", cookie.as_str())])
        .await
        .unwrap();
    ws::send_text(&mut socket, r#"{"type":"echo","payload":"ready"}"#).await;
    assert_eq!(
        ws::next_data(&mut socket, Duration::from_secs(5)).await,
        Some(Message::Text(r#"{"type":"echo","payload":"ready"}"#.into()))
    );
    let me: serde_json::Value = serde_json::from_str(&oauth::me(&app, &own).await.text()).unwrap();

    // The victim's own session is no longer current (X was linked after it): a confirmation.
    let confirmed = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &own).await);
    let done = post(&app, "/api/auth/unlink/discord", &confirmed).await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(providers_of(&pool, victim).await, ["google"]);
    assert!(!alive(&app, &attacker).await);
    assert!(!alive(&app, &other).await);
    assert!(alive(&app, &confirmed).await);
    assert_eq!(
        ws::ending(&mut socket, Duration::from_secs(5)).await,
        Ending::Close(1008, "auth.unauthenticated".into())
    );

    let again = oauth::session_of(&oauth::login(&app, Kind::Discord, &x).await);
    assert_ne!(user_of(&pool, &again).await, victim);
    let theirs: serde_json::Value =
        serde_json::from_str(&oauth::me(&app, &again).await.text()).unwrap();
    assert_ne!(theirs["display_name"], me["display_name"]);
    assert_eq!(theirs["providers"], serde_json::json!(["discord"]));
    // The grant of their own first login, never the victim's coins.
    assert_eq!(theirs["balance"], 100);
    assert_eq!(me["balance"], 100);
    only_first_grants(&pool).await;
}
