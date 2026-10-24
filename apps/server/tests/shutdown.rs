//! RF-B14: `SIGTERM` and `SIGINT` close every WebSocket with 1012 and exit with 0 in < 10 s.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, Instant};

use support::bin;
use support::db::TestDb;
use support::ws::{self, Ending};

const ORIGIN: &str = "http://localhost:5173";

async fn shutdown_with(signal: &str) {
    let db = TestDb::create().await;
    let url = db.url();
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("PORT", "0"),
            ("BIND_ADDR", "127.0.0.1"),
        ],
    );
    let port = server.wait_listening(Duration::from_secs(10)).await;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut client = ws::connect(addr, ORIGIN).await.unwrap();

    let started = Instant::now();
    server.signal(signal);
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(5)).await,
        Ending::Close(1012, String::new())
    );
    // Reading on lets the client answer the close.
    ws::assert_stream_ends(&mut client, Duration::from_secs(5)).await;

    let finished = server.finish(Duration::from_secs(10)).await;
    assert_eq!(
        finished.status.code(),
        Some(0),
        "{signal}: {:?}",
        finished.stdout
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert!(
        tokio::net::TcpStream::connect(addr).await.is_err(),
        "the port still accepts connections"
    );
    assert!(finished.stderr.is_empty(), "{:?}", finished.stderr);
}

#[tokio::test]
async fn sigterm_closes_sockets_with_1012_and_exits_with_zero() {
    shutdown_with("TERM").await;
}

#[tokio::test]
async fn sigint_closes_sockets_with_1012_and_exits_with_zero() {
    shutdown_with("INT").await;
}

#[tokio::test]
async fn new_connections_are_refused_right_after_the_signal() {
    let db = TestDb::create().await;
    let url = db.url();
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("PORT", "0"),
            ("BIND_ADDR", "127.0.0.1"),
        ],
    );
    let port = server.wait_listening(Duration::from_secs(10)).await;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    // A socket that never answers the close keeps the process alive for up to close_grace.
    let _silent = tokio::spawn(ws::silent_client_frames(
        addr,
        ORIGIN,
        Duration::from_secs(8),
        Duration::from_secs(8),
    ));
    tokio::time::sleep(Duration::from_millis(200)).await;
    server.signal("TERM");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        tokio::net::TcpStream::connect(addr).await.is_err(),
        "the listener must close right after the signal"
    );
    let finished = server.finish(Duration::from_secs(10)).await;
    assert_eq!(finished.status.code(), Some(0));
}
