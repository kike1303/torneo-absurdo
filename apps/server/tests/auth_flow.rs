//! 008 RF-B3 to RF-B6, RF-B11 to RF-B13 with the dev provider: T4 (adapted by the 2026-10-28
//! amendment), T5, T10 (rotation and logout), T19 and T20 (forced alias).
#![cfg(feature = "dev-auth")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use protocol::Provider;
use server::app::{AppState, Settings};
use server::auth::login::{self, Identity, LoginError};
use server::profile::alias::{AliasConfig, AliasSource};
use server::sessions::{self, TokenHash, UaFamily};
use sqlx::{PgPool, Row};
use support::app::{AlwaysUp, ORIGIN};
use support::auth::{self as flow, FLOW, SESSION};
use support::db::TestDb;
use support::http;

async fn app(db: &TestDb) -> (flow::App, PgPool) {
    flow::start_app(db, Settings::default()).await
}

async fn count(pool: &PgPool, table: &'static str) -> i64 {
    let sql = match table {
        "users" => "SELECT count(*) FROM users",
        "ledger_entries" => "SELECT count(*) FROM ledger_entries",
        "sessions" => "SELECT count(*) FROM sessions",
        "oauth_accounts" => "SELECT count(*) FROM oauth_accounts",
        "oauth_flows" => "SELECT count(*) FROM oauth_flows",
        "auth_events" => "SELECT count(*) FROM auth_events",
        _ => unreachable!(),
    };
    sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
}

/// A callback error: 303 to `/login?error=<code>`, no session, the flow cookie expired,
/// `no-store`, and nothing of the request in the `Location`.
fn assert_callback_error(response: &http::Response, code: &str) {
    assert_eq!(response.status, 303, "{}", response.text());
    assert_eq!(
        response.header("location"),
        Some(format!("/login?error={code}").as_str())
    );
    assert_eq!(response.cookie(FLOW), Some(""));
    assert!(
        response.cookie(SESSION).is_none(),
        "{:?}",
        response.set_cookies()
    );
    assert_eq!(response.header("cache-control"), Some("no-store"));
}

// --- The happy path ---------------------------------------------------------------------

/// The dev identity of the first-login test.
const IDENTITY: &str = "ana_bel-7";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_first_login_creates_the_user_the_grant_and_a_session() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let done = flow::login(&app, IDENTITY, Some("/es/onboarding"), None).await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.header("location"), Some("/es/onboarding"));
    assert_eq!(done.header("cache-control"), Some("no-store"));
    assert_eq!(done.cookie(FLOW), Some(""));
    let token = flow::session_of(&done);
    let line = done
        .set_cookies()
        .into_iter()
        .find(|l| l.starts_with(SESSION))
        .unwrap();
    assert_eq!(
        line,
        format!("{SESSION}={token}; Max-Age=2592000; Path=/; HttpOnly; SameSite=Lax")
    );

    let me = flow::me(app.addr, &token).await;
    assert_eq!(me.status, 200, "{}", me.text());
    assert_eq!(me.header("cache-control"), Some("private, no-store"));
    let body: serde_json::Value = serde_json::from_str(&me.text()).unwrap();
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "balance",
            "display_name",
            "display_name_is_generated",
            "locale",
            "providers",
            "role",
            "theme"
        ]
    );
    assert_eq!(body["balance"], 100);
    assert_eq!(body["providers"], serde_json::json!(["dev"]));
    assert_eq!(body["role"], "user");
    assert_eq!(body["display_name_is_generated"], true);
    // T19: the alias is generated, never copied from the provider. The identity has `_` and
    // `-`, which no alias has, so an alias can never contain it by chance (with `ana` it did:
    // Iguana, Manzana, Campana…).
    let name = body["display_name"].as_str().unwrap();
    assert!(!name.to_lowercase().contains(IDENTITY), "{name}");
    // A sanitized copy of the identity (`ana_bel-7` without separators) is not an alias either.
    assert!(!name.to_lowercase().contains("anabel7"), "{name}");
    assert!(name.chars().all(|c| c.is_ascii_alphanumeric()), "{name}");
    assert!(server::profile::name::validate(name).is_ok());

    assert_eq!(count(&pool, "users").await, 1);
    assert_eq!(count(&pool, "ledger_entries").await, 1);
    assert_eq!(count(&pool, "oauth_flows").await, 0);
    let kinds: Vec<String> = sqlx::query("SELECT kind FROM auth_events ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(kinds, ["grant", "login"]);
    let lens: Vec<i32> = sqlx::query("SELECT length(ip_hash) FROM auth_events")
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(lens, [32, 32]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_login_is_the_same_user_without_a_new_grant() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let first = flow::session_of(&flow::login(&app, "ana", None, None).await);
    let name = |me: http::Response| {
        let body: serde_json::Value = serde_json::from_str(&me.text()).unwrap();
        (body["display_name"].clone(), body["balance"].clone())
    };
    let before = name(flow::me(app.addr, &first).await);
    let second = flow::login(&app, "ana", None, None).await;
    assert_eq!(second.header("location"), Some("/"));
    let after = name(flow::me(app.addr, &flow::session_of(&second)).await);
    assert_eq!(before, after);
    assert_eq!(count(&pool, "users").await, 1);
    assert_eq!(count(&pool, "ledger_entries").await, 1);
    // Without a cookie in the request, the first session is not rotated.
    assert_eq!(count(&pool, "sessions").await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn providers_and_unknown_providers() {
    let db = TestDb::create().await;
    let (app, _pool) = app(&db).await;
    let list = http::get(app.addr, "/api/auth/providers").await;
    assert_eq!(list.status, 200);
    assert_eq!(list.text(), r#"["dev"]"#);
    for provider in ["google", "discord", "nope", "DEV"] {
        // The AUTH_IP bucket applies before the provider is looked up: refill it.
        app.clock.advance(Duration::from_secs(60));
        let path = format!("/api/auth/{provider}/start");
        let response = http::send_body(
            app.addr,
            "POST",
            &path,
            &[
                ("origin", ORIGIN),
                ("content-type", "application/x-www-form-urlencoded"),
            ],
            "dev_identity=ana",
        )
        .await;
        assert_eq!(response.status, 404, "{provider}");
        assert_eq!(response.text(), r#"{"error":"http.not_found"}"#);
        let path = format!("/api/auth/{provider}/callback?code=x&state=y");
        assert_eq!(http::get(app.addr, &path).await.status, 404, "{provider}");
    }
}

// --- T4 adapted and T5 ------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_bad_callback_is_303_invalid_state_without_a_session() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let fresh = || async {
        let started = flow::start(&app, &[("dev_identity", "ana")], None).await;
        let flow_id = started.cookie(FLOW).unwrap().to_owned();
        let authorized = flow::authorize(&app, &started).await;
        (flow_id, authorized.location_path())
    };

    // No flow cookie.
    let (_, path) = fresh().await;
    assert_callback_error(
        &flow::callback(&app, &path, &[]).await,
        "auth.invalid_state",
    );
    // No state.
    let (id, path) = fresh().await;
    let no_state = path.split("&state=").next().unwrap().to_owned();
    let cookie = vec![format!("{FLOW}={id}")];
    assert_callback_error(
        &flow::callback(&app, &no_state, &cookie).await,
        "auth.invalid_state",
    );
    // Another state.
    let (id, path) = fresh().await;
    let other = format!(
        "{}&state=AAAAAAAAAAAAAAAAAAAAAA",
        path.split("&state=").next().unwrap()
    );
    let cookie = vec![format!("{FLOW}={id}")];
    assert_callback_error(
        &flow::callback(&app, &other, &cookie).await,
        "auth.invalid_state",
    );
    // The flow of another cookie.
    let (_, path) = fresh().await;
    let (other_id, _) = fresh().await;
    let cookie = vec![format!("{FLOW}={other_id}")];
    assert_callback_error(
        &flow::callback(&app, &path, &cookie).await,
        "auth.invalid_state",
    );
    // Reused: the first callback consumes the flow.
    let (id, path) = fresh().await;
    let cookie = vec![format!("{FLOW}={id}")];
    assert_eq!(flow::callback(&app, &path, &cookie).await.status, 303);
    assert_callback_error(
        &flow::callback(&app, &path, &cookie).await,
        "auth.invalid_state",
    );
    // Expired.
    let (id, path) = fresh().await;
    sqlx::query("UPDATE oauth_flows SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    let cookie = vec![format!("{FLOW}={id}")];
    assert_callback_error(
        &flow::callback(&app, &path, &cookie).await,
        "auth.invalid_state",
    );
    // Of another provider.
    let (id, path) = fresh().await;
    sqlx::query("UPDATE oauth_flows SET provider = 'google'")
        .execute(&pool)
        .await
        .unwrap();
    let cookie = vec![format!("{FLOW}={id}")];
    assert_callback_error(
        &flow::callback(&app, &path, &cookie).await,
        "auth.invalid_state",
    );
    // Only the one good callback created a session.
    assert_eq!(count(&pool, "sessions").await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_external_next_redirects_to_the_root() {
    let db = TestDb::create().await;
    let (app, _pool) = app(&db).await;
    for next in [
        "https://evil.example/",
        "//evil.example",
        "/\\evil.example",
        "evil",
        "/a\u{1}b",
    ] {
        let done = flow::login(&app, "ana", Some(next), None).await;
        assert_eq!(done.header("location"), Some("/"), "{next:?}");
    }
    let done = flow::login(&app, "ana", Some("/es/cuenta"), None).await;
    assert_eq!(done.header("location"), Some("/es/cuenta"));
}

/// A stored `next` that is not a valid header value (a flow written before `sanitize` refused
/// DEL) never breaks the response after the login committed: it falls back to `/`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stored_next_that_cannot_be_a_location_falls_back_to_the_root() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let started = flow::start(&app, &[("dev_identity", "ana")], None).await;
    let id = started.cookie(FLOW).unwrap().to_owned();
    let authorized = flow::authorize(&app, &started).await;
    sqlx::query("UPDATE oauth_flows SET next = '/a' || chr(127) || 'b'")
        .execute(&pool)
        .await
        .unwrap();
    let done = flow::callback(&app, &authorized.location_path(), &[format!("{FLOW}={id}")]).await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.header("location"), Some("/"));
    let token = flow::session_of(&done);
    assert_eq!(flow::me(app.addr, &token).await.status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_invalid_dev_identity_never_starts_a_flow() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    for identity in ["", "Ana", "a b", &"a".repeat(33), "ana;x", "é"] {
        let started = flow::start(&app, &[("dev_identity", identity)], None).await;
        assert_callback_error(&started, "auth.invalid_state");
    }
    let started = flow::start(&app, &[], None).await;
    assert_callback_error(&started, "auth.invalid_state");
    assert_eq!(count(&pool, "oauth_flows").await, 0);
}

// --- Each callback error code -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_banned_user_gets_auth_banned_and_no_session() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    flow::login(&app, "ana", None, None).await;
    support::db::privileged(&pool, sqlx::query("UPDATE users SET banned_at = now()")).await;
    let before = count(&pool, "sessions").await;
    assert_callback_error(&flow::login(&app, "ana", None, None).await, "auth.banned");
    assert_eq!(count(&pool, "sessions").await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tampered_code_verifier_is_a_provider_error() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let started = flow::start(&app, &[("dev_identity", "ana")], None).await;
    let id = started.cookie(FLOW).unwrap().to_owned();
    let authorized = flow::authorize(&app, &started).await;
    sqlx::query("UPDATE oauth_flows SET code_verifier = repeat('A', 43)")
        .execute(&pool)
        .await
        .unwrap();
    let done = flow::callback(&app, &authorized.location_path(), &[format!("{FLOW}={id}")]).await;
    assert_callback_error(&done, "auth.provider_error");
    assert_eq!(count(&pool, "users").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_provider_error_in_the_query_is_a_provider_error() {
    let db = TestDb::create().await;
    let (app, _pool) = app(&db).await;
    let started = flow::start(&app, &[("dev_identity", "ana")], None).await;
    let id = started.cookie(FLOW).unwrap().to_owned();
    let authorized = flow::authorize(&app, &started).await;
    let path = authorized.location_path();
    let state = path.split("&state=").nth(1).unwrap();
    let denied = format!("/api/auth/dev/callback?error=access_denied&state={state}");
    let done = flow::callback(&app, &denied, &[format!("{FLOW}={id}")]).await;
    assert_callback_error(&done, "auth.provider_error");
    // And the provider's text never reaches the Location.
    assert!(!done.header("location").unwrap().contains("access_denied"));
}

/// An alias source that hands out a fixed list, then repeats the last one.
struct Fixed(Mutex<VecDeque<String>>);

impl AliasSource for Fixed {
    fn next(&self) -> Option<String> {
        let mut list = self.0.lock().unwrap();
        if list.len() > 1 {
            list.pop_front()
        } else {
            list.front().cloned()
        }
    }
}

fn fixed(names: &[&str]) -> Settings {
    Settings {
        alias: AliasConfig::new(Arc::new(Fixed(Mutex::new(
            names.iter().map(|n| (*n).to_owned()).collect(),
        )))),
        ..Settings::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t20_a_colliding_alias_is_retried_and_ten_collisions_are_try_again() {
    let db = TestDb::create().await;
    // A user that already holds "TigreVeloz42" (by key, so "TIGREVELOZ42" collides too).
    let (app, pool) = flow::start_app(&db, fixed(&["TIGREVELOZ42", "LoboFeliz77"])).await;
    sqlx::query("INSERT INTO users (display_name, display_name_key) VALUES ($1, $2)")
        .bind("TigreVeloz42")
        .bind(server::profile::name::key_of("TigreVeloz42"))
        .execute(&pool)
        .await
        .unwrap();
    let done = flow::login(&app, "ana", None, None).await;
    let me = flow::me(app.addr, &flow::session_of(&done)).await;
    assert!(
        me.text().contains(r#""display_name":"LoboFeliz77""#),
        "{}",
        me.text()
    );
    drop(app);

    let db = TestDb::create().await;
    let (app, pool) = flow::start_app(&db, fixed(&["TigreVeloz42"])).await;
    sqlx::query("INSERT INTO users (display_name, display_name_key) VALUES ($1, $2)")
        .bind("TigreVeloz42")
        .bind(server::profile::name::key_of("TigreVeloz42"))
        .execute(&pool)
        .await
        .unwrap();
    let done = flow::login(&app, "ana", None, None).await;
    assert_callback_error(&done, "auth.try_again");
    assert_eq!(count(&pool, "users").await, 1);
    assert_eq!(count(&pool, "ledger_entries").await, 0);
    assert_eq!(count(&pool, "oauth_accounts").await, 0);
}

// --- RF-B3 concurrency and atomicity ----------------------------------------------------

fn state(db: &TestDb, pool: PgPool) -> AppState {
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_after_the_user_insert_leaves_no_user_and_no_grant() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    // Test-only fault: the ledger account (step 3) fails.
    sqlx::query(
        "CREATE FUNCTION fail_account() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN RAISE EXCEPTION USING ERRCODE = 'TA999', MESSAGE = 'injected'; END; $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_account BEFORE INSERT ON ledger_accounts \
         FOR EACH ROW EXECUTE FUNCTION fail_account()",
    )
    .execute(&pool)
    .await
    .unwrap();
    let state = state(&db, pool.clone());
    let identity = Identity {
        provider: Provider::Dev,
        subject: "ana".to_owned(),
    };
    let result = login::login(
        &state.auth,
        &state.db,
        &identity,
        [1; 32],
        UaFamily::Other,
        None,
    )
    .await;
    assert!(matches!(result, Err(LoginError::TryAgain)), "{result:?}");
    for table in [
        "users",
        "ledger_entries",
        "oauth_accounts",
        "sessions",
        "auth_events",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
}

// --- T10: rotation and logout -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_with_a_previous_session_rotates_it() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let first = flow::session_of(&flow::login(&app, "ana", None, None).await);
    let second = flow::session_of(&flow::login(&app, "bea", None, Some(&first)).await);
    assert!(
        sessions::validate(&pool, &TokenHash::of(&first))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(flow::me(app.addr, &first).await.status, 401);
    assert_eq!(flow::me(app.addr, &second).await.status, 200);
    assert_eq!(count(&pool, "sessions").await, 1);
}

async fn logout(app: &flow::App, path: &str, token: Option<&str>) -> http::Response {
    let addr = app.addr;
    let cookie = token.map(|t| format!("{SESSION}={t}"));
    let mut headers = vec![("origin", ORIGIN)];
    if let Some(cookie) = &cookie {
        headers.push(("cookie", cookie));
    }
    http::send(addr, "POST", path, &headers).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logout_and_logout_all() {
    let db = TestDb::create().await;
    let (app, pool) = app(&db).await;
    let a = flow::session_of(&flow::login(&app, "ana", None, None).await);
    let b = flow::session_of(&flow::login(&app, "ana", None, None).await);
    let c = flow::session_of(&flow::login(&app, "ana", None, None).await);

    let out = logout(&app, "/api/auth/logout", Some(&a)).await;
    assert_eq!(out.status, 303);
    assert_eq!(out.header("location"), Some("/"));
    assert_eq!(out.cookie(SESSION), Some(""));
    assert_eq!(out.header("cache-control"), Some("private, no-store"));
    let me = flow::me(app.addr, &a).await;
    assert_eq!(me.status, 401);
    assert_eq!(me.text(), r#"{"error":"auth.unauthenticated"}"#);
    assert_eq!(me.cookie(SESSION), Some(""));
    assert_eq!(flow::me(app.addr, &b).await.status, 200);

    let all = logout(&app, "/api/auth/logout-all", Some(&b)).await;
    assert_eq!(all.status, 303);
    assert_eq!(flow::me(app.addr, &b).await.status, 401);
    assert_eq!(flow::me(app.addr, &c).await.status, 401);
    assert_eq!(count(&pool, "sessions").await, 0);

    // Idempotent: without a session, or with a dead one, still 303.
    for token in [None, Some(a.as_str())] {
        for path in ["/api/auth/logout", "/api/auth/logout-all"] {
            let out = logout(&app, path, token).await;
            assert_eq!(out.status, 303, "{path} {token:?}");
            assert_eq!(out.header("location"), Some("/"));
        }
    }
    // A logout without Origin is a cross-site mutation (D4).
    let cookie = format!("{SESSION}={c}");
    let forged = http::send(app.addr, "POST", "/api/auth/logout", &[("cookie", &cookie)]).await;
    assert_eq!(forged.status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me_without_a_valid_session_is_401() {
    let db = TestDb::create().await;
    let (app, _pool) = app(&db).await;
    let none = http::get(app.addr, "/api/me").await;
    assert_eq!(none.status, 401);
    assert_eq!(none.text(), r#"{"error":"auth.unauthenticated"}"#);
    assert_eq!(none.header("cache-control"), Some("private, no-store"));
    let bad = flow::me(app.addr, "not-a-session").await;
    assert_eq!(bad.status, 401);
    assert_eq!(bad.cookie(SESSION), Some(""));
}

// --- Rate limit and Origin --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sixth_start_in_a_burst_is_429_with_retry_after_6() {
    let db = TestDb::create().await;
    let (settings, clock) = flow::with_manual_clock(Settings::default());
    let (app, _pool) = support::app::start_db(&db, &[], &settings).await;
    let form = flow::form(&[("dev_identity", "ana")]);
    let start = || {
        http::send_body(
            app.addr,
            "POST",
            "/api/auth/dev/start",
            &[
                ("origin", ORIGIN),
                ("content-type", "application/x-www-form-urlencoded"),
            ],
            &form,
        )
    };
    for i in 0..5 {
        assert_eq!(start().await.status, 303, "{i}");
    }
    let limited = start().await;
    assert_eq!(limited.status, 429);
    assert_eq!(limited.header("retry-after"), Some("6"));
    assert_eq!(limited.text(), r#"{"error":"http.rate_limited"}"#);
    clock.advance(Duration::from_secs(6));
    assert_eq!(start().await.status, 303);
    // The callback shares the bucket.
    assert_eq!(
        http::get(app.addr, "/api/auth/dev/callback?code=x&state=y")
            .await
            .status,
        429
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_dev_token_endpoint_is_exempt_from_origin() {
    let db = TestDb::create().await;
    let (app, _pool) = app(&db).await;
    let form = [("content-type", "application/x-www-form-urlencoded")];
    // The token endpoint without Origin reaches the handler (an unknown code: 400).
    let token = http::send_body(
        app.addr,
        "POST",
        "/api/auth/dev/oauth/token",
        &form,
        "grant_type=authorization_code&code=nope&code_verifier=x",
    )
    .await;
    assert_eq!(token.status, 400, "{}", token.text());
    // Any other dev route with an unsafe method and no Origin is still 403.
    for path in [
        "/api/auth/dev/oauth/authorize",
        "/api/auth/dev/oauth/userinfo",
    ] {
        let response = http::send_body(app.addr, "POST", path, &form, "").await;
        assert_eq!(response.status, 403, "{path}");
        assert_eq!(response.text(), r#"{"error":"auth.csrf_origin"}"#);
    }
    // And so is start.
    let start = http::send_body(
        app.addr,
        "POST",
        "/api/auth/dev/start",
        &form,
        "dev_identity=ana",
    )
    .await;
    assert_eq!(start.status, 403);
}
