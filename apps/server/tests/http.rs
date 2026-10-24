//! RF-B12: unknown routes, read-only methods, no CORS, security headers, slow headers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use server::app::{AppState, HttpConfig, Settings};
use server::db::{DbProbe, PingFuture};
use support::http::{self, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct AlwaysUp;

impl DbProbe for AlwaysUp {
    fn ping(&self) -> PingFuture<'_> {
        Box::pin(async { true })
    }
}

async fn app() -> support::app::RunningApp {
    support::app::start(AppState::new(
        Arc::new(AlwaysUp),
        &Settings::default(),
        support::app::ws_state(),
    ))
    .await
}

fn assert_security_headers(response: &Response) {
    assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        response.header("strict-transport-security"),
        Some("max-age=31536000")
    );
}

fn assert_no_cors(response: &Response) {
    for name in response.headers.keys() {
        assert!(
            !name.as_str().starts_with("access-control-"),
            "unexpected {name}"
        );
    }
}

#[tokio::test]
async fn unknown_route_is_404_with_a_code() {
    let app = app().await;
    let response = http::get(app.addr, "/nada?x=1").await;
    assert_eq!(response.status, 404);
    assert_eq!(response.text(), r#"{"error":"http.not_found"}"#);
    assert_eq!(response.header("content-type"), Some("application/json"));
    assert_security_headers(&response);
}

#[tokio::test]
async fn write_methods_are_405_everywhere() {
    let app = app().await;
    for (method, path) in [
        ("POST", "/api/health"),
        ("DELETE", "/cualquier"),
        ("PUT", "/api/health"),
        ("PATCH", "/nada"),
        ("PURGE", "/api/health"),
    ] {
        let response = http::send(app.addr, method, path, &[]).await;
        assert_eq!(response.status, 405, "{method} {path}");
        assert_eq!(response.text(), r#"{"error":"http.method_not_allowed"}"#);
        assert_eq!(response.header("allow"), Some("GET, HEAD, OPTIONS"));
        assert_security_headers(&response);
    }
}

#[tokio::test]
async fn options_on_a_route_is_405_with_the_code() {
    let app = app().await;
    let response = http::send(app.addr, "OPTIONS", "/api/health", &[]).await;
    assert_eq!(response.status, 405);
    assert_eq!(response.text(), r#"{"error":"http.method_not_allowed"}"#);
    assert_security_headers(&response);
}

#[tokio::test]
async fn no_cors_even_for_a_foreign_preflight() {
    let app = app().await;
    let headers = [
        ("origin", "https://evil.example"),
        ("access-control-request-method", "POST"),
    ];
    for method in ["OPTIONS", "GET"] {
        let response = http::send(app.addr, method, "/api/health", &headers).await;
        assert_no_cors(&response);
    }
}

#[tokio::test]
async fn security_headers_on_a_200_and_head_works() {
    let app = app().await;
    let ok = http::get(app.addr, "/api/health").await;
    assert_eq!(ok.status, 200);
    assert_security_headers(&ok);
    assert!(
        !ok.header("strict-transport-security")
            .unwrap()
            .contains("includeSubDomains")
    );
    let head = http::send(app.addr, "HEAD", "/api/health", &[]).await;
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
}

#[tokio::test]
async fn slow_headers_are_cut_after_the_header_read_timeout() {
    let app = support::app::start_with(
        AppState::new(
            Arc::new(AlwaysUp),
            &Settings::default(),
            support::app::ws_state(),
        ),
        HttpConfig {
            header_read_timeout: Duration::from_millis(300),
        },
    )
    .await;
    let mut stream = TcpStream::connect(app.addr).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n")
        .await
        .unwrap();
    let started = Instant::now();
    let mut buffer = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut buffer)).await;
    assert!(read.is_ok(), "the server kept the connection open");
    assert!(started.elapsed() < Duration::from_secs(2));
}
