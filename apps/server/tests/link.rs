//! 008 RF-C2 (T4, T6, T10): linking a second provider. One transaction locks the user, rotates
//! the session (and refuses if it was closed meanwhile), inserts the identity without a
//! violation, writes `auth_events` and issues the new session. No error touches the session in
//! use, and nothing ever merges two users.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use server::app::{AppState, Settings};
use server::auth::link::{LinkGate, LinkGateConfig};
use server::sessions::TokenHash;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::app::{AlwaysUp, ORIGIN};
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use support::pg_proxy::PgProxy;
use support::ws::{self, Ending};
use tokio::sync::Semaphore;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
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

fn snowflake() -> String {
    let n = u64::from_le_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    format!("{}", 100_000_000_000_000_000 + n % 900_000_000_000_000_000)
}

fn sub() -> String {
    format!("g-{}", Uuid::new_v4().simple())
}

/// An echo round trip: the socket is past its handshake and in its loop.
async fn socket(app: &oauth::App, token: &str) -> ws::Client {
    let cookie = oauth::session_cookie(token);
    let mut client = ws::connect_with(app.addr, ORIGIN, &[("cookie", cookie.as_str())])
        .await
        .unwrap();
    ws::send_text(&mut client, r#"{"type":"echo","payload":"ready"}"#).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(5)).await,
        Some(Message::Text(r#"{"type":"echo","payload":"ready"}"#.into()))
    );
    client
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn linking_rotates_the_session_and_closes_the_old_socket() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let old = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let user = user_of(&pool, &old).await;
    let mut client = socket(&app, &old).await;
    let entries = count(&pool, "SELECT count(*) FROM ledger_entries").await;

    let done = oauth::link(&app, Kind::Discord, &snowflake(), &old).await;
    assert_eq!(done.header("location"), Some("/cuenta"));
    assert_eq!(done.header("cache-control"), Some("no-store"));
    assert_eq!(done.cookie(oauth::flow_name()), Some(""));
    let new = oauth::session_of(&done);
    assert_ne!(new, old);
    // T10: the old session is dead and its socket closes at once.
    assert!(!alive(&app, &old).await);
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(5)).await,
        Ending::Close(1008, "auth.unauthenticated".into())
    );
    let me: serde_json::Value = serde_json::from_str(&oauth::me(&app, &new).await.text()).unwrap();
    assert_eq!(me["providers"], serde_json::json!(["discord", "google"]));
    assert_eq!(user_of(&pool, &new).await, user);
    let events = sqlx::query(
        "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = 'link' \
         AND provider = 'discord' AND length(ip_hash) = 32",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get::<i64, _>(0);
    assert_eq!(events, 1);
    // No entry of the ledger moves (intent, ledger criterion).
    assert_eq!(
        count(&pool, "SELECT count(*) FROM ledger_entries").await,
        entries
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM users").await, 1);
}

/// T6: an identity of another user is never merged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_identity_of_another_user_is_already_linked() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let a = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let discord_of_b = snowflake();
    let b = oauth::session_of(&oauth::login(&app, Kind::Discord, &discord_of_b).await);
    let (user_a, user_b) = (user_of(&pool, &a).await, user_of(&pool, &b).await);
    let done = oauth::link(&app, Kind::Discord, &discord_of_b, &a).await;
    assert_eq!(oauth::error_of(&done), "auth.already_linked");
    assert_eq!(providers_of(&pool, user_a).await, ["google"]);
    assert_eq!(providers_of(&pool, user_b).await, ["discord"]);
    assert!(alive(&app, &a).await);
    assert!(alive(&app, &b).await);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'link'"
        )
        .await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_account_of_the_same_provider_is_provider_already_linked() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let first = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let token = oauth::session_of(&oauth::link(&app, Kind::Discord, &snowflake(), &first).await);
    let user = user_of(&pool, &token).await;
    let done = oauth::link(&app, Kind::Discord, &snowflake(), &token).await;
    assert_eq!(oauth::error_of(&done), "auth.provider_already_linked");
    assert_eq!(providers_of(&pool, user).await, ["discord", "google"]);
    assert!(alive(&app, &token).await);
    // Linking the very identity it already has is the same answer.
    let google: String = sqlx::query(
        "SELECT provider_user_id FROM oauth_accounts WHERE user_id = $1 AND provider = 'google'",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    let done = oauth::link(&app, Kind::Google, &google, &token).await;
    assert_eq!(oauth::error_of(&done), "auth.provider_already_linked");
    assert!(alive(&app, &token).await);
}

/// T4: a `link` flow only finishes with the session of the user who started it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_flow_of_another_session_does_not_link() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let a = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let b = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    for session in [Some(b.as_str()), None] {
        let started = oauth::start_link(&app, Kind::Discord, &a).await;
        let done = oauth::finish(&app, Kind::Discord, &started, &snowflake(), session).await;
        assert_eq!(oauth::error_of(&done), "auth.invalid_state", "{session:?}");
        assert!(alive(&app, &a).await);
        assert!(alive(&app, &b).await);
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM oauth_accounts WHERE provider = 'discord'"
        )
        .await,
        0
    );
    // A login flow never links, even with a session cookie: it is a new user.
    let user_a = user_of(&pool, &a).await;
    let started = oauth::start(&app, Kind::Discord, Some(&a)).await;
    let done = oauth::finish(&app, Kind::Discord, &started, &snowflake(), Some(&a)).await;
    let other = oauth::session_of(&done);
    assert_ne!(user_of(&pool, &other).await, user_a);
    assert_eq!(providers_of(&pool, user_a).await, ["google"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn link_needs_a_session() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    // 008 "Enmienda 2026-10-30" of RF-C2: a 303 to the login page, no JSON (the cases with a
    // revoked cookie are in `tests/navigation_errors.rs`).
    let refused = oauth::begin(&app, "/api/auth/link/discord", "next=%2F", None).await;
    assert_eq!(refused.status, 303);
    assert_eq!(refused.header("location"), Some("/login"));
    assert!(refused.body.is_empty());
    assert_eq!(refused.cookie(oauth::session_name()), Some(""));
    assert_eq!(refused.header("cache-control"), Some("private, no-store"));
    let dead = oauth::begin(&app, "/api/auth/link/discord", "next=%2F", Some("dead")).await;
    assert_eq!(dead.status, 303);
    assert_eq!(dead.header("location"), Some("/login"));
    // An unknown provider is a 404, as in `start`.
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    for provider in ["twitter", "start"] {
        let missing = oauth::begin(
            &app,
            &format!("/api/auth/link/{provider}"),
            "",
            Some(&token),
        )
        .await;
        assert_eq!(missing.status, 404, "{provider}");
        assert_eq!(missing.text(), r#"{"error":"http.not_found"}"#);
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM oauth_flows").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn link_with_the_database_down_is_try_again() {
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let proxy = PgProxy::start(db.server_addr()).await;
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
    let app = support::app::start(state).await;
    proxy.cut();
    let cookie = oauth::session_cookie(&token);
    let failed = http::send_body(
        app.addr,
        "POST",
        "/api/auth/link/discord",
        &[
            ("origin", ORIGIN),
            ("cookie", cookie.as_str()),
            ("content-type", "application/x-www-form-urlencoded"),
        ],
        "",
    )
    .await;
    // 008 "Enmienda 2026-10-30" of RF-C2: a 303 to `auth.try_again`, no JSON and no cookie.
    assert_eq!(failed.status, 303);
    assert_eq!(
        failed.header("location"),
        Some("/login?error=auth.try_again")
    );
    assert!(failed.body.is_empty());
    assert!(failed.set_cookies().is_empty());
}

/// Condition of security-reviewer (spec round 4): a rejected form is the closed error redirect,
/// never axum's text, and the session in use stays valid.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_form_is_invalid_state_without_axum_text() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let too_large = format!("next={}", "a".repeat(4097 - "next=".len()));
    assert_eq!(too_large.len(), 4097);
    for body in [too_large.as_str(), "next=%2Fa&next=%2Fb"] {
        let done = oauth::begin(&app, "/api/auth/link/discord", body, Some(&token)).await;
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
        assert!(
            done.header("content-type")
                .is_none_or(|t| !t.contains("text/plain")),
            "{:?}",
            done.header("content-type")
        );
        assert!(alive(&app, &token).await);
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM oauth_flows").await, 0);
}

/// Holds every link callback between its session check and its transaction until the test
/// releases it.
struct Gate {
    arrived: AtomicUsize,
    permits: Arc<Semaphore>,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            arrived: AtomicUsize::new(0),
            permits: Arc::new(Semaphore::new(0)),
        })
    }

    async fn wait_arrived(&self, n: usize) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while self.arrived.load(Ordering::SeqCst) < n {
            assert!(
                tokio::time::Instant::now() < deadline,
                "no callback arrived"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

impl LinkGate for Gate {
    fn before_link(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.arrived.fetch_add(1, Ordering::SeqCst);
            self.permits.acquire().await.unwrap().forget();
        })
    }
}

async fn gated_app(db: &TestDb, gate: &Arc<Gate>) -> (oauth::App, PgPool) {
    let settings = Settings {
        link_gate: LinkGateConfig(gate.clone()),
        ..Settings::default()
    };
    oauth::start_app_with(db, &[], settings).await
}

/// The path and the flow cookie of the callback of a started link, with a code for `identity`.
fn callback_of(
    app: &oauth::App,
    kind: Kind,
    started: &http::Response,
    identity: &str,
) -> (String, String) {
    assert_eq!(started.status, 303, "{}", started.text());
    let query = oauth::query_of(started.header("location").unwrap());
    let code = app.fake(kind).code_for(&query["code_challenge"], identity);
    let flow = started.cookie(oauth::flow_name()).unwrap().to_owned();
    (
        format!(
            "/api/auth/{}/callback?code={code}&state={}",
            kind.name(),
            query["state"]
        ),
        flow,
    )
}

fn spawn_callback(
    app: &oauth::App,
    path: String,
    flow: String,
    token: &str,
) -> tokio::task::JoinHandle<http::Response> {
    app.refill();
    let addr = app.addr;
    let cookies = format!(
        "{}={flow}; {}",
        oauth::flow_name(),
        oauth::session_cookie(token)
    );
    tokio::spawn(
        async move { http::send(addr, "GET", &path, &[("cookie", cookies.as_str())]).await },
    )
}

/// Requested change 1 of the architect: a `logout-all` that commits while the callback talks to
/// the provider never gets undone by the link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_logout_all_during_the_exchange_wins_over_the_link() {
    let db = TestDb::create().await;
    let gate = Gate::new();
    let (app, pool) = gated_app(&db, &gate).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &sub()).await);
    let user = user_of(&pool, &token).await;
    let other = support::app::session_for(&pool, user).await;
    let started = oauth::start_link(&app, Kind::Discord, &token).await;
    let (path, flow) = callback_of(&app, Kind::Discord, &started, &snowflake());
    let pending = spawn_callback(&app, path, flow, &token);
    gate.wait_arrived(1).await;
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
    assert!(done.cookie(oauth::session_name()).is_none());
    let sessions = sqlx::query("SELECT count(*) FROM sessions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(sessions, 0);
    assert_eq!(providers_of(&pool, user).await, ["google"]);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'link'"
        )
        .await,
        0
    );
}

/// Two link callbacks of the same session: the first rotates it, so the second finds it gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_link_callbacks_of_one_session_link_once() {
    let db = TestDb::create().await;
    let gate = Gate::new();
    let (app, pool) = gated_app(&db, &gate).await;
    // A user without providers, so neither link can collide with an existing account.
    let (user, token) = support::app::user_with_session(&pool).await;
    let google = oauth::start_link(&app, Kind::Google, &token).await;
    let discord = oauth::start_link(&app, Kind::Discord, &token).await;
    let (google_path, google_flow) = callback_of(&app, Kind::Google, &google, &sub());
    let (discord_path, discord_flow) = callback_of(&app, Kind::Discord, &discord, &snowflake());
    let mut first = spawn_callback(&app, google_path, google_flow, &token);
    let mut second = spawn_callback(&app, discord_path, discord_flow, &token);
    gate.wait_arrived(2).await;
    gate.permits.add_permits(1);
    let (winner, loser) = tokio::select! {
        done = &mut first => (done.unwrap(), second),
        done = &mut second => (done.unwrap(), first),
    };
    gate.permits.add_permits(1);
    let loser = loser.await.unwrap();
    let new = oauth::session_of(&winner);
    assert_eq!(oauth::error_of(&loser), "auth.invalid_state");
    let sessions: Vec<Vec<u8>> = sqlx::query("SELECT token_hash FROM sessions WHERE user_id = $1")
        .bind(user)
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(sessions, [TokenHash::of(&new).0.to_vec()]);
    assert_eq!(providers_of(&pool, user).await.len(), 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'link'"
        )
        .await,
        1
    );
}
