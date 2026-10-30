//! RF-B12: unknown routes, read-only methods, no CORS, security headers, slow headers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, Instant};

use server::app::{HttpConfig, Settings};
use support::app::{ORIGIN, SECRET};
use support::http::{self, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn app() -> support::app::RunningApp {
    support::app::start(support::app::state(&[], &Settings::default())).await
}

/// ADR-0012 B2: each header once, with its exact value.
fn assert_security_headers(response: &Response) {
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("strict-transport-security", "max-age=31536000"),
        ("cross-origin-resource-policy", "same-origin"),
        (
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'",
        ),
    ] {
        let values: Vec<_> = response.headers.get_all(name).iter().collect();
        assert_eq!(values, [value], "{name}");
    }
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
    for path in ["/nada?x=1", "/api/nada"] {
        let response = http::get(app.addr, path).await;
        assert_eq!(response.status, 404, "{path}");
        assert_eq!(response.text(), r#"{"error":"http.not_found"}"#);
        assert_eq!(response.header("content-type"), Some("application/json"));
        assert_security_headers(&response);
    }
}

const UNSAFE: [&str; 5] = ["POST", "PUT", "PATCH", "DELETE", "PURGE"];
const PATHS: [&str; 2] = ["/api/health", "/cualquier"];

async fn app_with_secret() -> support::app::RunningApp {
    support::app::start(support::app::state(
        &[support::app::WITH_SECRET],
        &Settings::default(),
    ))
    .await
}

fn assert_csrf_403(response: &Response, what: &str) {
    assert_eq!(response.status, 403, "{what}");
    assert_eq!(response.text(), r#"{"error":"auth.csrf_origin"}"#, "{what}");
    assert_security_headers(response);
}

/// With a valid same-origin proof the router answers as before: 405 with `Allow` on a route
/// that exists, 404 on one that does not (RF-A5).
fn assert_routed(response: &Response, path: &str, what: &str) {
    if path == "/api/health" {
        assert_eq!(response.status, 405, "{what}");
        assert_eq!(response.text(), r#"{"error":"http.method_not_allowed"}"#);
        assert_eq!(response.header("allow"), Some("GET, HEAD, OPTIONS"));
    } else {
        assert_eq!(response.status, 404, "{what}");
        assert_eq!(response.text(), r#"{"error":"http.not_found"}"#);
    }
    assert_security_headers(response);
}

/// T2: without a same-origin proof, every unsafe method is 403 before routing.
#[tokio::test]
async fn unsafe_methods_without_a_same_origin_proof_are_403() {
    let app = app().await;
    let refused: [&[(&str, &str)]; 9] = [
        &[],
        &[("origin", "https://evil.example")],
        &[("origin", "null")],
        &[("origin", ORIGIN), ("origin", ORIGIN)],
        &[("origin", ORIGIN), ("origin", "https://evil.example")],
        &[("sec-fetch-site", "same-site")],
        &[("sec-fetch-site", "cross-site")],
        &[("sec-fetch-site", "none")],
        // Not internal without the secret: X-Client-Origin is ignored.
        &[("x-client-origin", ORIGIN)],
    ];
    for method in UNSAFE {
        for path in PATHS {
            for headers in refused {
                let response = http::send(app.addr, method, path, headers).await;
                assert_csrf_403(&response, &format!("{method} {path} {headers:?}"));
            }
        }
    }
}

#[tokio::test]
async fn unsafe_methods_with_a_same_origin_proof_keep_the_405_and_the_404() {
    let app = app().await;
    let accepted: [&[(&str, &str)]; 2] =
        [&[("origin", ORIGIN)], &[("sec-fetch-site", "same-origin")]];
    for method in UNSAFE {
        for path in PATHS {
            for headers in accepted {
                let response = http::send(app.addr, method, path, headers).await;
                assert_routed(&response, path, &format!("{method} {path} {headers:?}"));
            }
        }
    }
}

#[tokio::test]
async fn safe_methods_are_not_checked() {
    let app = app().await;
    let foreign = [("origin", "https://evil.example")];
    assert_eq!(
        http::send(app.addr, "GET", "/api/health", &foreign)
            .await
            .status,
        200
    );
    assert_eq!(
        http::send(app.addr, "HEAD", "/api/health", &foreign)
            .await
            .status,
        200
    );
    assert_eq!(
        http::send(app.addr, "OPTIONS", "/api/health", &foreign)
            .await
            .status,
        405
    );
    assert_eq!(
        http::send(app.addr, "GET", "/cualquier", &foreign)
            .await
            .status,
        404
    );
}

/// T15: an internal request needs `X-Client-Origin` and, if present, a same-origin
/// `X-Client-Fetch-Site`.
#[tokio::test]
async fn internal_requests_are_checked_on_the_forwarded_headers() {
    let app = app_with_secret().await;
    let secret = ("x-internal-secret", SECRET);
    let refused: [&[(&str, &str)]; 7] = [
        &[],
        // The browser's Origin does not count for an internal request.
        &[("origin", ORIGIN)],
        &[("x-client-origin", "https://evil.example")],
        &[("x-client-origin", ORIGIN), ("x-client-origin", ORIGIN)],
        &[
            ("x-client-origin", ORIGIN),
            ("x-client-fetch-site", "cross-site"),
        ],
        &[
            ("x-client-origin", ORIGIN),
            ("x-client-fetch-site", "same-site"),
        ],
        &[("x-client-origin", ORIGIN), ("x-client-fetch-site", "none")],
    ];
    let accepted: [&[(&str, &str)]; 3] = [
        &[("x-client-origin", ORIGIN)],
        &[
            ("x-client-origin", ORIGIN),
            ("x-client-fetch-site", "same-origin"),
        ],
        &[
            ("x-client-origin", ORIGIN),
            ("origin", "https://web.internal"),
        ],
    ];
    for method in UNSAFE {
        for path in PATHS {
            for headers in refused {
                let mut all = vec![secret];
                all.extend_from_slice(headers);
                let response = http::send(app.addr, method, path, &all).await;
                assert_csrf_403(&response, &format!("{method} {path} {headers:?}"));
            }
            for headers in accepted {
                let mut all = vec![secret];
                all.extend_from_slice(headers);
                let response = http::send(app.addr, method, path, &all).await;
                assert_routed(&response, path, &format!("{method} {path} {headers:?}"));
            }
        }
    }
}

/// T3 (Origin part): without a valid secret the request is external, so `X-Client-Origin`
/// alone is no proof.
#[tokio::test]
async fn a_wrong_or_repeated_secret_is_an_external_request() {
    let wrong = format!("{}A", &SECRET[..SECRET.len() - 1]);
    let with_secret = app_with_secret().await;
    for secrets in [vec![wrong.as_str()], vec![SECRET, SECRET], vec!["short"]] {
        let mut headers: Vec<(&str, &str)> = secrets
            .iter()
            .map(|value| ("x-internal-secret", *value))
            .collect();
        headers.push(("x-client-origin", ORIGIN));
        let response = http::send(with_secret.addr, "POST", "/api/health", &headers).await;
        assert_csrf_403(&response, &format!("{secrets:?}"));
    }
    // Another secret configured: this value is ignored.
    let other = support::app::start(support::app::state(
        &[support::app::OTHER_SECRET],
        &Settings::default(),
    ))
    .await;
    let headers = [("x-internal-secret", SECRET), ("x-client-origin", ORIGIN)];
    let response = http::send(other.addr, "POST", "/api/health", &headers).await;
    assert_csrf_403(&response, "another secret configured");
}

#[tokio::test]
async fn options_on_a_route_is_405_with_the_code() {
    let app = app().await;
    let response = http::send(app.addr, "OPTIONS", "/api/health", &[]).await;
    assert_eq!(response.status, 405);
    assert_eq!(response.text(), r#"{"error":"http.method_not_allowed"}"#);
    assert_security_headers(&response);
}

/// T21: no CORS anywhere, not even for a foreign preflight or a refused `POST` (RF-A8).
#[tokio::test]
async fn no_cors_even_for_a_foreign_preflight() {
    let app = app().await;
    let headers = [
        ("origin", "https://evil.example"),
        ("access-control-request-method", "POST"),
        ("access-control-request-headers", "content-type"),
    ];
    for path in ["/api/health", "/no-existe", "/ws"] {
        for method in ["OPTIONS", "GET"] {
            let response = http::send(app.addr, method, path, &headers).await;
            assert_no_cors(&response);
        }
    }
    let refused = http::send(app.addr, "POST", "/api/health", &headers).await;
    assert_eq!(refused.status, 403);
    assert_no_cors(&refused);
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
        support::app::state(&[], &Settings::default()),
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
