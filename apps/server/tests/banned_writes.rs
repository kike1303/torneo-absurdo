//! A ban that commits after a request checked its session, and before the request's own write,
//! still stops the write (QA on phase C): `unlink` and the two `PUT /api/me/…` re-read the user
//! with `banned_at IS NULL` in the statement that locks or writes it, as `link` does. The test
//! holds the ban uncommitted (with the row locked) until the request waits on that lock in
//! Postgres, then commits it; the request has to answer 401 `auth.unauthenticated` without
//! changing anything.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use sqlx::{PgPool, Postgres, Row, Transaction};
use support::app::ORIGIN;
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use uuid::Uuid;

/// A ban of `user` in an open transaction (the CLI's actor, the row locked until commit).
async fn pending_ban(pool: &PgPool, user: Uuid) -> Transaction<'static, Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('torneo.actor', 'cli', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1")
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

/// Waits until one backend of this database waits on a lock.
async fn one_waiting(pool: &PgPool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let waiting: i64 = sqlx::query(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0);
        if waiting == 1 {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "{waiting} waiting");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn user_of(pool: &PgPool, token: &str) -> Uuid {
    sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1")
        .bind(server::sessions::TokenHash::of(token).0.as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// Sends `method path body` with the session `token`, with the ban committed in the middle.
async fn with_a_ban_in_between(
    app: &oauth::App,
    pool: &PgPool,
    user: Uuid,
    method: &'static str,
    path: &'static str,
    token: &str,
    body: &'static str,
) -> http::Response {
    let ban = pending_ban(pool, user).await;
    let cookie = oauth::session_cookie(token);
    let addr = app.addr;
    let request = tokio::spawn(async move {
        http::send_body(
            addr,
            method,
            path,
            &[
                ("origin", ORIGIN),
                ("cookie", cookie.as_str()),
                ("content-type", "application/json"),
            ],
            body,
        )
        .await
    });
    one_waiting(pool).await;
    ban.commit().await.unwrap();
    request.await.unwrap()
}

fn assert_unauthenticated(response: &http::Response) {
    assert_eq!(response.status, 401, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"auth.unauthenticated"}"#);
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ban_during_an_unlink_stops_it() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, "g-banned-unlink").await);
    let token =
        oauth::session_of(&oauth::link(&app, Kind::Discord, "180351110224678913", &first).await)
            .to_owned();
    let user = user_of(&pool, &token).await;
    let response = with_a_ban_in_between(
        &app,
        &pool,
        user,
        "POST",
        "/api/auth/unlink/google",
        &token,
        "",
    )
    .await;
    assert_unauthenticated(&response);
    let providers: i64 = sqlx::query("SELECT count(*) FROM oauth_accounts WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(providers, 2);
    let events: i64 = sqlx::query("SELECT count(*) FROM auth_events WHERE kind = 'unlink'")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(events, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ban_during_a_rename_or_a_preference_change_stops_it() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    for (path, body) in [
        (
            "/api/me/display-name",
            r#"{"display_name":"Nombre Baneado"}"#,
        ),
        ("/api/me/preferences", r#"{"locale":"es","theme":"dark"}"#),
    ] {
        let (user, token) = support::app::user_with_session(&pool).await;
        let before = sqlx::query("SELECT display_name, locale, theme FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
        let response = with_a_ban_in_between(&app, &pool, user, "PUT", path, &token, body).await;
        assert_unauthenticated(&response);
        let after = sqlx::query("SELECT display_name, locale, theme FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            after.get::<String, _>(0),
            before.get::<String, _>(0),
            "{path}"
        );
        assert_eq!(after.get::<Option<String>, _>(1), None, "{path}");
        assert_eq!(after.get::<Option<String>, _>(2), None, "{path}");
    }
}
