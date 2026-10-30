//! 008 RF-B17 (T13 extended, ADR-0012 A8): a whole login with the dev provider, `/api/me`, a
//! socket with a session, a logout and a bad callback, against the real binary, in the three
//! log levels. None of the secrets or identifiers of the run reaches stdout or stderr, and
//! every line only has allowed fields.
#![cfg(feature = "dev-auth")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

use base64::Engine as _;
use server::sessions::TokenHash;
use server::telemetry::ALLOWED_FIELDS;
use sqlx::Row;
use support::auth::{FLOW, SESSION};
use support::bin;
use support::db::TestDb;
use support::http;
use support::ws;
use tokio_tungstenite::tungstenite::Message;

const ORIGIN: &str = "http://localhost:5173";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn query_param(location: &str, name: &str) -> String {
    let url = url::Url::parse(&format!(
        "http://x{}",
        location.trim_start_matches("http://localhost:5173")
    ))
    .unwrap();
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
        .unwrap()
}

async fn run(rust_log: &str) {
    let db = TestDb::create().await;
    let url = db.url();
    let pool = db.migrated_pool().await;
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("PORT", "0"),
            ("RUST_LOG", rust_log),
            ("TRUSTED_PROXIES", "127.0.0.1/32"),
        ],
    );
    let port = server.wait_listening(Duration::from_secs(10)).await;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let identity = format!("cnry{}", &uuid::Uuid::new_v4().simple().to_string()[..24]);
    let ip = format!("2001:db8:{:x}::7", rand_u16());
    let form = support::auth::form(&[("dev_identity", &identity), ("next", "/es/onboarding")]);
    let started = http::send_body(
        addr,
        "POST",
        "/api/auth/dev/start",
        &[
            ("origin", ORIGIN),
            ("content-type", "application/x-www-form-urlencoded"),
            ("x-forwarded-for", &ip),
        ],
        &form,
    )
    .await;
    assert_eq!(started.status, 303, "{}", started.text());
    let flow = started.cookie(FLOW).unwrap().to_owned();
    let row = sqlx::query("SELECT state, code_verifier FROM oauth_flows")
        .fetch_one(&pool)
        .await
        .unwrap();
    let state: String = row.get(0);
    let verifier: String = row.get(1);
    let authorized = http::send(addr, "GET", &started.location_path(), &[]).await;
    assert_eq!(authorized.status, 303);
    let code = query_param(authorized.header("location").unwrap(), "code");
    let flow_cookie = format!("{FLOW}={flow}");
    let done = http::send(
        addr,
        "GET",
        &authorized.location_path(),
        &[("cookie", &flow_cookie), ("x-forwarded-for", &ip)],
    )
    .await;
    assert_eq!(done.status, 303, "{:?}", done.header("location"));
    let token = done.cookie(SESSION).unwrap().to_owned();
    let session = format!("{SESSION}={token}");

    let internal = format!("canary-internal-{}", uuid::Uuid::new_v4().simple());
    let me = http::send(
        addr,
        "GET",
        "/api/me",
        &[("cookie", &session), ("x-internal-secret", &internal)],
    )
    .await;
    assert_eq!(me.status, 200);
    let display_name =
        serde_json::from_str::<serde_json::Value>(&me.text()).unwrap()["display_name"]
            .as_str()
            .unwrap()
            .to_owned();

    let mut socket = ws::connect_with(addr, ORIGIN, &[("cookie", &session)])
        .await
        .unwrap();
    ws::send_text(&mut socket, r#"{"type":"subscribe","channel":"self"}"#).await;
    assert!(matches!(
        ws::next_data(&mut socket, Duration::from_secs(2)).await,
        Some(Message::Text(_))
    ));

    // The identifiers of the run, read before the logout deletes the session.
    let user_id: uuid::Uuid = sqlx::query("SELECT id FROM users")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    let session_id: uuid::Uuid = sqlx::query("SELECT id FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    let ip_hash: Vec<u8> = sqlx::query("SELECT ip_hash FROM auth_events LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);

    let out = http::send(
        addr,
        "POST",
        "/api/auth/logout",
        &[("origin", ORIGIN), ("cookie", &session)],
    )
    .await;
    assert_eq!(out.status, 303);
    ws::ending(&mut socket, Duration::from_secs(5)).await;
    // A bad callback with canaries in the query.
    let bad_code = format!("canary-code-{}", uuid::Uuid::new_v4().simple());
    let bad = http::send(
        addr,
        "GET",
        &format!("/api/auth/dev/callback?code={bad_code}&state={bad_code}"),
        &[],
    )
    .await;
    assert_eq!(
        bad.header("location"),
        Some("/login?error=auth.invalid_state")
    );

    // The quota branch of the callback (008 "Enmienda 2026-10-30" of RF-B5, security-reviewer):
    // a client of its own /64 drains AUTH_IP, and the refused callback carries canaries in the
    // query and the flow cookie. Neither the layer nor the request log may write them.
    let quota_ip = format!("2001:db8:{:x}::9", rand_u16() ^ 2);
    let quota_canary = format!("canary-quota-{}", uuid::Uuid::new_v4().simple());
    let quota_cookie = format!("{FLOW}={}", quota_canary.replace('-', "_"));
    let quota_path = format!("/api/auth/dev/callback?code={quota_canary}&state={quota_canary}");
    let quota_headers = [
        ("cookie", quota_cookie.as_str()),
        ("x-forwarded-for", quota_ip.as_str()),
    ];
    let mut refused = None;
    for _ in 0..6 {
        let response = http::send(addr, "GET", &quota_path, &quota_headers).await;
        assert_eq!(response.status, 303);
        if response.header("location") == Some("/login?error=auth.try_again") {
            refused = Some(response);
            break;
        }
    }
    let refused = refused.expect("the quota never refused the callback");
    assert_eq!(refused.cookie(FLOW), Some(""));

    server.signal("TERM");
    let finished = server.finish(Duration::from_secs(10)).await;
    assert_eq!(finished.status.code(), Some(0));

    let hash = TokenHash::of(&token).0;
    let canaries = [
        token.clone(),
        flow,
        code,
        state,
        verifier,
        server::auth::dev::ACCESS_TOKEN_CANARY.to_owned(),
        hex(&hash),
        base64::engine::general_purpose::STANDARD.encode(hash),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash),
        internal,
        ip,
        "2001:db8".to_owned(),
        hex(&ip_hash),
        user_id.to_string(),
        user_id.simple().to_string(),
        identity,
        display_name,
        session_id.to_string(),
        session_id.simple().to_string(),
        bad_code,
        quota_canary.clone(),
        quota_canary.replace('-', "_"),
    ];
    for line in finished.stdout.iter().chain(&finished.stderr) {
        for canary in &canaries {
            assert!(
                !line.contains(canary.as_str()),
                "{canary} leaked in: {line}"
            );
        }
        assert!(!line.contains('?'), "a query string in: {line}");
    }
    assert!(finished.stderr.is_empty(), "{:?}", finished.stderr);
    let allowed: HashSet<&str> = ALLOWED_FIELDS
        .iter()
        .copied()
        .chain(["ts", "level", "target"])
        .collect();
    let mut routes = HashSet::new();
    for line in &finished.stdout {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        let map = value.as_object().unwrap();
        for key in map.keys() {
            assert!(allowed.contains(key.as_str()), "field {key} in {line}");
        }
        if map.get("event").and_then(|e| e.as_str()) == Some("http.request") {
            routes.insert(map["route"].as_str().unwrap().to_owned());
        }
    }
    for route in [
        "/api/auth/{provider}/start",
        "/api/auth/dev/oauth/authorize",
        "/api/auth/{provider}/callback",
        "/api/auth/dev/oauth/token",
        "/api/auth/dev/oauth/userinfo",
        "/api/me",
        "/ws",
        "/api/auth/logout",
    ] {
        assert!(routes.contains(route), "{route} not in {routes:?}");
    }
}

fn rand_u16() -> u16 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    u16::from_be_bytes([bytes[0], bytes[1]]) | 1
}

#[tokio::test]
async fn login_canaries_with_info() {
    run("info").await;
}

#[tokio::test]
async fn login_canaries_with_debug() {
    run("debug").await;
}

#[tokio::test]
async fn login_canaries_with_explicit_trace_for_third_parties() {
    run("debug,sqlx=trace,tungstenite=trace").await;
}
