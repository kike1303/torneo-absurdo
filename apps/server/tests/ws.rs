//! RF-B13: the WebSocket handshake, limits, lifetimes and echo.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::ffi::OsString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use server::app::{AppState, Settings};
use server::config::Config;
use server::db::{DbProbe, PingFuture};
use server::ws::{WsConfig, WsState};
use support::app::{ORIGIN, RunningApp};
use support::http;
use support::ws::{self, Ending};
use tokio_tungstenite::tungstenite::Message;

struct AlwaysUp;

impl DbProbe for AlwaysUp {
    fn ping(&self) -> PingFuture<'_> {
        Box::pin(async { true })
    }
}

const SLOW: WsConfig = WsConfig {
    ping_interval: Duration::from_secs(30),
    pong_timeout: Duration::from_secs(60),
    idle_timeout: Duration::from_secs(120),
    close_grace: Duration::from_secs(2),
};

async fn start(ws: WsState) -> (RunningApp, Arc<WsState>) {
    let state = AppState::new(Arc::new(AlwaysUp), &Settings::default(), ws);
    let ws = state.ws.clone();
    (support::app::start(state).await, ws)
}

async fn start_echo(echo: bool, max: u32, config: WsConfig) -> (RunningApp, Arc<WsState>) {
    start(WsState::new(ORIGIN.to_owned(), echo, max, config)).await
}

const UPGRADE: [(&str, &str); 4] = [
    ("connection", "Upgrade"),
    ("upgrade", "websocket"),
    ("sec-websocket-version", "13"),
    ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
];

fn with_origin(origin: Option<&str>, upgrade: bool) -> Vec<(&str, &str)> {
    let mut headers: Vec<(&str, &str)> = if upgrade {
        UPGRADE.to_vec()
    } else {
        Vec::new()
    };
    if let Some(origin) = origin {
        headers.push(("origin", origin));
    }
    headers
}

fn assert_security_headers(response: &http::Response) {
    assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        response.header("strict-transport-security"),
        Some("max-age=31536000")
    );
}

// --- Handshake -------------------------------------------------------------------------

#[tokio::test]
async fn a_missing_or_foreign_origin_is_403_without_upgrade() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    for origin in [
        None,
        Some("https://evil.example"),
        Some("http://localhost:5173/"),
        Some("HTTP://localhost:5173"),
        Some("null"),
    ] {
        for upgrade in [true, false] {
            let response = http::send(app.addr, "GET", "/ws", &with_origin(origin, upgrade)).await;
            assert_eq!(response.status, 403, "{origin:?} upgrade={upgrade}");
            assert_eq!(response.text(), r#"{"error":"ws.origin_forbidden"}"#);
            assert!(response.header("upgrade").is_none());
            assert_security_headers(&response);
        }
    }
    assert!(ws::connect(app.addr, "https://evil.example").await.is_err());
}

#[tokio::test]
async fn two_origin_headers_are_403_even_if_one_is_valid() {
    let (app, ws_state) = start_echo(true, 2000, SLOW).await;
    for (first, second) in [
        (ORIGIN, ORIGIN),
        (ORIGIN, "https://evil.example"),
        ("https://evil.example", ORIGIN),
    ] {
        let mut headers = UPGRADE.to_vec();
        headers.push(("origin", first));
        headers.push(("origin", second));
        let response = http::send(app.addr, "GET", "/ws", &headers).await;
        assert_eq!(response.status, 403, "{first} + {second}");
        assert_eq!(response.text(), r#"{"error":"ws.origin_forbidden"}"#);
        assert!(response.header("upgrade").is_none());
    }
    assert_eq!(ws_state.available_permits(), 2000);
}

#[tokio::test]
async fn a_valid_origin_without_upgrade_gets_axums_4xx_with_an_empty_body() {
    // Cap 0: a request that is not an upgrade must never reach the cap (no 503).
    let (app, _) = start_echo(true, 0, SLOW).await;
    for method in ["GET", "HEAD"] {
        let response = http::send(app.addr, method, "/ws", &with_origin(Some(ORIGIN), false)).await;
        assert!(
            response.status.is_client_error(),
            "{method}: {}",
            response.status
        );
        assert!(response.body.is_empty(), "{method}");
        assert!(response.header("content-type").is_none(), "{method}");
        assert_security_headers(&response);
    }
    let full = http::send(app.addr, "GET", "/ws", &with_origin(Some(ORIGIN), true)).await;
    assert_eq!(full.status, 503);
    assert_eq!(full.text(), r#"{"error":"ws.capacity"}"#);
}

#[tokio::test]
async fn the_cap_answers_503_at_once_and_frees_the_permit_on_close() {
    let (app, ws_state) = start_echo(true, 2, SLOW).await;
    let mut first = ws::connect(app.addr, ORIGIN).await.unwrap();
    let _second = ws::connect(app.addr, ORIGIN).await.unwrap();

    let started = Instant::now();
    let refused = http::send(app.addr, "GET", "/ws", &with_origin(Some(ORIGIN), true)).await;
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(refused.status, 503);
    assert_eq!(refused.text(), r#"{"error":"ws.capacity"}"#);
    assert!(ws::connect(app.addr, ORIGIN).await.is_err());

    first.close(None).await.unwrap();
    ws::assert_stream_ends(&mut first, Duration::from_secs(2)).await;
    let deadline = Instant::now() + Duration::from_secs(2);
    while ws_state.available_permits() == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ws::connect(app.addr, ORIGIN).await.is_ok());
}

// --- Messages ---------------------------------------------------------------------------

#[tokio::test]
async fn echo_round_trip() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    ws::send_text(&mut client, r#"{"type":"echo","payload":"hola"}"#).await;
    let reply = ws::next_data(&mut client, Duration::from_secs(2)).await;
    assert_eq!(
        reply,
        Some(Message::Text(r#"{"type":"echo","payload":"hola"}"#.into()))
    );
}

#[tokio::test]
async fn echo_disabled_closes_with_1008() {
    let (app, _) = start_echo(false, 2000, SLOW).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    ws::send_text(&mut client, r#"{"type":"echo","payload":"hola"}"#).await;
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(2)).await,
        Ending::Close(1008, "ws.echo_disabled".into())
    );
}

#[tokio::test]
async fn echo_flag_with_an_https_origin_stays_off() {
    let config = Config::from_lookup(|name| match name {
        "DATABASE_URL" => Some(OsString::from("postgres://u:p@127.0.0.1/db")),
        "PUBLIC_ORIGIN" => Some(OsString::from("https://torneo.example")),
        "ENABLE_WS_ECHO" => Some(OsString::from("1")),
        _ => None,
    })
    .unwrap();
    let settings = Settings {
        ws: SLOW,
        ..Settings::default()
    };
    let (app, _) = start(AppState::ws_from_config(&config, &settings)).await;
    let mut client = ws::connect(app.addr, "https://torneo.example")
        .await
        .unwrap();
    ws::send_text(&mut client, r#"{"type":"echo","payload":"hola"}"#).await;
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(2)).await,
        Ending::Close(1008, "ws.echo_disabled".into())
    );
}

#[tokio::test]
async fn a_message_over_4_kb_closes_with_1009_or_drops() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    // 4097 bytes on the wire as a single text frame.
    let payload = "x".repeat(4097 - r#"{"type":"echo","payload":""}"#.len());
    let text = format!(r#"{{"type":"echo","payload":"{payload}"}}"#);
    assert_eq!(text.len(), 4097);
    ws::send_text(&mut client, &text).await;
    let ending = ws::ending(&mut client, Duration::from_secs(1)).await;
    // Observed with tungstenite 0.29 (plan-b deviations): a 1009 close frame.
    assert!(
        matches!(ending, Ending::Close(1009, _) | Ending::NoCloseFrame),
        "{ending:?}"
    );

    // 4096 bytes still pass.
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    let payload = "x".repeat(4096 - r#"{"type":"echo","payload":""}"#.len());
    let text = format!(r#"{{"type":"echo","payload":"{payload}"}}"#);
    ws::send_text(&mut client, &text).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(2)).await,
        Some(Message::Text(text.into()))
    );
}

#[tokio::test]
async fn the_eleventh_message_in_ten_seconds_is_rate_limited() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    for _ in 0..10 {
        ws::send_text(&mut client, r#"{"type":"echo","payload":"x"}"#).await;
        assert!(matches!(
            ws::next_data(&mut client, Duration::from_secs(2)).await,
            Some(Message::Text(_))
        ));
    }
    ws::send_text(&mut client, r#"{"type":"echo","payload":"x"}"#).await;
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(2)).await,
        Ending::Close(1008, "ws.rate_limited".into())
    );
}

#[tokio::test]
async fn client_pings_count_toward_the_rate() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    for _ in 0..11 {
        client
            .send(Message::Ping(Default::default()))
            .await
            .unwrap();
    }
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(2)).await,
        Ending::Close(1008, "ws.rate_limited".into())
    );
}

#[tokio::test]
async fn a_binary_frame_closes_with_1003() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    client
        .send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert!(matches!(
        ws::ending(&mut client, Duration::from_secs(2)).await,
        Ending::Close(1003, _)
    ));
}

#[tokio::test]
async fn invalid_messages_close_with_1008() {
    let (app, _) = start_echo(true, 2000, SLOW).await;
    for text in ["hola", r#"{"type":"subscribe"}"#, r#"{"type":"echo"}"#] {
        let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
        ws::send_text(&mut client, text).await;
        assert_eq!(
            ws::ending(&mut client, Duration::from_secs(2)).await,
            Ending::Close(1008, "ws.invalid_message".into()),
            "{text}"
        );
    }
}

// --- Lifetimes --------------------------------------------------------------------------

#[tokio::test]
async fn a_client_that_never_reads_is_closed_after_the_pong_timeout() {
    let config = WsConfig {
        ping_interval: Duration::from_millis(100),
        pong_timeout: Duration::from_millis(300),
        idle_timeout: Duration::from_secs(10),
        close_grace: Duration::from_millis(200),
    };
    let (app, _) = start_echo(true, 2000, config).await;
    // A client that never writes after the handshake: no pongs, no close reply. It starts
    // reading after pong timeout + close grace, so the server has already dropped it.
    let frames = ws::silent_client_frames(
        app.addr,
        ORIGIN,
        Duration::from_millis(900),
        Duration::from_secs(1),
    )
    .await;
    let (last_opcode, last_payload) = frames.last().expect("no frames");
    assert_eq!(*last_opcode, 0x8, "{frames:?}");
    assert_eq!(
        ws::close_payload(last_payload),
        (1008, "ws.pong_timeout".to_owned())
    );
    // Before the close, only pings.
    assert!(frames[..frames.len() - 1].iter().all(|(op, _)| *op == 0x9));
}

#[tokio::test]
async fn a_client_that_only_answers_pings_is_closed_when_idle() {
    let config = WsConfig {
        ping_interval: Duration::from_millis(100),
        pong_timeout: Duration::from_millis(400),
        idle_timeout: Duration::from_millis(800),
        close_grace: Duration::from_millis(500),
    };
    let (app, _) = start_echo(true, 2000, config).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    let started = Instant::now();
    // Reading keeps answering pings (tungstenite queues and flushes the pongs).
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(3)).await,
        Ending::Close(1008, "ws.idle_timeout".into())
    );
    assert!(
        started.elapsed() >= Duration::from_millis(700),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn accepted_messages_reset_the_idle_timer() {
    let config = WsConfig {
        ping_interval: Duration::from_secs(30),
        pong_timeout: Duration::from_secs(60),
        idle_timeout: Duration::from_millis(600),
        close_grace: Duration::from_millis(500),
    };
    let (app, _) = start_echo(true, 2000, config).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        ws::send_text(&mut client, r#"{"type":"echo","payload":"x"}"#).await;
        assert!(matches!(
            ws::next_data(&mut client, Duration::from_secs(1)).await,
            Some(Message::Text(_))
        ));
    }
    assert!(
        client.next().await.is_some(),
        "still open after 1.2 s of activity"
    );
}

// --- Per-IP limits of the anonymous /ws (008 RF-A7, RF-A4) ------------------------------

use server::rate::{MAX_KEYS, RateSettings, ServerClock};

/// The server trusting `127.0.0.1` (so `X-Forwarded-For` picks the client), with a manual
/// rate clock and `max_keys` keys per limiter.
async fn start_limited(ws: WsState, max_keys: usize) -> (RunningApp, Arc<WsState>, ServerClock) {
    let clock = ServerClock::manual();
    let settings = Settings {
        rate: RateSettings {
            max_keys,
            clock: clock.clone(),
        },
        ..Settings::default()
    };
    let state = support::app::state_with_ws(
        support::app::net(support::app::trusting_loopback(), false),
        &settings,
        ws,
    );
    let ws = state.ws.clone();
    (support::app::start(state).await, ws, clock)
}

fn upgrade_from<'a>(client: &'a str, origin: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut headers = UPGRADE.to_vec();
    headers.push(("origin", origin));
    headers.push(("x-forwarded-for", client));
    headers
}

async fn refused(app: &RunningApp, client: &str) -> http::Response {
    http::send(app.addr, "GET", "/ws", &upgrade_from(client, ORIGIN)).await
}

async fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !done() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(done(), "timed out");
}

#[tokio::test]
async fn the_fifty_first_socket_of_a_64_is_503_and_slots_are_released() {
    let (app, ws_state, clock) =
        start_limited(WsState::new(ORIGIN.to_owned(), true, 2000, SLOW), MAX_KEYS).await;
    let mut clients = Vec::new();
    for n in 0..50 {
        // One upgrade token every 2 s: never limited by the rate here.
        clock.advance(Duration::from_secs(2));
        let ip = format!("2001:db8:1:2::{n:x}");
        let headers = [("x-forwarded-for", ip.as_str())];
        clients.push(ws::connect_with(app.addr, ORIGIN, &headers).await.unwrap());
    }
    clock.advance(Duration::from_secs(2));
    let full = refused(&app, "2001:db8:1:2:ffff::1").await;
    assert_eq!(full.status, 503);
    assert_eq!(full.text(), r#"{"error":"ws.ip_capacity"}"#);
    assert_security_headers(&full);
    // Another /64 still gets in.
    let other = [("x-forwarded-for", "2001:db8:1:3::1")];
    let other = ws::connect_with(app.addr, ORIGIN, &other).await.unwrap();
    assert_eq!(ws_state.ip_keys_in_use(), 2);

    // Closing one frees its slot (RAII).
    let mut first = clients.remove(0);
    first.close(None).await.unwrap();
    ws::assert_stream_ends(&mut first, Duration::from_secs(2)).await;
    clock.advance(Duration::from_secs(2));
    let headers = [("x-forwarded-for", "2001:db8:1:2::abcd")];
    let deadline = Instant::now() + Duration::from_secs(3);
    let again = loop {
        match ws::connect_with(app.addr, ORIGIN, &headers).await {
            Ok(client) => break client,
            Err(_) if Instant::now() < deadline => {
                clock.advance(Duration::from_secs(2));
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => panic!("{error}"),
        }
    };
    clients.push(again);

    // After closing everything, no key is left.
    clients.push(other);
    for mut client in clients {
        client.close(None).await.unwrap();
        ws::assert_stream_ends(&mut client, Duration::from_secs(2)).await;
    }
    wait_until(|| ws_state.ip_keys_in_use() == 0).await;
}

#[tokio::test]
async fn the_eleventh_upgrade_in_a_burst_is_429_with_retry_after() {
    let (app, ws_state, clock) =
        start_limited(WsState::new(ORIGIN.to_owned(), true, 2000, SLOW), MAX_KEYS).await;
    let headers = [("x-forwarded-for", "203.0.113.7")];
    let mut clients = Vec::new();
    for _ in 0..10 {
        clients.push(ws::connect_with(app.addr, ORIGIN, &headers).await.unwrap());
    }
    let limited = refused(&app, "203.0.113.7").await;
    assert_eq!(limited.status, 429);
    assert_eq!(limited.text(), r#"{"error":"ws.rate_limited"}"#);
    assert_eq!(limited.header("retry-after"), Some("2"));
    assert_security_headers(&limited);
    assert!(limited.header("upgrade").is_none());
    // Another client is not affected, and the refused one is back after the refill.
    clients.push(
        ws::connect_with(app.addr, ORIGIN, &[("x-forwarded-for", "203.0.113.8")])
            .await
            .unwrap(),
    );
    clock.advance(Duration::from_secs(2));
    clients.push(ws::connect_with(app.addr, ORIGIN, &headers).await.unwrap());
    assert_eq!(ws_state.available_permits(), 2000 - 12);
}

#[tokio::test]
async fn a_full_key_table_refuses_new_keys_with_ws_rate_limited() {
    let (app, _, _) = start_limited(WsState::new(ORIGIN.to_owned(), true, 2000, SLOW), 1).await;
    let headers = [("x-forwarded-for", "203.0.113.7")];
    let _first = ws::connect_with(app.addr, ORIGIN, &headers).await.unwrap();
    let full = refused(&app, "203.0.113.8").await;
    assert_eq!(full.status, 429);
    assert_eq!(full.text(), r#"{"error":"ws.rate_limited"}"#);
    assert_eq!(full.header("retry-after"), Some("60"));
    // The existing key keeps its bucket.
    assert!(ws::connect_with(app.addr, ORIGIN, &headers).await.is_ok());
}

#[tokio::test]
async fn the_handshake_rejections_come_in_order() {
    // Origin before the rate: a foreign Origin with the quota drained is 403.
    let ws_state = WsState::new(ORIGIN.to_owned(), true, 2000, SLOW).with_ip_limit(1);
    let (app, _, _) = start_limited(ws_state, MAX_KEYS).await;
    let client = "203.0.113.7";
    let _socket = ws::connect_with(app.addr, ORIGIN, &[("x-forwarded-for", client)])
        .await
        .unwrap();
    // Rate before the per-IP cap: with the IP cap full, the next 9 upgrades are 503 and
    // drain the quota, and the 11th is 429.
    for _ in 0..9 {
        let response = refused(&app, client).await;
        assert_eq!(response.text(), r#"{"error":"ws.ip_capacity"}"#);
    }
    let response = refused(&app, client).await;
    assert_eq!(response.status, 429);
    assert_eq!(response.text(), r#"{"error":"ws.rate_limited"}"#);
    let foreign = http::send(
        app.addr,
        "GET",
        "/ws",
        &upgrade_from(client, "https://evil.example"),
    )
    .await;
    assert_eq!(foreign.status, 403);
    assert_eq!(foreign.text(), r#"{"error":"ws.origin_forbidden"}"#);

    // The per-IP cap before the global cap: both full gives ws.ip_capacity.
    let ws_state = WsState::new(ORIGIN.to_owned(), true, 1, SLOW).with_ip_limit(1);
    let (app, _, _) = start_limited(ws_state, MAX_KEYS).await;
    let _socket = ws::connect_with(app.addr, ORIGIN, &[("x-forwarded-for", client)])
        .await
        .unwrap();
    let response = refused(&app, client).await;
    assert_eq!(response.status, 503);
    assert_eq!(response.text(), r#"{"error":"ws.ip_capacity"}"#);
    // Another IP hits the global cap.
    let response = refused(&app, "203.0.113.8").await;
    assert_eq!(response.text(), r#"{"error":"ws.capacity"}"#);
}

#[tokio::test]
async fn an_invalid_upgrade_consumes_no_quota() {
    let (app, _, _) =
        start_limited(WsState::new(ORIGIN.to_owned(), true, 2000, SLOW), MAX_KEYS).await;
    let client = "203.0.113.7";
    for _ in 0..20 {
        let response = http::send(
            app.addr,
            "GET",
            "/ws",
            &[("origin", ORIGIN), ("x-forwarded-for", client)],
        )
        .await;
        assert!(response.status.is_client_error());
        assert_ne!(response.status, 429);
    }
    let mut clients = Vec::new();
    for _ in 0..10 {
        clients.push(
            ws::connect_with(app.addr, ORIGIN, &[("x-forwarded-for", client)])
                .await
                .unwrap(),
        );
    }
}
