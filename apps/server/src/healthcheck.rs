//! `server healthcheck` (RF-B11, plan-b decision 12).
//!
//! `GET http://127.0.0.1:${PORT}/api/health` with hyper's HTTP/1 client (no TLS, no new
//! client crate) and a 2 s timeout for the whole exchange. Exit 0 only on a 200. It reads only
//! `PORT`, never the body of the response, and writes nothing to stdout or stderr: Docker
//! keeps the healthcheck output in `docker inspect`.
//!
//! It always calls `127.0.0.1`, so it assumes `BIND_ADDR` is `0.0.0.0` or `127.0.0.1`.

use std::net::Ipv4Addr;
use std::time::Duration;

use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::{Request, StatusCode, header};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::config::DEFAULT_PORT;

/// Timeout of the whole exchange (connect, request, status line and headers).
pub const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the health check failed")]
pub struct HealthcheckFailed;

/// Reads `PORT` (default 8080) and runs the check.
pub fn run_from_env() -> Result<(), HealthcheckFailed> {
    let port = match std::env::var_os("PORT") {
        None => DEFAULT_PORT,
        Some(raw) => raw
            .to_str()
            .and_then(|text| text.parse::<u16>().ok())
            .ok_or(HealthcheckFailed)?,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| HealthcheckFailed)?;
    runtime.block_on(check(port, TIMEOUT))
}

/// Ok only if `GET /api/health` on `127.0.0.1:port` answers 200 within `timeout`.
pub async fn check(port: u16, timeout: Duration) -> Result<(), HealthcheckFailed> {
    match tokio::time::timeout(timeout, status(port)).await {
        Ok(Some(StatusCode::OK)) => Ok(()),
        _ => Err(HealthcheckFailed),
    }
}

async fn status(port: u16) -> Option<StatusCode> {
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.ok()?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .ok()?;
    let driver = tokio::spawn(connection);
    let request = Request::get("/api/health")
        // hyper's HTTP/1 client does not add `Host`.
        .header(header::HOST, format!("127.0.0.1:{port}"))
        .body(Empty::<Bytes>::new())
        .ok()?;
    let status = sender.send_request(request).await.ok().map(|r| r.status());
    // The body is never read: dropping the connection discards it.
    driver.abort();
    status
}
