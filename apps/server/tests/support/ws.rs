//! WebSocket client for the tests (tokio-tungstenite, the same line axum uses).

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect(addr: SocketAddr, origin: &str) -> Result<Client, Error> {
    let mut request = format!("ws://{addr}/ws").into_client_request()?;
    request
        .headers_mut()
        .insert("origin", origin.parse().unwrap());
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(client, _)| client)
}

/// Like [`connect`], with extra request headers (`X-Forwarded-For`, …).
pub async fn connect_with(
    addr: SocketAddr,
    origin: &str,
    headers: &[(&str, &str)],
) -> Result<Client, Error> {
    let mut request = format!("ws://{addr}/ws").into_client_request()?;
    let map = request.headers_mut();
    map.insert("origin", origin.parse().unwrap());
    for (name, value) in headers {
        map.append(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(client, _)| client)
}

pub async fn send_text(client: &mut Client, text: &str) {
    client.send(Message::Text(text.into())).await.unwrap();
}

/// Reads until a data message arrives (skipping pings and pongs).
pub async fn next_data(client: &mut Client, timeout: Duration) -> Option<Message> {
    tokio::time::timeout(timeout, async {
        while let Some(Ok(message)) = client.next().await {
            if matches!(
                message,
                Message::Text(_) | Message::Binary(_) | Message::Close(_)
            ) {
                return Some(message);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// What ended the stream: a close frame (code, reason), or the connection ending without one.
#[derive(Debug, PartialEq, Eq)]
pub enum Ending {
    Close(u16, String),
    NoCloseFrame,
}

/// Reads until the server closes, up to `timeout` (panics if it stays open).
pub async fn ending(client: &mut Client, timeout: Duration) -> Ending {
    tokio::time::timeout(timeout, async {
        loop {
            match client.next().await {
                Some(Ok(Message::Close(Some(frame)))) => {
                    return Ending::Close(u16::from(frame.code), frame.reason.to_string());
                }
                Some(Ok(Message::Close(None))) => return Ending::Close(1005, String::new()),
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return Ending::NoCloseFrame,
            }
        }
    })
    .await
    .expect("the server did not close the socket in time")
}

/// After a close, the TCP connection must end shortly.
pub async fn assert_stream_ends(client: &mut Client, timeout: Duration) {
    let ended = tokio::time::timeout(timeout, async {
        loop {
            match client.next().await {
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return,
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the connection stayed open");
}

/// A raw client that performs the handshake and then never writes again (so it never
/// answers pings or the close). Returns the server frames read until EOF as
/// (opcode, payload); errs if the connection is still open at `timeout`.
pub async fn silent_client_frames(
    addr: SocketAddr,
    origin: &str,
    wait_before_reading: Duration,
    timeout: Duration,
) -> Vec<(u8, Vec<u8>)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET /ws HTTP/1.1\r\nHost: {addr}\r\nOrigin: {origin}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    tokio::time::sleep(wait_before_reading).await;
    let mut bytes = Vec::new();
    tokio::time::timeout(timeout, stream.read_to_end(&mut bytes))
        .await
        .expect("the server did not close the TCP connection")
        .unwrap();
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("no handshake response")
        + 4;
    assert!(
        bytes.starts_with(b"HTTP/1.1 101"),
        "{:?}",
        String::from_utf8_lossy(&bytes[..end])
    );
    let mut frames = Vec::new();
    let mut rest = &bytes[end..];
    while rest.len() >= 2 {
        let opcode = rest[0] & 0x0f;
        let (len, header) = match rest[1] & 0x7f {
            126 => (usize::from(u16::from_be_bytes([rest[2], rest[3]])), 4),
            127 => panic!("unexpected 64-bit frame length"),
            n => (usize::from(n), 2),
        };
        frames.push((opcode, rest[header..header + len].to_vec()));
        rest = &rest[header + len..];
    }
    frames
}

/// Close code and reason of a close frame payload.
pub fn close_payload(payload: &[u8]) -> (u16, String) {
    (
        u16::from_be_bytes([payload[0], payload[1]]),
        String::from_utf8(payload[2..].to_vec()).unwrap(),
    )
}
