//! RF-B15, RF-B16: log canaries against the real binary (stdout and stderr, whole run).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

use server::telemetry::ALLOWED_FIELDS;
use support::bin::{self, Finished};
use support::db::TestDb;
use support::http;
use support::ws;
use tokio_tungstenite::tungstenite::Message;

const ORIGIN: &str = "http://localhost:5173";

fn upgrade_headers(origin: &str) -> Vec<(&str, &str)> {
    vec![
        ("connection", "Upgrade"),
        ("upgrade", "websocket"),
        ("sec-websocket-version", "13"),
        ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
        ("origin", origin),
    ]
}

fn canary(name: &str) -> String {
    format!("canary-{name}-{}", uuid::Uuid::new_v4().simple())
}

fn json_lines(finished: &Finished) -> Vec<serde_json::Map<String, serde_json::Value>> {
    finished
        .stdout
        .iter()
        .map(
            |line| match serde_json::from_str::<serde_json::Value>(line) {
                Ok(serde_json::Value::Object(map)) => map,
                _ => panic!("not a JSON object line: {line}"),
            },
        )
        .collect()
}

fn assert_no_canary(finished: &Finished, secrets: &[&str]) {
    for line in finished.stdout.iter().chain(&finished.stderr) {
        for secret in secrets {
            assert!(!line.contains(secret), "{secret} leaked in: {line}");
        }
    }
}

async fn run_canaries(rust_log: &str) {
    let db = TestDb::create().await;
    let url = db.url();
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("ENABLE_WS_ECHO", "1"),
            ("PORT", "0"),
            ("BIND_ADDR", "127.0.0.1"),
            ("RUST_LOG", rust_log),
        ],
    );
    let port = server.wait_listening(Duration::from_secs(10)).await;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let cookie = canary("cookie");
    let authorization = canary("authorization");
    let internal = canary("internal");
    let client_ip = canary("clientip");
    let forwarded = canary("forwarded");
    let real_ip = canary("realip");
    let forwarded_rfc = canary("forwardedrfc");
    let forwarded_value = format!("for={forwarded_rfc}");
    let query = canary("query");
    let origin_host = canary("origin");
    let payload = canary("payload");
    let sensitive: Vec<(&str, &str)> = vec![
        ("cookie", &cookie),
        ("authorization", &authorization),
        ("x-internal-secret", &internal),
        ("x-client-ip", &client_ip),
        ("x-forwarded-for", &forwarded),
        ("x-real-ip", &real_ip),
        ("forwarded", &forwarded_value),
    ];
    let foreign_origin = format!("https://{origin_host}.example");

    // 1. /api/health with every sensitive header.
    assert_eq!(
        http::send(addr, "GET", "/api/health", &sensitive)
            .await
            .status,
        200
    );
    // 2. An unknown route with a query string.
    let path = format!("/no-existe?q={query}");
    assert_eq!(http::send(addr, "GET", &path, &sensitive).await.status, 404);
    // 3. The handshake with a foreign Origin.
    let mut headers = upgrade_headers(&foreign_origin);
    headers.extend(sensitive.iter().copied());
    assert_eq!(http::send(addr, "GET", "/ws", &headers).await.status, 403);
    // 4. An echo with a canary payload (one more HTTP request: the 101).
    let mut client = ws::connect(addr, ORIGIN).await.unwrap();
    ws::send_text(
        &mut client,
        &format!(r#"{{"type":"echo","payload":"{payload}"}}"#),
    )
    .await;
    assert!(matches!(
        ws::next_data(&mut client, Duration::from_secs(2)).await,
        Some(Message::Text(_))
    ));
    client.close(None).await.unwrap();
    ws::assert_stream_ends(&mut client, Duration::from_secs(2)).await;
    // 5. An extension method.
    assert_eq!(
        http::send(addr, "PURGE", "/api/health", &sensitive)
            .await
            .status,
        405
    );
    // 6. /ws with a valid Origin and no upgrade.
    let no_upgrade = http::send(addr, "GET", "/ws", &[("origin", ORIGIN)]).await;
    assert!(no_upgrade.status.is_client_error());
    const HTTP_REQUESTS: usize = 6;

    server.signal("TERM");
    let finished = server.finish(Duration::from_secs(10)).await;
    assert_eq!(finished.status.code(), Some(0));

    assert_no_canary(
        &finished,
        &[
            &cookie,
            &authorization,
            &internal,
            &client_ip,
            &forwarded,
            &real_ip,
            &forwarded_rfc,
            &query,
            &origin_host,
            &payload,
            "no-existe",
        ],
    );
    assert!(finished.stderr.is_empty(), "stderr: {:?}", finished.stderr);

    let lines = json_lines(&finished);
    let allowed: HashSet<&str> = ALLOWED_FIELDS
        .iter()
        .copied()
        .chain(["ts", "level"])
        .collect();
    let mut request_ids = HashSet::new();
    let mut routes = Vec::new();
    for line in &lines {
        if line.contains_key("target") {
            let mut keys: Vec<&str> = line.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["event", "level", "target", "ts"], "{line:?}");
            assert_eq!(line["event"], "external");
            continue;
        }
        for key in line.keys() {
            assert!(allowed.contains(key.as_str()), "field {key} in {line:?}");
        }
        if line["event"] == "http.request" {
            assert_eq!(line["level"], "info");
            let method = line["method"].as_str().unwrap();
            assert!(
                [
                    "GET", "HEAD", "OPTIONS", "POST", "PUT", "PATCH", "DELETE", "other"
                ]
                .contains(&method),
                "{method}"
            );
            let route = line["route"].as_str().unwrap();
            assert!(
                ["/api/health", "unmatched", "/ws"].contains(&route),
                "{route}"
            );
            routes.push((
                method.to_owned(),
                route.to_owned(),
                line["status"].as_u64().unwrap(),
            ));
            let id = line["request_id"].as_str().unwrap();
            assert!(uuid::Uuid::parse_str(id).is_ok(), "{id}");
            assert!(request_ids.insert(id.to_owned()), "repeated request_id");
            assert!(line["latency_ms"].is_u64());
        }
    }
    assert_eq!(routes.len(), HTTP_REQUESTS, "{routes:?}");
    // RF-B16: /api/health is logged like any other request.
    assert!(routes.contains(&("GET".into(), "/api/health".into(), 200)));
    assert!(routes.contains(&("GET".into(), "unmatched".into(), 404)));
    assert!(routes.contains(&("GET".into(), "/ws".into(), 403)));
    assert!(routes.contains(&("GET".into(), "/ws".into(), 101)));
    assert!(routes.contains(&("other".into(), "/api/health".into(), 405)));
    assert!(
        routes
            .iter()
            .any(|(m, r, s)| m == "GET" && r == "/ws" && (400..500).contains(s) && *s != 403)
    );
    // The echo warning at startup.
    assert!(
        lines
            .iter()
            .any(|l| l["event"] == "ws.echo_enabled" && l["level"] == "warn"),
        "{lines:?}"
    );
}

#[tokio::test]
async fn canaries_with_info() {
    run_canaries("info").await;
}

#[tokio::test]
async fn canaries_with_debug() {
    run_canaries("debug").await;
}

#[tokio::test]
async fn canaries_with_explicit_trace_for_third_parties() {
    run_canaries("debug,sqlx=trace,tungstenite=trace").await;
}

#[tokio::test]
async fn the_database_password_is_never_logged() {
    let password = canary("password");
    let port = support::free_port();
    let url = format!("postgres://torneo:{password}@127.0.0.1:{port}/torneo");
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("PORT", "0"),
            ("RUST_LOG", "debug,sqlx=trace"),
        ],
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    let finished = server.kill().await;
    assert!(
        finished
            .stdout
            .iter()
            .any(|l| l.contains("\"db.unavailable\"")),
        "{:?}",
        finished.stdout
    );
    assert_no_canary(&finished, &[&password, &format!("127.0.0.1:{port}")]);
    assert!(finished.stderr.is_empty(), "{:?}", finished.stderr);
}
