//! Minimal HTTP/1.1 client over hyper for the tests.

use std::net::SocketAddr;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{HeaderMap, Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8(self.body.to_vec()).unwrap()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|v| v.to_str().unwrap())
    }

    /// Every `Set-Cookie` line.
    pub fn set_cookies(&self) -> Vec<&str> {
        self.headers
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect()
    }

    /// The value a `Set-Cookie` gives to `name` (`Some("")` if it expires it).
    pub fn cookie(&self, name: &str) -> Option<&str> {
        let prefix = format!("{name}=");
        self.set_cookies().into_iter().find_map(|line| {
            line.strip_prefix(&prefix)
                .map(|rest| rest.split(';').next().unwrap_or_default())
        })
    }

    /// The `Location` without the scheme and host (the tests follow redirects to the app).
    pub fn location_path(&self) -> String {
        let location = self.header("location").expect("no location");
        match location.find("://") {
            Some(i) => {
                let rest = &location[i + 3..];
                rest.find('/')
                    .map_or("/".to_owned(), |j| rest[j..].to_owned())
            }
            None => location.to_owned(),
        }
    }
}

/// Sends one request on a new connection. `Host` is added unless given.
pub async fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> Response {
    send_body(addr, method, path, headers, "").await
}

/// Like [`send`], with a body.
pub async fn send_body(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Response {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(connection);
    let mut builder = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).unwrap())
        .uri(path);
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        builder = builder.header("host", addr.to_string());
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = sender
        .send_request(
            builder
                .body(Full::new(Bytes::from(body.to_owned())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    Response {
        status,
        headers,
        body,
    }
}

pub async fn get(addr: SocketAddr, path: &str) -> Response {
    send(addr, "GET", path, &[]).await
}
