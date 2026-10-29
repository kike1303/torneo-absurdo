//! The HTTP client of the OAuth flow (008 plan-b decision 1): `oauth2`'s `AsyncHttpClient` over
//! hyper. Phase B only talks to the dev provider of the same process, so it only accepts
//! `http` to a loopback IP; phase C adds TLS for Google and Discord.
//!
//! No personal data ever leaves in a request: the only headers it adds are `Host` and a generic
//! `User-Agent`. It never follows redirects, gives up after 10 s and reads at most 64 KB.

use std::net::IpAddr;
use std::time::Duration;

use axum::http::{self, HeaderValue, Uri, header};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
pub use oauth2::{HttpRequest, HttpResponse};
use tokio::net::TcpStream;

/// The only `User-Agent` the server sends: generic, with no version and no host.
pub const USER_AGENT: &str = "torneo-absurdo-server";
pub const TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the URL is not http to a loopback IP")]
    Refused,
    #[error("the request timed out")]
    Timeout,
    #[error("the response body is too large")]
    TooLarge,
    #[error("the connection failed")]
    Io,
    #[error("the request or the response is not valid HTTP")]
    Http,
}

/// Sends `request` with [`TIMEOUT`]. The signature `oauth2` expects of an async client.
pub async fn send(request: HttpRequest) -> Result<HttpResponse, ClientError> {
    send_with(request, TIMEOUT).await
}

pub async fn send_with(
    request: HttpRequest,
    timeout: Duration,
) -> Result<HttpResponse, ClientError> {
    tokio::time::timeout(timeout, exchange(request))
        .await
        .map_err(|_| ClientError::Timeout)?
}

/// `http://<loopback IP>[:port]`, nothing else (no DNS, no TLS in phase B).
fn target(uri: &Uri) -> Result<(IpAddr, u16), ClientError> {
    if uri.scheme_str() != Some("http") {
        return Err(ClientError::Refused);
    }
    let host = uri.host().ok_or(ClientError::Refused)?;
    let ip: IpAddr = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .map_err(|_| ClientError::Refused)?;
    if !ip.is_loopback() {
        return Err(ClientError::Refused);
    }
    Ok((ip, uri.port_u16().unwrap_or(80)))
}

async fn exchange(request: HttpRequest) -> Result<HttpResponse, ClientError> {
    let (mut parts, body) = request.into_parts();
    let (ip, port) = target(&parts.uri)?;
    let authority = parts.uri.authority().ok_or(ClientError::Refused)?.clone();
    // Origin-form on the wire, with `Host` and the generic `User-Agent`.
    let path = parts
        .uri
        .path_and_query()
        .map_or_else(|| "/".to_owned(), |p| p.as_str().to_owned());
    parts.uri = path.parse().map_err(|_| ClientError::Http)?;
    parts.headers.insert(
        header::HOST,
        HeaderValue::from_str(authority.as_str()).map_err(|_| ClientError::Http)?,
    );
    parts
        .headers
        .insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
    let request = http::Request::from_parts(parts, Full::new(Bytes::from(body)));

    let stream = TcpStream::connect((ip, port))
        .await
        .map_err(|_| ClientError::Io)?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|_| ClientError::Io)?;
    tokio::spawn(connection);
    let response = sender
        .send_request(request)
        .await
        .map_err(|_| ClientError::Io)?;
    let (parts, body) = response.into_parts();
    let bytes = Limited::new(body, MAX_BODY_BYTES)
        .collect()
        .await
        .map_err(|error| {
            if error
                .downcast_ref::<http_body_util::LengthLimitError>()
                .is_some()
            {
                ClientError::TooLarge
            } else {
                ClientError::Io
            }
        })?
        .to_bytes();
    Ok(http::Response::from_parts(parts, bytes.to_vec()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// A one-shot server that answers `reply` and returns what it received.
    async fn server(reply: Vec<u8>, silent: bool) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 8192];
            let n = stream.read(&mut buffer).await.unwrap();
            buffer.truncate(n);
            if silent {
                tokio::time::sleep(Duration::from_secs(5)).await;
            } else {
                stream.write_all(&reply).await.unwrap();
            }
            buffer
        });
        (format!("http://{addr}"), task)
    }

    fn get(url: &str) -> HttpRequest {
        http::Request::builder()
            .method("GET")
            .uri(url)
            .header("accept", "application/json")
            .body(Vec::new())
            .unwrap()
    }

    #[tokio::test]
    async fn sends_only_a_generic_user_agent_and_the_given_headers() {
        let (base, task) = server(
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\ncontent-type: application/json\r\n\r\n{}"
                .to_vec(),
            false,
        )
        .await;
        let response = send(get(&format!("{base}/x?y=1"))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), b"{}");
        let request = String::from_utf8(task.await.unwrap())
            .unwrap()
            .to_ascii_lowercase();
        assert!(request.starts_with("get /x?y=1 http/1.1\r\n"), "{request}");
        assert!(
            request.contains("\r\nuser-agent: torneo-absurdo-server\r\n"),
            "{request}"
        );
        let headers: Vec<&str> = request
            .split("\r\n")
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(|l| l.split(':').next().unwrap())
            .collect();
        let mut sorted = headers.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, ["accept", "host", "user-agent"], "{request}");
    }

    #[tokio::test]
    async fn never_follows_a_redirect() {
        let (base, _task) = server(
            b"HTTP/1.1 303 See Other\r\nlocation: http://192.0.2.1/\r\ncontent-length: 0\r\n\r\n"
                .to_vec(),
            false,
        )
        .await;
        let response = send(get(&base)).await.unwrap();
        assert_eq!(response.status(), 303);
    }

    #[tokio::test]
    async fn cuts_a_body_over_64_kb() {
        let mut reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )
        .into_bytes();
        reply.extend(vec![b'x'; MAX_BODY_BYTES + 1]);
        let (base, _task) = server(reply, false).await;
        assert!(matches!(send(get(&base)).await, Err(ClientError::TooLarge)));
    }

    #[tokio::test]
    async fn gives_up_at_the_timeout() {
        let (base, _task) = server(Vec::new(), true).await;
        let result = send_with(get(&base), Duration::from_millis(200)).await;
        assert!(matches!(result, Err(ClientError::Timeout)));
    }

    #[tokio::test]
    async fn refuses_anything_but_http_to_a_loopback_ip() {
        for url in [
            "https://127.0.0.1/x",
            "http://192.0.2.1/x",
            "http://localhost/x",
            "http://[2001:db8::1]/x",
            "ftp://127.0.0.1/x",
        ] {
            assert!(
                matches!(send(get(url)).await, Err(ClientError::Refused)),
                "{url}"
            );
        }
    }
}
