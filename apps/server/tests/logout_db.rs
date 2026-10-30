//! 008 RF-B11 with the database down: a logout that could not revoke never claims success. It
//! is a form-post navigation, so instead of a JSON error the browser gets a 303 to the login
//! page with the closed `auth.try_again` code, keeps its cookie (the session is still alive)
//! and can retry.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use server::app::{AppState, Settings};
use server::sessions::{self, TokenHash};
use sqlx::postgres::PgPoolOptions;
use support::app::{AlwaysUp, ORIGIN};
use support::db::TestDb;
use support::http;
use support::pg_proxy::PgProxy;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_logout_without_the_database_does_not_claim_success() {
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let url = db.url_via(proxy.addr());
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(config.database.clone());
    let state = AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp));
    let app = support::app::start(state).await;
    let name = config.cookies.session_name();
    let cookie = format!("{name}={token}");
    let headers = [("origin", ORIGIN), ("cookie", cookie.as_str())];
    let logout = || http::send(app.addr, "POST", "/api/auth/logout", &headers);

    proxy.cut();
    let failed = logout().await;
    assert_eq!(failed.status, 303, "{}", failed.text());
    assert_eq!(
        failed.header("location"),
        Some("/login?error=auth.try_again")
    );
    assert_eq!(failed.header("cache-control"), Some("private, no-store"));
    assert!(
        failed.set_cookies().is_empty(),
        "{:?}",
        failed.set_cookies()
    );
    assert!(failed.body.is_empty());

    proxy.restore();
    // The session is still alive: nothing pretended otherwise.
    let hash = TokenHash::of(&token);
    assert!(sessions::validate(&direct, &hash).await.unwrap().is_some());
    let done = logout().await;
    assert_eq!(done.status, 303);
    assert_eq!(done.header("location"), Some("/"));
    assert!(sessions::validate(&direct, &hash).await.unwrap().is_none());
}

/// QA, security and architect on #54: the session lookup fails but the database is otherwise up
/// (here `users` is locked and the app's statements time out after 300 ms; the revoke only
/// touches `sessions`). logout-all must not fall back to revoking just this session and
/// claiming success: nothing is revoked, the cookie stays, and the user is told to retry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logout_all_with_a_failed_session_lookup_revokes_nothing() {
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (user, token) = support::app::user_with_session(&direct).await;
    let other = support::app::session_for(&direct, user).await;
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let options = config
        .database
        .clone()
        .options([("statement_timeout", "300")]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(options);
    let state = AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp));
    let app = support::app::start(state).await;
    let cookie = format!("{}={token}", config.cookies.session_name());
    let headers = [("origin", ORIGIN), ("cookie", cookie.as_str())];

    let mut lock = direct.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let failed = http::send(app.addr, "POST", "/api/auth/logout-all", &headers).await;
    lock.rollback().await.unwrap();

    assert_eq!(failed.status, 303, "{}", failed.text());
    assert_eq!(
        failed.header("location"),
        Some("/login?error=auth.try_again")
    );
    assert!(
        failed.set_cookies().is_empty(),
        "{:?}",
        failed.set_cookies()
    );
    for live in [&token, &other] {
        let hash = TokenHash::of(live);
        assert!(sessions::validate(&direct, &hash).await.unwrap().is_some());
    }
}

/// The same failure on `GET /api/me` is not "no session": no 401 and no expired cookie (that
/// would log the user out on a database hiccup), but 503 `auth.try_again`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me_with_a_failed_session_lookup_keeps_the_session() {
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let options = config
        .database
        .clone()
        .options([("statement_timeout", "300")]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(options);
    let state = AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp));
    let app = support::app::start(state).await;
    let cookie = format!("{}={token}", config.cookies.session_name());

    let mut lock = direct.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let me = http::send(app.addr, "GET", "/api/me", &[("cookie", &cookie)]).await;
    lock.rollback().await.unwrap();

    assert_eq!(me.status, 503, "{}", me.text());
    assert_eq!(me.text(), r#"{"error":"auth.try_again"}"#);
    assert_eq!(me.header("cache-control"), Some("private, no-store"));
    assert!(me.set_cookies().is_empty(), "{:?}", me.set_cookies());
    let ok = http::send(app.addr, "GET", "/api/me", &[("cookie", &cookie)]).await;
    assert_eq!(ok.status, 200);
}
