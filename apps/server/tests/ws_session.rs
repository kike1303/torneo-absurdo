//! 008 RF-B10, RF-B14, RF-B15: the `/ws` with a session (T10 socket part, T17, T18 partial,
//! T22 user part), revocation through LISTEN/NOTIFY and the self channel.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::SinkExt;
use protocol::SelfEvent;
use server::app::Settings;
use server::rate::{RateSettings, ServerClock};
use server::sessions::{self, RevokeFilter, TokenHash};
use server::ws::{ValidationGate, WsConfig};
use sqlx::PgPool;
use support::app::ORIGIN;
use support::db::TestDb;
use support::http;
use support::ws::{self, Ending};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;

fn cookie(token: &str) -> String {
    let name = support::config::config(&[]).cookies.session_name();
    format!("{name}={token}")
}

async fn connect(
    addr: std::net::SocketAddr,
    token: Option<&str>,
    ip: Option<&str>,
) -> Result<ws::Client, tokio_tungstenite::tungstenite::Error> {
    let cookie = token.map(cookie);
    let mut headers = Vec::new();
    if let Some(cookie) = &cookie {
        headers.push(("cookie", cookie.as_str()));
    }
    if let Some(ip) = ip {
        headers.push(("x-forwarded-for", ip));
    }
    ws::connect_with(addr, ORIGIN, &headers).await
}

/// The status of a refused upgrade, with its body.
async fn refused(addr: std::net::SocketAddr, token: &str, ip: &str) -> http::Response {
    let cookie = cookie(token);
    http::send(
        addr,
        "GET",
        "/ws",
        &[
            ("connection", "Upgrade"),
            ("upgrade", "websocket"),
            ("sec-websocket-version", "13"),
            ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
            ("origin", ORIGIN),
            ("cookie", &cookie),
            ("x-forwarded-for", ip),
        ],
    )
    .await
}

fn manual() -> (Settings, ServerClock) {
    let clock = ServerClock::manual();
    (
        Settings {
            rate: RateSettings {
                clock: clock.clone(),
                ..RateSettings::default()
            },
            ..Settings::default()
        },
        clock,
    )
}

async fn closes_unauthenticated(client: &mut ws::Client, within: Duration) {
    assert_eq!(
        ws::ending(client, within).await,
        Ending::Close(1008, "auth.unauthenticated".into())
    );
}

async fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(done(), "timed out");
}

/// Waits until the revocation listener has run `LISTEN` (and so its startup revalidation),
/// so a test sees only the path it is about.
async fn listener_ready(pool: &PgPool) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "the listener never ran LISTEN");
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT pid FROM pg_stat_activity WHERE application_name = 'torneo-listener' \
             AND datname = current_database() AND query LIKE 'LISTEN%'",
        )
        .fetch_optional(pool)
        .await
        .unwrap();
        if let Some((pid,)) = row {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// An echo round trip: the socket is past its handshake (second validation included) and in
/// its loop.
async fn loop_started(client: &mut ws::Client) {
    ws::send_text(client, r#"{"type":"echo","payload":"ready"}"#).await;
    assert_eq!(
        ws::next_data(client, Duration::from_secs(5)).await,
        Some(Message::Text(r#"{"type":"echo","payload":"ready"}"#.into()))
    );
}

async fn revoke(pool: &PgPool, filter: RevokeFilter) {
    let mut tx = pool.begin().await.unwrap();
    sessions::revoke(&mut tx, filter).await.unwrap();
    tx.commit().await.unwrap();
}

// --- Revocation (T10) -------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_revoked_session_closes_its_socket_at_once() {
    let db = TestDb::create().await;
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    wait_until(|| ws_state.session_sockets() == 1).await;
    let started = Instant::now();
    // Logout through the API (the revalidation is 5 min away).
    let logout = http::send(
        app.addr,
        "POST",
        "/api/auth/logout",
        &[("origin", ORIGIN), ("cookie", &cookie(&token))],
    )
    .await;
    assert_eq!(logout.status, 303);
    closes_unauthenticated(&mut client, Duration::from_secs(5)).await;
    assert!(started.elapsed() < Duration::from_secs(5));
    wait_until(|| ws_state.session_sockets() == 0).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_that_rotates_the_session_closes_the_old_socket() {
    let db = TestDb::create().await;
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws).await;
    let (_user, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    // A new login in the same browser brings the old cookie: rotation revokes it.
    let mut tx = pool.begin().await.unwrap();
    sessions::revoke(&mut tx, RevokeFilter::Token(TokenHash::of(&token)))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    closes_unauthenticated(&mut client, Duration::from_secs(5)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_listener_catches_up_after_losing_its_connection() {
    let db = TestDb::create().await;
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws).await;
    let (user, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    wait_until(|| ws_state.session_sockets() == 1).await;
    // Wait for the listener to be connected, then cut it.
    let pid = listener_ready(&pool).await;
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .execute(&pool)
        .await
        .unwrap();
    // Revoked in the gap: the NOTIFY has no listener.
    revoke(&pool, RevokeFilter::AllOfUser(user)).await;
    // The revalidation on reconnect closes it, long before the 5-minute one.
    closes_unauthenticated(&mut client, Duration::from_secs(5)).await;
}

/// RF-B14: the socket is registered before the second validation, so a revoke in between is
/// never lost. The gate holds the socket right after the registration.
struct Gate {
    registered: Notify,
    go: Notify,
}

impl ValidationGate for Gate {
    fn before_validate(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            self.registered.notify_one();
            self.go.notified().await;
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_revoke_between_registration_and_validation_closes_the_socket() {
    let db = TestDb::create().await;
    let gate = Arc::new(Gate {
        registered: Notify::new(),
        go: Notify::new(),
    });
    let held = gate.clone();
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &Settings::default(), move |ws| {
        ws.with_validation_gate(held)
    })
    .await;
    let (user, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    gate.registered.notified().await;
    revoke(&pool, RevokeFilter::AllOfUser(user)).await;
    gate.go.notify_one();
    closes_unauthenticated(&mut client, Duration::from_secs(5)).await;
}

/// Round 3 of #54, the root cause of the flaky revalidation tests: the second validation of
/// the handshake runs after the client already has its 101, and it had no timeout. If it hangs
/// (here `users` is locked right before it), the socket never reached its loop: no frames, no
/// shutdown, no revocation. Now it is bounded by the revalidation timeout and fails closed with
/// `auth.try_again`, so the client reconnects.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hung_second_validation_closes_the_socket_with_try_again() {
    let db = TestDb::create().await;
    let gate = Arc::new(Gate {
        registered: Notify::new(),
        go: Notify::new(),
    });
    let held = gate.clone();
    let settings = Settings {
        ws: WsConfig {
            handshake_validate_timeout: Duration::from_millis(300),
            ..WsConfig::default()
        },
        ..Settings::default()
    };
    let (app, pool, _ws) =
        support::app::start_db_ws(&db, &[], &settings, move |ws| ws.with_validation_gate(held))
            .await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    gate.registered.notified().await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    gate.go.notify_one();
    let ending = ws::ending(&mut client, Duration::from_secs(3)).await;
    lock.rollback().await.unwrap();
    assert_eq!(ending, Ending::Close(1008, "auth.try_again".into()));
}

/// QA on #54: a database error (not a timeout) in the second validation also fails closed with
/// `auth.try_again`, never as `auth.unauthenticated` (the session may be fine).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_database_error_in_the_second_validation_closes_with_try_again() {
    use server::app::AppState;
    use support::pg_proxy::PgProxy;
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let url = db.url_via(proxy.addr());
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(1))
        .connect_lazy_with(config.database.clone());
    let gate = Arc::new(Gate {
        registered: Notify::new(),
        go: Notify::new(),
    });
    let held = gate.clone();
    let state = AppState::from_config(
        &config,
        &Settings::default(),
        pool,
        Arc::new(support::app::AlwaysUp),
    )
    .unwrap()
    .with_ws(AppState::ws_from_config(&config, &Settings::default()).with_validation_gate(held));
    let app = support::app::start(state).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    gate.registered.notified().await;
    proxy.cut();
    gate.go.notify_one();
    let ending = ws::ending(&mut client, Duration::from_secs(4)).await;
    proxy.restore();
    assert_eq!(ending, Ending::Close(1008, "auth.try_again".into()));
}

/// RF-B14, pinned: the socket is held after the first validation and before its registration,
/// and the session is revoked there. Its NOTIFY finds nothing registered, so only the second
/// validation, which must come after the registration, can close it. A registration moved
/// after the second validation leaves this socket open and fails the test.
struct RegisterGate {
    reached: Notify,
    go: Notify,
}

impl ValidationGate for RegisterGate {
    fn before_register(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            self.reached.notify_one();
            self.go.notified().await;
        })
    }

    fn before_validate(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_revoke_before_the_registration_is_caught_by_the_second_validation() {
    let db = TestDb::create().await;
    let gate = Arc::new(RegisterGate {
        reached: Notify::new(),
        go: Notify::new(),
    });
    let held = gate.clone();
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[], &Settings::default(), move |ws| {
            ws.with_validation_gate(held)
        })
        .await;
    let (user, token) = support::app::user_with_session(&pool).await;
    listener_ready(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    gate.reached.notified().await;
    assert_eq!(ws_state.session_sockets(), 0);
    revoke(&pool, RevokeFilter::AllOfUser(user)).await;
    // The NOTIFY has been handled, and found no socket to close.
    wait_until(|| ws_state.registry().closes_requested() == 1).await;
    gate.go.notify_one();
    closes_unauthenticated(&mut client, Duration::from_secs(5)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_periodic_revalidation_closes_an_expired_session() {
    let db = TestDb::create().await;
    let settings = Settings {
        ws: WsConfig {
            revalidate_interval: Duration::from_millis(200),
            ..WsConfig::default()
        },
        ..Settings::default()
    };
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &settings, |ws| ws).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    // Expiry is not a revoke: no NOTIFY, only the revalidation sees it.
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    closes_unauthenticated(&mut client, Duration::from_secs(3)).await;
}

/// architect on #54 (condition of deviation 17): a revalidation query that hangs (here `users`
/// is locked) is cut by its timeout and treated like an error, so the socket keeps answering
/// instead of freezing inside the `select!`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hung_revalidation_does_not_freeze_the_socket() {
    let db = TestDb::create().await;
    let settings = Settings {
        ws: WsConfig {
            revalidate_interval: Duration::from_millis(100),
            revalidate_timeout: Duration::from_millis(200),
            ..WsConfig::default()
        },
        ..Settings::default()
    };
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &settings, |ws| ws).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    loop_started(&mut client).await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    ws::send_text(&mut client, r#"{"type":"echo","payload":"vivo"}"#).await;
    let reply = ws::next_data(&mut client, Duration::from_secs(2)).await;
    lock.rollback().await.unwrap();
    assert_eq!(
        reply,
        Some(Message::Text(r#"{"type":"echo","payload":"vivo"}"#.into()))
    );
}

/// The check runs outside the socket loop (architect on #54): with a timeout longer than the
/// echo wait, the socket still answers while the check hangs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_revalidation_runs_outside_the_socket_loop() {
    let db = TestDb::create().await;
    let settings = Settings {
        ws: WsConfig {
            revalidate_interval: Duration::from_millis(100),
            revalidate_timeout: Duration::from_secs(10),
            ..WsConfig::default()
        },
        ..Settings::default()
    };
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &settings, |ws| ws).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    loop_started(&mut client).await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    // Wait until a revalidation is really hanging on the lock (architect on #54).
    let admin = db_admin(&db).await;
    assert!(
        hung_checks_reach(&admin, 1).await,
        "no revalidation ever waited on the lock"
    );
    ws::send_text(&mut client, r#"{"type":"echo","payload":"vivo"}"#).await;
    let reply = ws::next_data(&mut client, Duration::from_secs(2)).await;
    lock.rollback().await.unwrap();
    assert_eq!(
        reply,
        Some(Message::Text(r#"{"type":"echo","payload":"vivo"}"#.into()))
    );
}

/// The timeout bounds a hung check, so the next one starts: while `users` stays locked, the
/// app's revalidation queries waiting on the lock keep growing. Without the timeout there would
/// be exactly one, forever (and no later check could ever close the socket).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hung_revalidation_times_out_and_the_next_one_starts() {
    let db = TestDb::create().await;
    let settings = Settings {
        ws: WsConfig {
            revalidate_interval: Duration::from_millis(50),
            revalidate_timeout: Duration::from_millis(150),
            ..WsConfig::default()
        },
        ..Settings::default()
    };
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &settings, |ws| ws).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut _client = connect(app.addr, Some(&token), None).await.unwrap();
    loop_started(&mut _client).await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let admin = db_admin(&db).await;
    let started_again = hung_checks_reach(&admin, 2).await;
    lock.rollback().await.unwrap();
    assert!(
        started_again,
        "a timed-out revalidation was never followed by a new one"
    );
}

/// Polls `pg_stat_activity` (up to 5 s) until at least `n` revalidation queries of the app wait
/// on the lock of `users`.
async fn hung_checks_reach(admin: &PgPool, n: i64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let (waiting,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
             AND wait_event_type = 'Lock' AND query LIKE '%FROM sessions s JOIN users%'",
        )
        .fetch_one(admin)
        .await
        .unwrap();
        if waiting >= n {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// A connection outside the app's pool (the app's may be all waiting on the lock).
async fn db_admin(db: &TestDb) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(db.options())
        .await
        .unwrap()
}

/// security on #54: when the session lookup of the handshake fails, the socket is anonymous:
/// it opens, is never registered with a session, and `self` is forbidden.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_session_lookup_makes_an_anonymous_socket() {
    use server::app::AppState;
    let db = TestDb::create().await;
    let direct = db.migrated_pool().await;
    let (_, token) = support::app::user_with_session(&direct).await;
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let options = config
        .database
        .clone()
        .options([("statement_timeout", "300")]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(options);
    let state = AppState::from_config(
        &config,
        &Settings::default(),
        pool,
        Arc::new(support::app::AlwaysUp),
    )
    .unwrap();
    let ws_state = state.ws.clone();
    let app = support::app::start(state).await;
    let mut lock = direct.begin().await.unwrap();
    sqlx::query("LOCK TABLE users IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let client = connect(app.addr, Some(&token), None).await;
    lock.rollback().await.unwrap();
    let mut client = client.expect("the socket opens, as anonymous");
    assert_eq!(ws_state.session_sockets(), 0);
    ws::send_text(&mut client, r#"{"type":"subscribe","channel":"self"}"#).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(2)).await,
        Some(Message::Text(
            r#"{"type":"error","code":"ws.forbidden_channel"}"#.into()
        ))
    );
}

// --- Limits (RF-B14, T18, T22) ----------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sixth_socket_of_a_user_is_503_and_they_do_not_use_anonymous_slots() {
    let db = TestDb::create().await;
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws.with_ip_limit(1)).await;
    let (user, _) = support::app::user_with_session(&pool).await;
    let mut clients = Vec::new();
    for _ in 0..5 {
        let token = support::app::session_for(&pool, user).await;
        clients.push(connect(app.addr, Some(&token), None).await.unwrap());
    }
    let token = support::app::session_for(&pool, user).await;
    let sixth = refused(app.addr, &token, "127.0.0.1").await;
    assert_eq!(sixth.status, 503);
    assert_eq!(sixth.text(), r#"{"error":"ws.user_capacity"}"#);
    // The anonymous cap of this IP (1 here) is still free.
    clients.push(connect(app.addr, None, None).await.unwrap());
    assert_eq!(ws_state.session_sockets(), 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_invalid_cookie_is_an_anonymous_socket_without_self() {
    let db = TestDb::create().await;
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws).await;
    let (user, _) = support::app::user_with_session(&pool).await;
    let mut clients = Vec::new();
    for _ in 0..5 {
        let token = support::app::session_for(&pool, user).await;
        clients.push(connect(app.addr, Some(&token), None).await.unwrap());
    }
    let expired = support::app::session_for(&pool, user).await;
    sqlx::query(
        "UPDATE sessions SET expires_at = now() - interval '1 second' WHERE token_hash = $1",
    )
    .bind(TokenHash::of(&expired).0.as_slice())
    .execute(&pool)
    .await
    .unwrap();
    // The sixth, with an expired cookie, gets in as anonymous (no ws.user_capacity).
    let mut anonymous = connect(app.addr, Some(&expired), None).await.unwrap();
    assert_eq!(ws_state.session_sockets(), 5);
    ws::send_text(&mut anonymous, r#"{"type":"subscribe","channel":"self"}"#).await;
    assert_eq!(
        ws::next_data(&mut anonymous, Duration::from_secs(2)).await,
        Some(Message::Text(
            r#"{"type":"error","code":"ws.forbidden_channel"}"#.into()
        ))
    );
    // Nothing of the user's self channel reaches it.
    ws_state.self_hub().publish(
        user,
        SelfEvent::ProfileChanged {
            display_name: "Nuevo".to_owned(),
        },
    );
    assert!(
        ws::next_data(&mut anonymous, Duration::from_millis(300))
            .await
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_201st_socket_with_a_session_from_one_64_is_503() {
    let db = TestDb::create().await;
    let (settings, clock) = manual();
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[support::app::TRUST_LOOPBACK], &settings, |ws| ws).await;
    let mut users = Vec::new();
    for _ in 0..41 {
        users.push(support::app::user_with_session(&pool).await.0);
    }
    let mut sockets = Vec::new();
    for n in 0..200_usize {
        // One upgrade token per half second at 120/min: never limited here.
        clock.advance(Duration::from_secs(1));
        let token = support::app::session_for(&pool, users[n / 5]).await;
        let ip = format!("2001:db8:1:2::{:x}", n + 1);
        sockets.push(connect(app.addr, Some(&token), Some(&ip)).await.unwrap());
    }
    clock.advance(Duration::from_secs(1));
    let token = support::app::session_for(&pool, users[40]).await;
    let full = refused(app.addr, &token, "2001:db8:1:2::ffff").await;
    assert_eq!(full.status, 503);
    assert_eq!(full.text(), r#"{"error":"ws.ip_capacity"}"#);
    // Another /64 and the anonymous sockets of this one still get in.
    let other = connect(app.addr, Some(&token), Some("2001:db8:1:3::1")).await;
    assert!(other.is_ok());
    assert!(
        connect(app.addr, None, Some("2001:db8:1:2::1"))
            .await
            .is_ok()
    );
    assert_eq!(ws_state.session_sockets(), 201);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_user_from_two_ips_shares_its_upgrade_bucket() {
    let db = TestDb::create().await;
    let (settings, _clock) = manual();
    let (app, pool, _ws) =
        support::app::start_db_ws(&db, &[support::app::TRUST_LOOPBACK], &settings, |ws| ws).await;
    let (user, _) = support::app::user_with_session(&pool).await;
    // 10 upgrades (5 sockets, then 5 refused by the user cap) alternating two IPs.
    let mut sockets = Vec::new();
    for n in 0..10 {
        let token = support::app::session_for(&pool, user).await;
        let ip = if n % 2 == 0 {
            "203.0.113.7"
        } else {
            "198.51.100.9"
        };
        if let Ok(client) = connect(app.addr, Some(&token), Some(ip)).await {
            sockets.push(client);
        }
    }
    assert_eq!(sockets.len(), 5);
    let token = support::app::session_for(&pool, user).await;
    let limited = refused(app.addr, &token, "192.0.2.44").await;
    assert_eq!(limited.status, 429);
    assert_eq!(limited.text(), r#"{"error":"ws.rate_limited"}"#);
}

// --- Channels (T17, T18) ----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t17_self_reaches_only_its_user_and_other_channels_are_forbidden() {
    let db = TestDb::create().await;
    let (app, pool, ws_state) =
        support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws).await;
    let (a, token_a) = support::app::user_with_session(&pool).await;
    let (b, _) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token_a), None).await.unwrap();
    for channel in [
        format!("user:{b}"),
        format!("user:{a}"),
        "users".to_owned(),
        "SELF".to_owned(),
        "self ".to_owned(),
        String::new(),
        "fight:1".to_owned(),
    ] {
        let message = serde_json::json!({"type": "subscribe", "channel": channel}).to_string();
        ws::send_text(&mut client, &message).await;
        assert_eq!(
            ws::next_data(&mut client, Duration::from_secs(2)).await,
            Some(Message::Text(
                r#"{"type":"error","code":"ws.forbidden_channel"}"#.into()
            )),
            "{channel:?}"
        );
        // The rate window is 10 per 10 s.
        tokio::time::sleep(Duration::from_millis(1100)).await;
    }
    // Before subscribing, nothing arrives.
    let event = SelfEvent::ProfileChanged {
        display_name: "TigreVeloz42".to_owned(),
    };
    ws_state.self_hub().publish(a, event.clone());
    assert!(
        ws::next_data(&mut client, Duration::from_millis(300))
            .await
            .is_none()
    );
    ws::send_text(&mut client, r#"{"type":"subscribe","channel":"self"}"#).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(2)).await,
        Some(Message::Text(
            r#"{"type":"subscribed","channel":"self"}"#.into()
        ))
    );
    ws_state.self_hub().publish(b, event.clone());
    ws_state.self_hub().publish(a, event);
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(2)).await,
        Some(Message::Text(
            r#"{"type":"self_event","event":{"kind":"profile_changed","display_name":"TigreVeloz42"}}"#
                .into()
        ))
    );
    assert!(
        ws::next_data(&mut client, Duration::from_millis(300))
            .await
            .is_none()
    );
    // Unsubscribed: nothing more.
    ws::send_text(&mut client, r#"{"type":"unsubscribe","channel":"self"}"#).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(2)).await,
        Some(Message::Text(
            r#"{"type":"unsubscribed","channel":"self"}"#.into()
        ))
    );
    ws_state.self_hub().publish(
        a,
        SelfEvent::ProfileChanged {
            display_name: "Otro".to_owned(),
        },
    );
    assert!(
        ws::next_data(&mut client, Duration::from_millis(300))
            .await
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_over_4_kb_closes_a_socket_with_a_session() {
    let db = TestDb::create().await;
    let (app, pool, _ws) = support::app::start_db_ws(&db, &[], &Settings::default(), |ws| ws).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut client = connect(app.addr, Some(&token), None).await.unwrap();
    let text = format!(r#"{{"type":"subscribe","channel":"{}"}}"#, "x".repeat(4097));
    client.send(Message::Text(text.into())).await.unwrap();
    let ending = ws::ending(&mut client, Duration::from_secs(2)).await;
    assert!(
        matches!(ending, Ending::Close(1009, _) | Ending::NoCloseFrame),
        "{ending:?}"
    );
}
