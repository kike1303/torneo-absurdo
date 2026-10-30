//! 008 RF-C4, RF-C5, RF-C8 (T9 part, T20): the rename and the preferences over HTTP. The unique
//! index decides a taken name (no prior "exists?"), every rejected body is 422
//! `http.invalid_body` with nothing of axum or serde, and no mutating route ever writes `role`
//! or `banned_at`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use server::app::{AppState, Settings};
use server::http::routes::{Effect, ROUTES, RouteMethod};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::app::{AlwaysUp, ORIGIN};
use support::db::TestDb;
use support::http;
use support::oauth;
use support::pg_proxy::PgProxy;
use tokio::sync::Barrier;
use uuid::Uuid;

const NAME: &str = "/api/me/display-name";
const PREFERENCES: &str = "/api/me/preferences";

async fn put(addr: std::net::SocketAddr, path: &str, token: &str, body: &str) -> http::Response {
    put_as(addr, path, token, "application/json", body).await
}

async fn put_as(
    addr: std::net::SocketAddr,
    path: &str,
    token: &str,
    content_type: &str,
    body: &str,
) -> http::Response {
    let cookie = oauth::session_cookie(token);
    http::send_body(
        addr,
        "PUT",
        path,
        &[
            ("origin", ORIGIN),
            ("cookie", cookie.as_str()),
            ("content-type", content_type),
        ],
        body,
    )
    .await
}

fn rename(name: &str) -> String {
    serde_json::json!({ "display_name": name }).to_string()
}

fn json(response: &http::Response) -> serde_json::Value {
    serde_json::from_str(&response.text()).unwrap()
}

/// A user with a name and a session.
async fn named(pool: &PgPool, name: &str, key: &str) -> (Uuid, String) {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, display_name, display_name_key) VALUES ($1, $2, $3)")
        .bind(user)
        .bind(name)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    server::ledger::create_user_account(&mut tx, user)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (user, support::app::session_for(pool, user).await)
}

async fn manual_app(db: &TestDb) -> (support::auth::App, PgPool) {
    support::auth::start_app(db, Settings::default()).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_valid_rename_answers_the_profile() {
    let db = TestDb::create().await;
    let (app, pool) = manual_app(&db).await;
    let (user, token) = named(&pool, "Generado Uno", "generado uno").await;
    let done = put(app.addr, NAME, &token, &rename("  Tigre Veloz  ")).await;
    assert_eq!(done.status, 200, "{}", done.text());
    assert_eq!(done.header("cache-control"), Some("private, no-store"));
    let me = json(&done);
    assert_eq!(me["display_name"], "Tigre Veloz");
    assert_eq!(me["display_name_is_generated"], false);
    let row = sqlx::query(
        "SELECT display_name, display_name_key, display_name_is_generated FROM users WHERE id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "Tigre Veloz");
    assert_eq!(
        row.get::<String, _>(1),
        server::profile::name::validate("Tigre Veloz")
            .unwrap()
            .key()
    );
    assert!(!row.get::<bool, _>(2));
    // The same name in another capitalization is the same row: no collision.
    let again = put(app.addr, NAME, &token, &rename("TIGRE veloz")).await;
    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(json(&again)["display_name"], "TIGRE veloz");
}

/// T20 over HTTP: each refused name with its code.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refused_names_have_their_code() {
    let db = TestDb::create().await;
    let (app, pool) = manual_app(&db).await;
    let taken = server::profile::name::validate("Torneo_King").unwrap();
    named(&pool, taken.as_str(), taken.key()).await;
    let (user, token) = named(&pool, "Original Name", "original name").await;
    for (name, status, code) in [
        ("ab", 422, "profile.display_name_invalid"),
        ("   ", 422, "profile.display_name_invalid"),
        ("a\u{200B}bcd", 422, "profile.display_name_invalid"),
        (&"x".repeat(25), 422, "profile.display_name_invalid"),
        ("admin", 422, "profile.display_name_reserved"),
        ("Torneo·Absurdo", 422, "profile.display_name_reserved"),
        ("Torneo_King", 409, "profile.display_name_taken"),
        // A homoglyph of the taken one (Cyrillic о).
        ("T\u{43E}rneo_King", 409, "profile.display_name_taken"),
        ("TORNEO_KING", 409, "profile.display_name_taken"),
    ] {
        let refused = put(app.addr, NAME, &token, &rename(name)).await;
        assert_eq!(refused.status, status, "{name:?}");
        assert_eq!(
            refused.text(),
            format!(r#"{{"error":"{code}"}}"#),
            "{name:?}"
        );
        assert_eq!(refused.header("cache-control"), Some("private, no-store"));
    }
    let name: String = sqlx::query("SELECT display_name FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(name, "Original Name");
}

/// T20: of two renames to the same name at once, exactly one wins.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn of_two_renames_at_once_exactly_one_wins() {
    let db = TestDb::create().await;
    let (app, pool) = manual_app(&db).await;
    let (_, a) = named(&pool, "Primero Uno", "primero uno").await;
    let (_, b) = named(&pool, "Segundo Dos", "segundo dos").await;
    let barrier = Arc::new(Barrier::new(2));
    let tasks: Vec<_> = [a, b]
        .into_iter()
        .map(|token| {
            let barrier = barrier.clone();
            let addr = app.addr;
            tokio::spawn(async move {
                barrier.wait().await;
                put(addr, NAME, &token, &rename("Mismo Nombre")).await
            })
        })
        .collect();
    let mut statuses = Vec::new();
    for task in tasks {
        statuses.push(task.await.unwrap().status.as_u16());
    }
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 409]);
    let holders: i64 =
        sqlx::query("SELECT count(*) FROM users WHERE display_name = 'Mismo Nombre'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert_eq!(holders, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preferences_tell_absent_from_null() {
    let db = TestDb::create().await;
    let (app, pool) = manual_app(&db).await;
    let (user, token) = named(&pool, "Con Gustos", "con gustos").await;
    let mut expected = (serde_json::Value::Null, serde_json::Value::Null);
    for (body, locale, theme) in [
        ("{}", None, None),
        (r#"{"locale":"es"}"#, Some("es"), None),
        (r#"{"theme":"dark"}"#, None, Some("dark")),
        (
            r#"{"locale":"en","theme":"system"}"#,
            Some("en"),
            Some("system"),
        ),
        (r#"{"locale":null}"#, Some(""), None),
        (r#"{"theme":null,"locale":"es"}"#, Some("es"), Some("")),
    ] {
        let set = |v: Option<&str>, old: &serde_json::Value| match v {
            None => old.clone(),
            Some("") => serde_json::Value::Null,
            Some(value) => serde_json::Value::from(value),
        };
        expected = (set(locale, &expected.0), set(theme, &expected.1));
        let done = put(app.addr, PREFERENCES, &token, body).await;
        assert_eq!(done.status, 200, "{body}: {}", done.text());
        assert_eq!(done.header("cache-control"), Some("private, no-store"));
        let me = json(&done);
        assert_eq!(
            (me["locale"].clone(), me["theme"].clone()),
            expected,
            "{body}"
        );
    }
    let row = sqlx::query("SELECT locale, theme FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<String>, _>(0).as_deref(), Some("es"));
    assert_eq!(row.get::<Option<String>, _>(1), None);
}

/// RF-C5 amendment: every rejected body is the same 422 with our code and nothing else, for
/// both routes. The canary of a broken JSON never reaches the answer (nor the log: that half is
/// in `tests/logs_account.rs`, which captures the log in its own binary).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_rejected_body_is_invalid_body() {
    let canary = format!("c4n4ry{}", Uuid::new_v4().simple());
    let db = TestDb::create().await;
    let (app, pool) = manual_app(&db).await;
    let (user, token) = named(&pool, "Cuerpo Malo", "cuerpo malo").await;
    for (path, content_type, body) in rejected_bodies(&canary) {
        let refused = put_as(app.addr, path, &token, content_type, &body).await;
        assert_eq!(refused.status, 422, "{path} {content_type} {}", body.len());
        assert_eq!(refused.text(), r#"{"error":"http.invalid_body"}"#);
        assert_eq!(refused.header("content-type"), Some("application/json"));
        assert_eq!(refused.header("cache-control"), Some("private, no-store"));
        assert!(!refused.text().contains("c4n4ry"));
    }
    let row = sqlx::query("SELECT display_name, locale, theme FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "Cuerpo Malo");
    assert_eq!(row.get::<Option<String>, _>(1), None);
    assert_eq!(row.get::<Option<String>, _>(2), None);
}

/// Malformed JSON (with the canary), unknown fields, a wrong type, a value outside the enum, a
/// `Content-Type` that is not JSON and bodies over 4 KB.
fn rejected_bodies(canary: &str) -> Vec<(&'static str, &'static str, String)> {
    let too_large = format!(r#"{{"display_name":"{}"}}"#, "x".repeat(4097 - 19));
    assert_eq!(too_large.len(), 4097);
    vec![
        (
            NAME,
            "application/json",
            format!(r#"{{"display_name": "{canary}"#),
        ),
        (
            PREFERENCES,
            "application/json",
            format!(r#"{{"locale": "{canary}"#),
        ),
        (
            NAME,
            "application/json",
            format!(r#"{{"display_name":"Bueno Bueno","{canary}":1}}"#),
        ),
        (
            PREFERENCES,
            "application/json",
            format!(r#"{{"theme":"dark","{canary}":1}}"#),
        ),
        (
            PREFERENCES,
            "application/json",
            r#"{"locale":"fr"}"#.to_owned(),
        ),
        (NAME, "application/json", r#"{"display_name":5}"#.to_owned()),
        (NAME, "text/plain", rename("Bueno Bueno")),
        (
            PREFERENCES,
            "application/x-www-form-urlencoded",
            "locale=es".to_owned(),
        ),
        (NAME, "application/json", too_large),
        (
            PREFERENCES,
            "application/json",
            format!(r#"{{"locale":null,"x":"{}"}}"#, "y".repeat(4100)),
        ),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_routes_need_a_session_and_the_database() {
    let db = TestDb::create().await;
    let (app, _pool) = manual_app(&db).await;
    for path in [NAME, PREFERENCES] {
        let refused = http::send_body(
            app.addr,
            "PUT",
            path,
            &[("origin", ORIGIN), ("content-type", "application/json")],
            "{}",
        )
        .await;
        assert_eq!(refused.status, 401, "{path}");
        assert_eq!(refused.text(), r#"{"error":"auth.unauthenticated"}"#);
        // The Origin check of phase A covers PUT.
        let cookie = oauth::session_cookie("x");
        let forbidden = http::send_body(
            app.addr,
            "PUT",
            path,
            &[
                ("cookie", cookie.as_str()),
                ("content-type", "application/json"),
            ],
            "{}",
        )
        .await;
        assert_eq!(forbidden.status, 403, "{path}");
    }
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
    let down = support::app::start(state).await;
    proxy.cut();
    for (path, body) in [(NAME, rename("Sin Base")), (PREFERENCES, "{}".to_owned())] {
        let failed = put(down.addr, path, &token, &body).await;
        assert_eq!(failed.status, 503, "{path}");
        assert_eq!(failed.text(), r#"{"error":"auth.try_again"}"#);
    }
}

/// The "other mutations" quota per user (D7): 60 per minute.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_user_quota_answers_429() {
    let db = TestDb::create().await;
    let (app, pool) = manual_app(&db).await;
    let (_, token) = named(&pool, "Muy Activo", "muy activo").await;
    for i in 0..60 {
        let done = put(app.addr, PREFERENCES, &token, "{}").await;
        assert_eq!(done.status, 200, "{i}");
    }
    let limited = put(app.addr, NAME, &token, &rename("Otro Nombre")).await;
    assert_eq!(limited.status, 429);
    assert_eq!(limited.text(), r#"{"error":"http.rate_limited"}"#);
    assert!(limited.header("retry-after").is_some());
}

/// RF-C8 (T9): no route of the manifest that mutates writes `role` or `banned_at`, whatever its
/// body says. A new route is covered without touching this test; each one gets a fresh session
/// (`logout` and `logout-all` revoke theirs).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_mutating_route_writes_role_or_banned_at() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, _) = named(&pool, "Nunca Admin", "nunca admin").await;
    sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) \
         VALUES ('google', 'g-rfc8', $1), ('discord', '123456789', $1)",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let json = r#"{"display_name":"Nunca Admin","role":"admin","banned_at":"2026-10-29T00:00:00Z","locale":"es"}"#;
    let form = "role=admin&banned_at=2026-10-29T00%3A00%3A00Z&next=%2F";
    let mut checked = 0;
    for route in ROUTES.iter().filter(|r| r.effect == Effect::Mutate) {
        let token = support::app::session_for(&pool, user).await;
        app.refill();
        let path = route.path.replace("{provider}", "google");
        let cookie = oauth::session_cookie(&token);
        let (content_type, body) = match route.method {
            RouteMethod::Put | RouteMethod::Patch => ("application/json", json),
            _ => ("application/x-www-form-urlencoded", form),
        };
        let response = http::send_body(
            app.addr,
            route.method.http().as_str(),
            &path,
            &[
                ("origin", ORIGIN),
                ("cookie", cookie.as_str()),
                ("content-type", content_type),
            ],
            body,
        )
        .await;
        assert_ne!(response.status, 405, "{route:?}");
        let row = sqlx::query("SELECT role, banned_at IS NULL FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>(0), "user", "{route:?}");
        assert!(row.get::<bool, _>(1), "{route:?}");
        checked += 1;
    }
    // start, link, unlink, callback, logout, logout-all and the two PUTs (and two dev routes).
    assert!(checked >= 8, "{checked}");
}
