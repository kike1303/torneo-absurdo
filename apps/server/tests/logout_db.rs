//! 008 RF-B11 with the database down: a logout that could not revoke never claims success. It
//! is a form-post navigation, so instead of a JSON error the browser gets a 303 to the login
//! page with the closed `auth.try_again` code, keeps its cookie (the session is still alive)
//! and can retry. 009 plan decision 6: `logout-all` takes the lock of the user first.
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
    let state =
        AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp)).unwrap();
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
    let state =
        AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp)).unwrap();
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
    let state =
        AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp)).unwrap();
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

/// Waits until one backend of this database waits on a lock.
async fn one_waiting(pool: &sqlx::PgPool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let waiting: i64 = sqlx::Row::get(
            &sqlx::query(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(pool)
            .await
            .unwrap(),
            0,
        );
        if waiting == 1 {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "{waiting} waiting");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn sessions_of(pool: &sqlx::PgPool, user: uuid::Uuid) -> i64 {
    sqlx::Row::get(
        &sqlx::query("SELECT count(*) FROM sessions WHERE user_id = $1")
            .bind(user)
            .fetch_one(pool)
            .await
            .unwrap(),
        0,
    )
}

/// 009 plan decision 6 (answer 2 of architect): `logout-all` takes the lock of the user first,
/// so it runs in series with every link, reauth and unlink of that user and never forms a cycle
/// with them. Here the test holds the lock; the `logout-all` waits for it, then revokes all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_all_waits_for_the_lock_of_the_user() {
    let db = TestDb::create().await;
    let (app, pool) = support::app::start_db(&db, &[], &Settings::default()).await;
    let (user, token) = support::app::user_with_session(&pool).await;
    support::app::session_for(&pool, user).await;
    let name = support::config::config(&[]).cookies.session_name();
    let cookie = format!("{name}={token}");
    let mut hold = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(user)
        .execute(&mut *hold)
        .await
        .unwrap();
    let addr = app.addr;
    let request = tokio::spawn(async move {
        http::send(
            addr,
            "POST",
            "/api/auth/logout-all",
            &[("origin", ORIGIN), ("cookie", cookie.as_str())],
        )
        .await
    });
    one_waiting(&pool).await;
    assert_eq!(sessions_of(&pool, user).await, 2);
    hold.commit().await.unwrap();
    let done = request.await.unwrap();
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.header("location"), Some("/"));
    assert_eq!(sessions_of(&pool, user).await, 0);
}

/// The precision of architect for step 6: a ban that commits while `logout-all` waits on the
/// lock. The lock has no `banned_at IS NULL`: a banned user can still close every session.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_all_still_closes_everything_if_a_ban_commits_while_it_waits() {
    let db = TestDb::create().await;
    let (app, pool) = support::app::start_db(&db, &[], &Settings::default()).await;
    let (user, token) = support::app::user_with_session(&pool).await;
    support::app::session_for(&pool, user).await;
    let name = support::config::config(&[]).cookies.session_name();
    let cookie = format!("{name}={token}");
    let mut ban = support::db::begin_privileged(&pool).await;
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(user)
        .execute(&mut *ban)
        .await
        .unwrap();
    let addr = app.addr;
    let request = tokio::spawn(async move {
        http::send(
            addr,
            "POST",
            "/api/auth/logout-all",
            &[("origin", ORIGIN), ("cookie", cookie.as_str())],
        )
        .await
    });
    one_waiting(&pool).await;
    // The ban of the test is only the column: the sessions are the logout-all's to close.
    sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1")
        .bind(user)
        .execute(&mut *ban)
        .await
        .unwrap();
    ban.commit().await.unwrap();
    let done = request.await.unwrap();
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.header("location"), Some("/"));
    assert_eq!(sessions_of(&pool, user).await, 0);
}
