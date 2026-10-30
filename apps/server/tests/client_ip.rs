//! 008 T14 (server), T22 (IP) and T3 (IP part), through the `/ws` upgrade rate (burst 10):
//! which `IpKey` a request lands on, seen from outside.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::app::Settings;
use server::rate::{RateSettings, ServerClock};
use support::app::{ORIGIN, RunningApp, SECRET};
use support::http;

const UPGRADE: [(&str, &str); 5] = [
    ("connection", "Upgrade"),
    ("upgrade", "websocket"),
    ("sec-websocket-version", "13"),
    ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
    ("origin", ORIGIN),
];

async fn start(vars: &[(&str, &str)]) -> RunningApp {
    // A manual clock that never moves: the burst is all there is.
    let settings = Settings {
        rate: RateSettings {
            clock: ServerClock::manual(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    support::app::start(support::app::state(vars, &settings)).await
}

/// The status of one upgrade with `extra` headers (101 or a rejection).
async fn upgrade(app: &RunningApp, extra: &[(&str, &str)]) -> u16 {
    let mut headers = UPGRADE.to_vec();
    headers.extend_from_slice(extra);
    http::send(app.addr, "GET", "/ws", &headers)
        .await
        .status
        .as_u16()
}

/// Ten upgrades with `headers(i)` pass; the eleventh is 429 (one shared bucket).
async fn assert_one_bucket(app: &RunningApp, headers: impl Fn(usize) -> Vec<(String, String)>) {
    for i in 0..11 {
        let owned = headers(i);
        let pairs: Vec<(&str, &str)> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let expected = if i < 10 { 101 } else { 429 };
        assert_eq!(upgrade(app, &pairs).await, expected, "upgrade {}", i + 1);
    }
}

fn xff(value: String) -> Vec<(String, String)> {
    vec![("x-forwarded-for".to_owned(), value)]
}

#[tokio::test]
async fn an_untrusted_peer_cannot_choose_its_key_with_x_forwarded_for() {
    let app = start(&[]).await;
    assert_one_bucket(&app, |i| xff(format!("198.51.100.{i}"))).await;
}

#[tokio::test]
async fn behind_a_trusted_proxy_a_spoofed_left_entry_is_ignored() {
    let app = start(&[support::app::TRUST_LOOPBACK]).await;
    assert_one_bucket(&app, |i| xff(format!("198.51.100.{i}, 203.0.113.7"))).await;
    // The rightmost untrusted entry is the client: another one has its own bucket.
    assert_eq!(
        upgrade(&app, &[("x-forwarded-for", "203.0.113.8")]).await,
        101
    );
}

#[tokio::test]
async fn two_ipv6_of_the_same_64_share_a_bucket() {
    let app = start(&[support::app::TRUST_LOOPBACK]).await;
    assert_one_bucket(&app, |i| xff(format!("2001:db8:1:2::{i:x}"))).await;
    assert_eq!(
        upgrade(&app, &[("x-forwarded-for", "2001:db8:1:3::1")]).await,
        101
    );
}

#[tokio::test]
async fn a_wrong_secret_ignores_x_client_ip() {
    let app = start(&[support::app::WITH_SECRET]).await;
    let wrong = format!("{}A", &SECRET[..SECRET.len() - 1]);
    assert_one_bucket(&app, |i| {
        vec![
            ("x-internal-secret".to_owned(), wrong.clone()),
            ("x-client-ip".to_owned(), format!("198.51.100.{i}")),
        ]
    })
    .await;
    // A repeated secret is not internal either.
    let app = start(&[support::app::WITH_SECRET]).await;
    assert_one_bucket(&app, |i| {
        vec![
            ("x-internal-secret".to_owned(), SECRET.to_owned()),
            ("x-internal-secret".to_owned(), SECRET.to_owned()),
            ("x-client-ip".to_owned(), format!("198.51.100.{i}")),
        ]
    })
    .await;
    // Another deployment's secret: our value is ignored too.
    let app = start(&[support::app::OTHER_SECRET]).await;
    assert_one_bucket(&app, |i| {
        vec![
            ("x-internal-secret".to_owned(), SECRET.to_owned()),
            ("x-client-ip".to_owned(), format!("198.51.100.{i}")),
        ]
    })
    .await;
}

#[tokio::test]
async fn the_right_secret_uses_x_client_ip() {
    let app = start(&[support::app::WITH_SECRET]).await;
    for i in 0..15 {
        let ip = format!("198.51.100.{i}");
        let headers = [("x-internal-secret", SECRET), ("x-client-ip", ip.as_str())];
        assert_eq!(upgrade(&app, &headers).await, 101, "{ip}");
    }
}
