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

mod in_process {

    use std::time::Duration;

    use server::app::{HttpConfig, Settings, run_until_shutdown};

    use server::ws::{WsConfig, WsState};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use super::ORIGIN;
    use super::support::ws::{self, Ending};

    /// A signal right after the handshake: the socket upgraded at that moment must still
    /// get 1012 (it is in the tracker from the handshake on), never a bare TCP close.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_socket_upgraded_right_before_the_signal_gets_1012() {
        for attempt in 0..100 {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let config = WsConfig {
                close_grace: Duration::from_secs(2),
                ..WsConfig::default()
            };
            let ws_state = WsState::new(ORIGIN.to_owned(), true, 10, config);
            let state = super::support::app::state_with_ws(&[], &Settings::default(), ws_state);
            let (signal, signalled) = oneshot::channel::<()>();
            let server = tokio::spawn(run_until_shutdown(
                listener,
                state,
                HttpConfig::default(),
                async move {
                    let _ = signalled.await;
                },
            ));
            let mut client = ws::connect(addr, ORIGIN).await.unwrap();
            signal.send(()).unwrap();
            let ending = ws::ending(&mut client, Duration::from_secs(3)).await;
            assert_eq!(
                ending,
                Ending::Close(1012, String::new()),
                "attempt {attempt}"
            );
            ws::assert_stream_ends(&mut client, Duration::from_secs(3)).await;
            tokio::time::timeout(Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }
}
