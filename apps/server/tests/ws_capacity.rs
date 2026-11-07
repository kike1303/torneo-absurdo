//! Test 18 (012 RF39, ADR-0008 D6): the last fifth of `WS_MAX_CONNECTIONS` is for accounts out of
//! probation. With a cap of 5 the reserve is 1: four sockets enter; the fifth, anonymous or in
//! probation, gets 503 `ws.capacity`; one out of probation enters; and when one leaves, an anonymous
//! one still does not, because it would leave less than the reserve.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use server::app::Settings;
use sqlx::PgPool;
use support::app::ORIGIN;
use support::db::TestDb;
use support::{http, ws};
use uuid::Uuid;

async fn session(pool: &PgPool, out_of_probation: bool) -> String {
    let (user, token) = support::app::user_with_session(pool).await;
    if out_of_probation {
        sqlx::query("UPDATE users SET created_at = now() - interval '8 days' WHERE id = $1")
            .bind(user)
            .execute(pool)
            .await
            .unwrap();
        mature(pool, user).await;
    }
    token
}

/// Ten fights with a bet lost, written straight: the guard of `bets` is off for them.
async fn mature(pool: &PgPool, user: Uuid) {
    let mut tx = support::db::begin_privileged(pool).await;
    sqlx::query("ALTER TABLE bets DISABLE TRIGGER USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    for k in 1..=10 {
        let id = support::fights::announce(
            pool,
            support::fights::T0 + k * support::fights::CADENCE,
            &support::fights::golden_spec("striker"),
            &support::fights::golden_spec("tank"),
        )
        .await;
        let bet = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
             VALUES ($1, $2, $3, 'a', 1, false, $4)",
        )
        .bind(bet)
        .bind(id)
        .bind(user)
        .bind(Uuid::new_v4())
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO bet_settlements \
             (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
             VALUES ($1, $2, $3, 'lost', 0, -1, -1)",
        )
        .bind(bet)
        .bind(id)
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query("ALTER TABLE bets ENABLE TRIGGER USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn with_session(
    addr: std::net::SocketAddr,
    token: &str,
) -> Result<ws::Client, tokio_tungstenite::tungstenite::Error> {
    let cookie = support::oauth::session_cookie(token);
    ws::connect_with(addr, ORIGIN, &[("cookie", cookie.as_str())]).await
}

async fn refused_with(addr: std::net::SocketAddr, cookie: Option<&str>) {
    let mut headers = vec![
        ("origin", ORIGIN),
        ("connection", "upgrade"),
        ("upgrade", "websocket"),
        ("sec-websocket-version", "13"),
        ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
    ];
    if let Some(cookie) = cookie {
        headers.push(("cookie", cookie));
    }
    let response = http::send(addr, "GET", "/ws", &headers).await;
    assert_eq!(response.status, 503, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"ws.capacity"}"#);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_reserve_is_for_accounts_out_of_probation() {
    let db = TestDb::create().await;
    let (app, pool, state) = support::app::start_db_ws(
        &db,
        &[("WS_MAX_CONNECTIONS", "5")],
        &Settings::default(),
        |ws| ws,
    )
    .await;
    assert_eq!(state.reserve(), 1);
    let mut open = Vec::new();
    for _ in 0..4 {
        open.push(ws::connect(app.addr, ORIGIN).await.unwrap());
    }
    assert_eq!(state.available_permits(), 1);
    // Anonymous, and in probation: refused, and the permit comes back.
    refused_with(app.addr, None).await;
    let new = session(&pool, false).await;
    let cookie = support::oauth::session_cookie(&new);
    refused_with(app.addr, Some(cookie.as_str())).await;
    assert_eq!(state.available_permits(), 1);
    // Out of probation: in.
    let grown = session(&pool, true).await;
    let mature = with_session(app.addr, &grown).await.unwrap();
    assert_eq!(state.available_permits(), 0);
    // One leaves: an anonymous one would leave 0 free, under the reserve.
    let mut leaving = open.pop().unwrap();
    leaving.close(None).await.unwrap();
    ws::assert_stream_ends(&mut leaving, Duration::from_secs(2)).await;
    support::fights::wait_for("the permit back", || async {
        state.available_permits() == 1
    })
    .await;
    refused_with(app.addr, None).await;
    drop(mature);
}
