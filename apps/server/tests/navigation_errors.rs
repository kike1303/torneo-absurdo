//! 008 "Enmienda 2026-10-30" of RF-B5 and RF-C2 (plan-d step 9b): the five routes the browser
//! posts or navigates to (`start`, `link`, `callback`, `logout`, `logout-all`) never answer
//! JSON for a 429 or a 503. They answer 303 to `/login?error=auth.try_again`, without
//! `Retry-After` or a body, with `no-store` (`private, no-store` for the logouts), never with
//! `next`, and without touching the database in the quota branch. `link` without a session is a
//! 303 to `/login` with no query. The 404 of an unknown provider and the 403 of `Origin` stay
//! JSON.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use server::app::{AppState, Settings};
use server::sessions::{self, TokenHash};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::app::{AlwaysUp, ORIGIN, RunningApp};
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http::{self, Response};
use support::oauth;
use support::pg_proxy::PgProxy;

const TRY_AGAIN: &str = "/login?error=auth.try_again";

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
}

async fn rows(pool: &PgPool) -> (i64, i64) {
    (
        count(pool, "SELECT count(*) FROM oauth_flows").await,
        count(pool, "SELECT count(*) FROM sessions").await,
    )
}

/// The amendment's answer: 303 to the closed `auth.try_again`, nothing else of a 429 or a 503.
fn assert_try_again(response: &Response, cache_control: &str) {
    assert_eq!(response.status, 303, "{}", response.text());
    assert_eq!(response.header("location"), Some(TRY_AGAIN));
    assert_eq!(response.header("retry-after"), None);
    assert_eq!(response.header("content-type"), None);
    assert!(response.body.is_empty(), "{}", response.text());
    assert_eq!(response.header("cache-control"), Some(cache_control));
}

fn form_headers(cookie: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("origin", ORIGIN.to_owned()),
        (
            "content-type",
            "application/x-www-form-urlencoded".to_owned(),
        ),
    ];
    if let Some(cookie) = cookie {
        headers.push(("cookie", cookie.to_owned()));
    }
    headers
}

/// A form post that does not refill the AUTH_IP bucket.
async fn post(addr: std::net::SocketAddr, path: &str, cookie: Option<&str>) -> Response {
    let headers = form_headers(cookie);
    let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    http::send_body(addr, "POST", path, &headers, "next=%2Fcuenta").await
}

/// Drains the AUTH_IP burst (5) of 127.0.0.1 with `start`s, each a 303 to the provider.
async fn drain_auth_ip(app: &oauth::App) {
    app.refill();
    for i in 0..5 {
        let started = post(app.addr, "/api/auth/google/start", None).await;
        assert_eq!(started.status, 303, "{i}: {}", started.text());
        assert_ne!(started.header("location"), Some(TRY_AGAIN), "{i}");
    }
}

// --- The quota (AUTH_IP) ----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_over_the_quota_is_try_again_without_a_flow() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    drain_auth_ip(&app).await;
    let before = rows(&pool).await;
    // `next` is ignored, whatever it says.
    let limited = post(app.addr, "/api/auth/google/start", None).await;
    assert_try_again(&limited, "no-store");
    assert!(
        limited.set_cookies().is_empty(),
        "{:?}",
        limited.set_cookies()
    );
    assert_eq!(rows(&pool).await, before);
    // The full key table is a 429 of RF-A4 too: covered by the layer's UT (`max_keys`).
    // Once the bucket refills, the same request starts a flow again.
    app.refill();
    let started = post(app.addr, "/api/auth/google/start", None).await;
    assert!(
        started
            .cookie(oauth::flow_name())
            .is_some_and(|v| !v.is_empty())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn link_over_the_quota_is_try_again_without_a_flow() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, "g-sub-quota").await);
    drain_auth_ip(&app).await;
    let before = rows(&pool).await;
    let cookie = oauth::session_cookie(&token);
    let limited = post(app.addr, "/api/auth/link/discord", Some(&cookie)).await;
    assert_try_again(&limited, "no-store");
    assert!(
        limited.set_cookies().is_empty(),
        "{:?}",
        limited.set_cookies()
    );
    assert_eq!(rows(&pool).await, before);
    assert_eq!(oauth::me(&app, &token).await.status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callback_over_the_quota_expires_the_flow_cookie_like_the_handler() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    // A live flow, started before the bucket runs out.
    let started = oauth::start(&app, Kind::Google, None).await;
    let query = oauth::query_of(started.header("location").unwrap());
    let code = app
        .google
        .code_for(&query["code_challenge"], "g-sub-callback");
    let flow = started.cookie(oauth::flow_name()).unwrap().to_owned();
    let path = format!(
        "/api/auth/google/callback?code={code}&state={}",
        query["state"]
    );
    let cookie = format!("{}={flow}", oauth::flow_name());
    drain_auth_ip(&app).await;
    let before = rows(&pool).await;
    let limited = http::send(app.addr, "GET", &path, &[("cookie", cookie.as_str())]).await;
    assert_try_again(&limited, "no-store");
    // The flow cookie is expired, and with the very `Set-Cookie` of the handler's errors.
    app.refill();
    let handler = http::send(
        app.addr,
        "GET",
        "/api/auth/google/callback?code=x&state=y",
        &[],
    )
    .await;
    assert_eq!(
        handler.header("location"),
        Some("/login?error=auth.invalid_state")
    );
    assert_eq!(limited.cookie(oauth::flow_name()), Some(""));
    assert_eq!(limited.set_cookies(), handler.set_cookies());
    assert!(limited.cookie(oauth::session_name()).is_none());
    // Nothing was consumed: the row is still there, and no session was made.
    assert_eq!(before, rows(&pool).await);
    assert_eq!(before.1, 0);
    assert!(before.0 >= 1);
}

// --- The quota of the logouts (MUTATION_IP and MUTATION_USER) ---------------------------

/// `logout` over the per-IP quota (240 a minute): the session stays valid and its cookie is not
/// expired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logout_over_the_ip_quota_keeps_the_session() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, "g-sub-logout").await);
    // Without a cookie a logout reads nothing: it only spends the bucket.
    for i in 0..240 {
        let out = post(app.addr, "/api/auth/logout", None).await;
        assert_eq!(out.header("location"), Some("/"), "{i}");
    }
    let before = rows(&pool).await;
    let cookie = oauth::session_cookie(&token);
    for path in ["/api/auth/logout", "/api/auth/logout-all"] {
        let limited = post(app.addr, path, Some(&cookie)).await;
        assert_try_again(&limited, "private, no-store");
        assert!(
            limited.set_cookies().is_empty(),
            "{path}: {:?}",
            limited.set_cookies()
        );
    }
    assert_eq!(rows(&pool).await, before);
    let hash = TokenHash::of(&token);
    assert!(sessions::validate(&pool, &hash).await.unwrap().is_some());
}

/// `logout-all` over the per-user quota (60 a minute, shared with the profile writes).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logout_all_over_the_user_quota_keeps_every_session() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, "g-sub-all").await);
    let other = oauth::session_of(&oauth::login(&app, Kind::Google, "g-sub-all").await);
    for i in 0..60 {
        let done = oauth::send(&app, "PUT", "/api/me/preferences", &token, "{}").await;
        assert_eq!(done.status, 200, "{i}");
    }
    let before = rows(&pool).await;
    let cookie = oauth::session_cookie(&token);
    for path in ["/api/auth/logout", "/api/auth/logout-all"] {
        let limited = post(app.addr, path, Some(&cookie)).await;
        assert_try_again(&limited, "private, no-store");
        assert!(limited.set_cookies().is_empty(), "{path}");
    }
    assert_eq!(rows(&pool).await, before);
    for live in [&token, &other] {
        let hash = TokenHash::of(live);
        assert!(sessions::validate(&pool, &hash).await.unwrap().is_some());
    }
}

// --- The database down (503) ------------------------------------------------------------

async fn app_via_proxy(db: &TestDb, proxy: &PgProxy) -> (RunningApp, &'static str) {
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
    let session_name = config.cookies.session_name();
    (support::app::start(state).await, session_name)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_navigation_route_with_the_database_down_is_try_again() {
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let (app, session_name) = app_via_proxy(&db, &proxy).await;
    let session = format!("{session_name}={token}");
    proxy.cut();

    let start = post(app.addr, "/api/auth/google/start", None).await;
    assert_try_again(&start, "no-store");
    assert!(start.set_cookies().is_empty(), "{:?}", start.set_cookies());

    let link = post(app.addr, "/api/auth/link/discord", Some(&session)).await;
    assert_eq!(link.header("location"), Some(TRY_AGAIN), "{}", link.text());
    assert_eq!(link.status, 303);
    assert!(link.body.is_empty());
    assert_eq!(link.header("retry-after"), None);
    // It read the session (ADR-0008 D5): `private, no-store`, a superset of the amendment's.
    assert_eq!(link.header("cache-control"), Some("private, no-store"));
    assert!(link.set_cookies().is_empty(), "{:?}", link.set_cookies());

    let flow_name = support::config::config(&[]).cookies.flow_name();
    let flow = format!("{flow_name}=c29tZS1mbG93");
    let callback = http::send(
        app.addr,
        "GET",
        "/api/auth/google/callback?code=x&state=y",
        &[("cookie", flow.as_str())],
    )
    .await;
    assert_try_again(&callback, "no-store");
    assert_eq!(callback.cookie(flow_name), Some(""));

    for path in ["/api/auth/logout", "/api/auth/logout-all"] {
        let out = post(app.addr, path, Some(&session)).await;
        assert_try_again(&out, "private, no-store");
        assert!(out.set_cookies().is_empty(), "{path}");
    }
    proxy.restore();
    let hash = TokenHash::of(&token);
    assert!(sessions::validate(&direct, &hash).await.unwrap().is_some());
}

// --- `link` without a session -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn link_without_a_session_goes_to_the_login_page() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    // A session that was revoked: logged in, then out.
    let revoked = oauth::session_of(&oauth::login(&app, Kind::Google, "g-sub-revoked").await);
    let cookie = oauth::session_cookie(&revoked);
    let out = post(app.addr, "/api/auth/logout", Some(&cookie)).await;
    assert_eq!(out.header("location"), Some("/"));
    let flows = count(&pool, "SELECT count(*) FROM oauth_flows").await;

    for session in [None, Some(revoked.as_str()), Some("dead")] {
        let refused = oauth::begin(&app, "/api/auth/link/discord", "next=%2Fcuenta", session).await;
        assert_eq!(refused.status, 303, "{session:?}: {}", refused.text());
        assert_eq!(refused.header("location"), Some("/login"), "{session:?}");
        assert!(refused.body.is_empty());
        assert_eq!(refused.header("content-type"), None);
        assert_eq!(refused.cookie(oauth::session_name()), Some(""));
        // The session check runs before the flow: no flow cookie and no row (security-reviewer).
        assert_eq!(refused.cookie(oauth::flow_name()), None, "{session:?}");
        assert_eq!(
            refused.set_cookies().len(),
            1,
            "{:?}",
            refused.set_cookies()
        );
        assert_eq!(
            refused.header("cache-control"),
            Some("private, no-store"),
            "{session:?}"
        );
    }
    assert_eq!(
        count(&pool, "SELECT count(*) FROM oauth_flows").await,
        flows
    );
}

// --- What stays JSON --------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_provider_and_a_foreign_origin_stay_json() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, "g-sub-json").await);
    let cookie = oauth::session_cookie(&token);
    app.refill();
    for (path, cookie) in [
        ("/api/auth/twitter/start", None),
        ("/api/auth/link/twitter", Some(cookie.as_str())),
    ] {
        let missing = post(app.addr, path, cookie).await;
        assert_eq!(missing.status, 404, "{path}");
        assert_eq!(missing.text(), r#"{"error":"http.not_found"}"#);
    }
    let missing = http::get(app.addr, "/api/auth/twitter/callback?code=x&state=y").await;
    assert_eq!(missing.status, 404);
    for path in [
        "/api/auth/google/start",
        "/api/auth/link/discord",
        "/api/auth/logout",
        "/api/auth/logout-all",
    ] {
        let foreign = http::send_body(
            app.addr,
            "POST",
            path,
            &[
                ("origin", "https://evil.example"),
                ("content-type", "application/x-www-form-urlencoded"),
                ("cookie", cookie.as_str()),
            ],
            "",
        )
        .await;
        assert_eq!(foreign.status, 403, "{path}");
        assert_eq!(foreign.text(), r#"{"error":"auth.csrf_origin"}"#);
    }
}
