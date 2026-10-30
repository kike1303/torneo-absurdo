//! 009 RF18 (plan decision 8): `GET /api/auth/reauth-status`, what `/account` needs before the
//! user taps. No dates, no ids and not the provider of the authentication; `private, no-store`;
//! the rule is the one of `unlink` (`unlink_allowed`) and the order that of the login buttons.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::app::Settings;
use server::sessions::TokenHash;
use sqlx::{PgPool, Row};
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use uuid::Uuid;

fn sub() -> String {
    format!("g-{}", Uuid::new_v4().simple())
}

fn snowflake() -> String {
    let n = u64::from_le_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    format!("{}", 100_000_000_000_000_000 + n % 900_000_000_000_000_000)
}

async fn status(app: &oauth::App, token: &str) -> serde_json::Value {
    let response = oauth::reauth_status(app, token).await;
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    serde_json::from_str(&response.text()).unwrap()
}

fn entry<'a>(status: &'a serde_json::Value, provider: &str) -> &'a serde_json::Value {
    status["unlink"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["provider"] == provider)
        .unwrap_or_else(|| panic!("no entry for {provider}: {status}"))
}

/// Nothing but the shape: only the keys of the protocol, and no number anywhere.
fn assert_only_the_shape(status: &serde_json::Value) {
    let mut keys: Vec<_> = status.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["link_ready", "unlink"]);
    assert!(status["link_ready"].is_boolean());
    for entry in status["unlink"].as_array().unwrap() {
        let mut keys: Vec<_> = entry.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["provider", "ready", "reauth_with"]);
        assert!(entry["provider"].is_string());
        assert!(entry["ready"].is_boolean());
        assert!(
            entry["reauth_with"]
                .as_array()
                .unwrap()
                .iter()
                .all(serde_json::Value::is_string)
        );
    }
}

/// Google and Discord linked, and a session reauthenticated with `by`.
async fn both(app: &oauth::App, by: Kind) -> (String, String, String) {
    let google = sub();
    let discord = snowflake();
    let first = oauth::session_of(&oauth::login(app, Kind::Google, &google).await);
    let rotated = oauth::session_of(&oauth::link(app, Kind::Discord, &discord, &first).await);
    let identity = match by {
        Kind::Google => &google,
        Kind::Discord => &discord,
    };
    let token = oauth::session_of(&oauth::reauth(app, by, identity, &rotated).await);
    (token, google, discord)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_session_it_is_401_with_the_cookie_expired() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let refused = http::send(app.addr, "GET", "/api/auth/reauth-status", &[]).await;
    assert_eq!(refused.status, 401);
    assert_eq!(refused.text(), r#"{"error":"auth.unauthenticated"}"#);
    assert_eq!(refused.cookie(oauth::session_name()), Some(""));
    assert_eq!(refused.header("cache-control"), Some("private, no-store"));
}

/// Authenticated with V1 (Google, the older): everything is ready, and V2 can be removed with
/// either, in the order of the login buttons.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_with_the_older_everything_is_ready() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let (token, _, _) = both(&app, Kind::Google).await;
    let status = status(&app, &token).await;
    assert_only_the_shape(&status);
    assert_eq!(status["link_ready"], true);
    assert_eq!(status["unlink"].as_array().unwrap().len(), 2);
    let google = entry(&status, "google");
    assert_eq!(google["ready"], true);
    assert_eq!(google["reauth_with"], serde_json::json!(["google"]));
    let discord = entry(&status, "discord");
    assert_eq!(discord["ready"], true);
    assert_eq!(
        discord["reauth_with"],
        serde_json::json!(["google", "discord"])
    );
}

/// Authenticated with V2: V1 is not ready and only V1 could confirm its removal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_with_the_newer_the_older_is_not_ready() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let (token, _, _) = both(&app, Kind::Discord).await;
    let status = status(&app, &token).await;
    assert_eq!(status["link_ready"], true);
    let google = entry(&status, "google");
    assert_eq!(google["ready"], false);
    assert_eq!(google["reauth_with"], serde_json::json!(["google"]));
    assert_eq!(entry(&status, "discord")["ready"], true);
}

/// Area 5: a disabled V1 with nothing older: `reauth_with` is empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disabled_provider_with_nothing_older_has_no_one_to_confirm_it() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let (_, _, discord) = both(&app, Kind::Google).await;
    drop(app);
    let (off, _) = oauth::start_app_with(
        &db,
        &[("GOOGLE_CLIENT_ID", ""), ("GOOGLE_CLIENT_SECRET", "")],
        Settings::default(),
    )
    .await;
    let token = oauth::session_of(&oauth::login(&off, Kind::Discord, &discord).await);
    let status = status(&off, &token).await;
    let google = entry(&status, "google");
    assert_eq!(google["ready"], false);
    assert_eq!(google["reauth_with"], serde_json::json!([]));
    assert_eq!(
        entry(&status, "discord")["reauth_with"],
        serde_json::json!(["discord"])
    );
}

/// After a link (the rotated session) and after a successful unlink (test 34) there is no
/// confirmation; after a reauthentication there is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_confirmation_follows_the_link_the_unlink_and_the_reauth() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let google = sub();
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, &google).await);
    assert_eq!(status(&app, &first).await["link_ready"], true);
    let rotated = oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
    let after_link = status(&app, &rotated).await;
    assert_eq!(after_link["link_ready"], false);
    assert_eq!(entry(&after_link, "discord")["ready"], false);
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &rotated).await);
    assert_eq!(status(&app, &token).await["link_ready"], true);
    // Older than 10 minutes.
    oauth::age(&pool, &token, 11).await;
    let old = status(&app, &token).await;
    assert_eq!(old["link_ready"], false);
    assert_eq!(entry(&old, "google")["ready"], false);
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &google, &token).await);
    // The confirmation consumed by an unlink.
    let cookie = oauth::session_cookie(&token);
    let done = http::send(
        app.addr,
        "POST",
        "/api/auth/unlink/discord",
        &[
            ("origin", support::app::ORIGIN),
            ("cookie", cookie.as_str()),
        ],
    )
    .await;
    assert_eq!(done.status, 303, "{}", done.text());
    let after_unlink = status(&app, &token).await;
    assert_eq!(after_unlink["link_ready"], false);
    assert_eq!(after_unlink["unlink"].as_array().unwrap().len(), 1);
    assert_eq!(entry(&after_unlink, "google")["ready"], false);
    let row = sqlx::query("SELECT auth_account_id FROM sessions WHERE token_hash = $1")
        .bind(TokenHash::of(&token).0.as_slice())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<i64>, _>(0), None);
}

/// A user without any account (a session of a test helper) still gets an answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_user_without_accounts_has_an_empty_list() {
    let db = TestDb::create().await;
    let (app, pool): (oauth::App, PgPool) = oauth::start_app(&db).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let status = status(&app, &token).await;
    assert_eq!(status["link_ready"], false);
    assert_eq!(status["unlink"], serde_json::json!([]));
}
