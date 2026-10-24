//! RF-B11: `server healthcheck` against the real binary.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, Instant};

use support::bin::{self, Finished};
use support::db::TestDb;
use support::pg_proxy::PgProxy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn healthcheck(port: u16) -> (Finished, Duration) {
    bin::warm_up().await;
    let started = Instant::now();
    let port = port.to_string();
    let finished = bin::spawn(&["healthcheck"], &[("PORT", &port)])
        .finish(Duration::from_secs(10))
        .await;
    (finished, started.elapsed())
}

fn assert_silent(finished: &Finished) {
    assert!(finished.stdout.is_empty(), "stdout: {:?}", finished.stdout);
    assert!(finished.stderr.is_empty(), "stderr: {:?}", finished.stderr);
}

#[tokio::test]
async fn zero_with_the_database_up_and_one_with_it_cut() {
    let db = TestDb::create().await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let url = db.url_via(proxy.addr());
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", "http://localhost:5173"),
            ("PORT", "0"),
            ("BIND_ADDR", "127.0.0.1"),
        ],
    );
    let port = server.wait_listening(Duration::from_secs(10)).await;

    let (up, _) = healthcheck(port).await;
    assert_eq!(up.status.code(), Some(0));
    assert_silent(&up);

    proxy.cut();
    // The binary uses the default 1 s cache.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (down, _) = healthcheck(port).await;
    assert_eq!(down.status.code(), Some(1));
    assert_silent(&down);
    server.kill().await;
}

#[tokio::test]
async fn one_without_a_server() {
    let (finished, elapsed) = healthcheck(support::free_port()).await;
    assert_eq!(finished.status.code(), Some(1));
    assert!(elapsed < Duration::from_millis(2500), "{elapsed:?}");
    assert_silent(&finished);
}

#[tokio::test]
async fn one_against_a_server_that_never_answers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let hold = tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    });
    let (finished, elapsed) = healthcheck(port).await;
    assert_eq!(finished.status.code(), Some(1));
    assert!(elapsed <= Duration::from_millis(2500), "{elapsed:?}");
    assert_silent(&finished);
    hold.abort();
}

#[tokio::test]
async fn sends_host_and_never_reads_the_body() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_host = format!("host: 127.0.0.1:{port}\r\n");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
        }
        let text = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(text.starts_with("get /api/health http/1.1\r\n"), "{text}");
        assert!(text.contains(&expected_host), "{text}");
        // 200 with a body that never ends.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        let chunk = vec![b'x'; 64 * 1024];
        loop {
            let frame = [
                format!("{:x}\r\n", chunk.len()).into_bytes(),
                chunk.clone(),
                b"\r\n".to_vec(),
            ]
            .concat();
            if stream.write_all(&frame).await.is_err() {
                return;
            }
        }
    });
    let (finished, elapsed) = healthcheck(port).await;
    assert_eq!(finished.status.code(), Some(0));
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    assert_silent(&finished);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
