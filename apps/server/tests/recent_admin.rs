//! 008 RF-C7 (plan-c decision 9) and 009 RF16: `RecentAdmin` lets through an admin whose
//! authentication is current, of the last 60 minutes and with an account inside the cutoff of
//! the last `grant`, and only that. There is no admin route in this feature, so the test builds
//! its own `Router` with one route that only extracts it, over a real `AppState` on the database
//! of the app with the fake providers; no test route ever reaches the binary.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use server::admin::{AdminAction, AdminCommand};
use server::app::{AppState, Settings};
use server::http::admin::RecentAdmin;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::app::AlwaysUp;
use support::db::{TestDb, privileged};
use support::fake_provider::Kind;
use support::oauth;
use tower::ServiceExt;
use uuid::Uuid;

async fn only_recent_admins(_admin: RecentAdmin) -> StatusCode {
    StatusCode::NO_CONTENT
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/test/recent-admin", get(only_recent_admins))
        .with_state(state)
}

/// `(status, body, cache-control)` of `GET /test/recent-admin` with `token`.
async fn call(state: &AppState, token: Option<&str>) -> (StatusCode, String, Option<String>) {
    let mut request = Request::builder().uri("/test/recent-admin");
    if let Some(token) = token {
        let name = state.auth.cookies.session_name();
        request = request.header("cookie", format!("{name}={token}"));
    }
    let response = router(state.clone())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cache = response
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_owned());
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap(), cache)
}

async fn user(pool: &PgPool, role: &str) -> Uuid {
    let id = Uuid::new_v4();
    privileged(
        pool,
        sqlx::query(
            "INSERT INTO users (id, display_name, display_name_key, role) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(format!("Test{}", id.simple()))
        .bind(format!("test{}", id.simple()))
        .bind(role),
    )
    .await;
    id
}

async fn session(pool: &PgPool, user: Uuid, role: protocol::Role) -> String {
    let mut tx = pool.begin().await.unwrap();
    let issued = server::sessions::issue(
        &mut tx,
        user,
        role,
        server::sessions::Authentication::None,
        &[0; 32],
        server::sessions::UaFamily::Other,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    issued.token.as_str().to_owned()
}

/// The state of the test router: the database of `db`, the config of the app (same cookies).
fn state_on(db: &TestDb, pool: &PgPool) -> AppState {
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    AppState::from_config(
        &config,
        &Settings::default(),
        pool.clone(),
        Arc::new(AlwaysUp),
    )
    .unwrap()
}

async fn user_of(pool: &PgPool, token: &str) -> Uuid {
    sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1")
        .bind(server::sessions::TokenHash::of(token).0.as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// `server admin grant`, in process (the binary has its own tests in `admin_cli.rs`).
async fn grant(db: &TestDb, user: Uuid) {
    let command = AdminCommand {
        action: AdminAction::Grant,
        user,
        reason: None,
    };
    server::admin::run(&db.options(), &command).await.unwrap();
}

fn sub() -> String {
    format!("g-{}", Uuid::new_v4().simple())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_an_admin_who_authenticated_in_the_last_hour_passes() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let state = state_on(&db, &pool);
    let private = Some("private, no-store".to_owned());

    let (status, body, cache) = call(&state, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, r#"{"error":"auth.unauthenticated"}"#);
    assert_eq!(cache, private);

    let plain = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let (status, body, cache) = call(&state, Some(&plain)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, r#"{"error":"auth.reauth_required"}"#);
    assert_eq!(cache, private);

    let identity = sub();
    let before = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let admin = user_of(&pool, &before).await;
    grant(&db, admin).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let (status, _, _) = call(&state, Some(&token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    sqlx::query(
        "UPDATE sessions SET authenticated_at = now() - interval '61 minutes' \
         WHERE user_id = $1",
    )
    .bind(admin)
    .execute(&pool)
    .await
    .unwrap();
    let (status, body, cache) = call(&state, Some(&token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, r#"{"error":"auth.reauth_required"}"#);
    assert_eq!(cache, private);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_database_down_it_is_try_again() {
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let admin = user(&direct, "admin").await;
    let token = session(&direct, admin, protocol::Role::Admin).await;
    let proxy = support::pg_proxy::PgProxy::start(db.server_addr()).await;
    let url = db.url_via(proxy.addr());
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(config.database.clone());
    let state =
        AppState::from_config(&config, &Settings::default(), pool, Arc::new(AlwaysUp)).unwrap();
    proxy.cut();
    let (status, body, cache) = call(&state, Some(&token)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, r#"{"error":"auth.try_again"}"#);
    assert_eq!(cache, Some("private, no-store".to_owned()));
}
