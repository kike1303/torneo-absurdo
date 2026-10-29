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

/// A random IPv6 in the documentation range, as a canary that is also a valid IP.
fn canary_ipv6() -> String {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let group = |i: usize| u16::from_be_bytes([bytes[i], bytes[i + 1]]);
    format!(
        "2001:db8:{:x}:{:x}::{:x}",
        group(0) | 1,
        group(2) | 1,
        group(4) | 1
    )
}

/// A test-only `INTERNAL_API_SECRET` (32 random bytes, base64url without padding).
fn test_secret() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let a = uuid::Uuid::new_v4().into_bytes();
    let b = uuid::Uuid::new_v4().into_bytes();
    // 44 characters of 6 bits: 33 bytes once decoded.
    a.iter()
        .chain(&b)
        .cycle()
        .take(44)
        .map(|byte| char::from(ALPHABET[usize::from(byte % 64)]))
        .collect()
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
    let secret = test_secret();
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("ENABLE_WS_ECHO", "1"),
            ("PORT", "0"),
            ("BIND_ADDR", "127.0.0.1"),
            ("RUST_LOG", rust_log),
            ("TRUSTED_PROXIES", "127.0.0.1/32"),
            ("INTERNAL_API_SECRET", &secret),
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
    // 5. An extension method, with a valid Origin (so it reaches the 405).
    let mut purge = sensitive.clone();
    purge.push(("origin", ORIGIN));
    assert_eq!(
        http::send(addr, "PURGE", "/api/health", &purge)
            .await
            .status,
        405
    );
    // 6. /ws with a valid Origin and no upgrade.
    let no_upgrade = http::send(addr, "GET", "/ws", &[("origin", ORIGIN)]).await;
    assert!(no_upgrade.status.is_client_error());

    // 008 RF-A9. 7. An IPv6 canary in X-Forwarded-For (from the trusted 127.0.0.1): ten
    // accepted upgrades of its /64 and the eleventh with 429.
    let forwarded_ip = canary_ipv6();
    let mut headers = upgrade_headers(ORIGIN);
    headers.push(("x-forwarded-for", &forwarded_ip));
    for _ in 0..10 {
        assert_eq!(http::send(addr, "GET", "/ws", &headers).await.status, 101);
    }
    assert_eq!(http::send(addr, "GET", "/ws", &headers).await.status, 429);
    // 8. Another one in X-Client-Ip with the valid secret.
    let internal_ip = canary_ipv6();
    let mut headers = upgrade_headers(ORIGIN);
    headers.push(("x-internal-secret", &secret));
    headers.push(("x-client-ip", &internal_ip));
    assert_eq!(http::send(addr, "GET", "/ws", &headers).await.status, 101);
    // 9. X-Client-Origin and X-Client-Fetch-Site canaries with the valid secret (403).
    let client_origin = canary("clientorigin");
    let client_fetch_site = canary("clientfetchsite");
    let headers = [
        ("x-internal-secret", secret.as_str()),
        ("x-client-origin", client_origin.as_str()),
    ];
    assert_eq!(
        http::send(addr, "POST", "/api/health", &headers)
            .await
            .status,
        403
    );
    let headers = [
        ("x-internal-secret", secret.as_str()),
        ("x-client-origin", ORIGIN),
        ("x-client-fetch-site", client_fetch_site.as_str()),
    ];
    assert_eq!(
        http::send(addr, "POST", "/api/health", &headers)
            .await
            .status,
        403
    );
    // 10. A Sec-Fetch-Site canary without Origin (403).
    let fetch_site = canary("fetchsite");
    assert_eq!(
        http::send(
            addr,
            "POST",
            "/api/health",
            &[("sec-fetch-site", &fetch_site)]
        )
        .await
        .status,
        403
    );
    const HTTP_REQUESTS: usize = 6 + 11 + 1 + 3;

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
            &forwarded_ip,
            &internal_ip,
            "2001:db8",
            &client_origin,
            &client_fetch_site,
            &fetch_site,
            &secret,
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
    assert!(routes.contains(&("GET".into(), "/ws".into(), 429)));
    assert_eq!(
        routes
            .iter()
            .filter(|(m, _, s)| m == "POST" && *s == 403)
            .count(),
        3
    );
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
