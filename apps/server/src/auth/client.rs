//! The HTTP client of the OAuth flow (008 plan-b decision 1, plan-c decision 4): `oauth2`'s
//! `AsyncHttpClient` over hyper's `http1` connection, with `tokio-rustls` for Google and Discord.
//!
//! - `http` only to a loopback IP (the dev provider of the same process and the fake providers
//!   of the tests).
//! - `https` only to a DNS name, with `webpki` validation, SNI, the `ring` provider, the
//!   `webpki-roots` roots and ALPN `http/1.1`. An IP literal is refused: real hosts are names,
//!   and SNI needs one.
//!
//! No personal data ever leaves in a request: the only headers it adds are `Host` and a generic
//! `User-Agent`. It never follows redirects, gives up after 10 s in total (DNS, connections,
//! TLS and the response; each connection attempt has 3 s) and reads at most 64 KB. Errors are
//! codes ([`ClientError::as_code`]): never the text of rustls or of the certificate.

use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{self, HeaderValue, Uri, header};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
pub use oauth2::{HttpRequest, HttpResponse};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// The only `User-Agent` the server sends: generic, with no version and no host.
pub const USER_AGENT: &str = "torneo-absurdo-server";
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// Bound of one connection attempt (TCP and the TLS handshake) to one address of a name, so an
/// address without a route does not use up [`TIMEOUT`].
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("the URL is neither http to a loopback IP nor https to a name")]
    Refused,
    #[error("the name did not resolve")]
    Dns,
    #[error("the connection failed")]
    Io,
    #[error("the TLS handshake or the certificate failed")]
    Tls,
    #[error("the request timed out")]
    Timeout,
    #[error("the response body is too large")]
    TooLarge,
    #[error("the request or the response is not valid HTTP")]
    Http,
}

impl ClientError {
    /// The code of the `error_kind` of `auth.provider_failed` (plan-c decision 12).
    pub const fn as_code(&self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::Dns => "dns",
            Self::Io => "io",
            Self::Tls => "tls",
            Self::Timeout => "timeout",
            Self::TooLarge => "too_large",
            Self::Http => "http",
        }
    }
}

/// What a name resolves to. A plain function, so the tests resolve without the network.
pub type ResolveFuture = Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send>>;
pub type Resolve = fn(String, u16) -> ResolveFuture;

/// The system resolver (in the image, musl's, over the `/etc/resolv.conf` Docker mounts).
pub fn system_resolve(host: String, port: u16) -> ResolveFuture {
    Box::pin(async move {
        Ok(tokio::net::lookup_host((host.as_str(), port))
            .await?
            .collect())
    })
}

/// The roots of the production config: `webpki-roots` (the distroless image has no CA bundle).
pub fn production_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    roots
}

/// The TLS config of production: built once at startup, only with a real provider.
pub fn production_config() -> Result<Arc<ClientConfig>, rustls::Error> {
    config_with_roots(production_roots())
}

/// `ring`, TLS 1.2 and 1.3, `roots`, ALPN `http/1.1` and no client certificate.
pub(crate) fn config_with_roots(roots: RootCertStore) -> Result<Arc<ClientConfig>, rustls::Error> {
    let mut config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// The client of the OAuth flow: the TLS config (if a real provider is on) and the resolver.
#[derive(Clone)]
pub struct HttpClient {
    tls: Option<Arc<ClientConfig>>,
    resolve: Resolve,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient")
            .field("tls", &self.tls.is_some())
            .finish_non_exhaustive()
    }
}

impl HttpClient {
    pub fn new(tls: Option<Arc<ClientConfig>>) -> Self {
        Self {
            tls,
            resolve: system_resolve,
        }
    }

    /// Sends `request` with [`TIMEOUT`]. An owned future (the client is an `Arc` and a `fn`), so
    /// `oauth2` can take `|request| client.send(request)` as its async client.
    pub fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, ClientError>> + Send + 'static {
        let client = self.clone();
        async move { send_with_config(request, TIMEOUT, client.tls.as_ref(), client.resolve).await }
    }
}

/// Sends `request` with [`TIMEOUT`], `http` to a loopback IP only (no TLS config).
pub async fn send(request: HttpRequest) -> Result<HttpResponse, ClientError> {
    send_with(request, TIMEOUT).await
}

pub async fn send_with(
    request: HttpRequest,
    timeout: Duration,
) -> Result<HttpResponse, ClientError> {
    send_with_config(request, timeout, None, system_resolve).await
}

/// Sends `request` within `timeout`, with `tls` for `https` (without it, `https` is refused).
pub(crate) async fn send_with_config(
    request: HttpRequest,
    timeout: Duration,
    tls: Option<&Arc<ClientConfig>>,
    resolve: Resolve,
) -> Result<HttpResponse, ClientError> {
    tokio::time::timeout(timeout, exchange(request, tls, resolve))
        .await
        .map_err(|_| ClientError::Timeout)?
}

enum Target {
    /// `http://<loopback IP>[:port]`.
    Plain(SocketAddr),
    /// `https://<DNS name>[:port]`.
    Tls(String, u16),
}

fn target(uri: &Uri) -> Result<Target, ClientError> {
    let host = uri.host().ok_or(ClientError::Refused)?;
    let literal = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok();
    match (uri.scheme_str(), literal) {
        (Some("http"), Some(ip)) if ip.is_loopback() => Ok(Target::Plain(SocketAddr::new(
            ip,
            uri.port_u16().unwrap_or(80),
        ))),
        (Some("https"), None) if !host.is_empty() => {
            // A name rustls can put in SNI and check the certificate against.
            match ServerName::try_from(host) {
                Ok(ServerName::DnsName(_)) => {
                    Ok(Target::Tls(host.to_owned(), uri.port_u16().unwrap_or(443)))
                }
                _ => Err(ClientError::Refused),
            }
        }
        _ => Err(ClientError::Refused),
    }
}

/// Tries each address in order, [`ATTEMPT_TIMEOUT`] each (TCP and the handshake), until one
/// completes the TLS handshake for `host`.
pub(crate) async fn connect_tls(
    addrs: &[SocketAddr],
    host: &str,
    config: &Arc<ClientConfig>,
) -> Result<TlsStream<TcpStream>, ClientError> {
    let name = ServerName::try_from(host.to_owned()).map_err(|_| ClientError::Refused)?;
    let connector = TlsConnector::from(config.clone());
    let mut last = ClientError::Dns;
    for addr in addrs {
        let attempt = async {
            let stream = TcpStream::connect(addr)
                .await
                .map_err(|_| ClientError::Io)?;
            connector
                .connect(name.clone(), stream)
                .await
                .map_err(|_| ClientError::Tls)
        };
        match tokio::time::timeout(ATTEMPT_TIMEOUT, attempt).await {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(error)) => last = error,
            Err(_) => last = ClientError::Timeout,
        }
    }
    Err(last)
}

async fn exchange(
    request: HttpRequest,
    tls: Option<&Arc<ClientConfig>>,
    resolve: Resolve,
) -> Result<HttpResponse, ClientError> {
    let (mut parts, body) = request.into_parts();
    let target = target(&parts.uri)?;
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

    match target {
        Target::Plain(addr) => {
            let stream = TcpStream::connect(addr)
                .await
                .map_err(|_| ClientError::Io)?;
            round_trip(stream, request).await
        }
        Target::Tls(host, port) => {
            let config = tls.ok_or(ClientError::Refused)?;
            let addrs = resolve(host.clone(), port)
                .await
                .map_err(|_| ClientError::Dns)?;
            if addrs.is_empty() {
                return Err(ClientError::Dns);
            }
            let stream = connect_tls(&addrs, &host, config).await?;
            round_trip(stream, request).await
        }
    }
}

/// One request and its response (at most [`MAX_BODY_BYTES`]) on a fresh connection.
async fn round_trip<S>(
    stream: S,
    request: http::Request<Full<Bytes>>,
) -> Result<HttpResponse, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
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

    // --- HTTPS (008 RF-C1, plan-c decision 4) ------------------------------------------------

    use std::net::SocketAddr;
    use std::sync::Arc;

    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::{RootCertStore, ServerConfig};

    /// Every name resolves to the loopback, on the port of the URL: the TLS tests never use
    /// the system resolver or the network.
    fn to_loopback(_host: String, port: u16) -> ResolveFuture {
        Box::pin(async move { Ok(vec![SocketAddr::from(([127, 0, 0, 1], port))]) })
    }

    fn unresolvable(_host: String, _port: u16) -> ResolveFuture {
        Box::pin(async { Err(std::io::Error::other("no such name")) })
    }

    fn no_address(_host: String, _port: u16) -> ResolveFuture {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// A test CA and a certificate for `names` signed by it.
    struct TestPki {
        ca: CertificateDer<'static>,
        chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    }

    fn pki(names: &[&str]) -> TestPki {
        let ca_key = rcgen::KeyPair::generate().unwrap();
        let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca = rcgen::CertifiedIssuer::self_signed(ca_params, ca_key).unwrap();
        let leaf_key = rcgen::KeyPair::generate().unwrap();
        let names: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
        let leaf = rcgen::CertificateParams::new(names)
            .unwrap()
            .signed_by(&leaf_key, &ca)
            .unwrap();
        TestPki {
            ca: ca.der().clone(),
            chain: vec![leaf.der().clone()],
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        }
    }

    fn trusting(ca: &CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
        let mut roots = RootCertStore::empty();
        roots.add(ca.clone()).unwrap();
        config_with_roots(roots).unwrap()
    }

    /// A one-shot TLS server (ring, http/1.1) that answers `200 {}` and returns the request.
    async fn tls_server(pki: &TestPki) -> (u16, tokio::task::JoinHandle<Option<Vec<u8>>>) {
        let mut config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(pki.chain.clone(), pki.key.clone_key())
                .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(stream).await.ok()?;
            let mut buffer = vec![0; 8192];
            let n = tls.read(&mut buffer).await.ok()?;
            buffer.truncate(n);
            tls.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}")
                .await
                .ok()?;
            tls.flush().await.ok()?;
            Some(buffer)
        });
        (port, task)
    }

    async fn send_tls(
        url: &str,
        config: &Arc<rustls::ClientConfig>,
        resolve: Resolve,
    ) -> Result<HttpResponse, ClientError> {
        send_with_config(get(url), TIMEOUT, Some(config), resolve).await
    }

    #[tokio::test]
    async fn https_to_a_trusted_certificate_works_with_sni_and_the_same_headers() {
        let pki = pki(&["provider.test"]);
        let (port, task) = tls_server(&pki).await;
        let url = format!("https://provider.test:{port}/userinfo?x=1");
        let response = send_tls(&url, &trusting(&pki.ca), to_loopback)
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), b"{}");
        let request = String::from_utf8(task.await.unwrap().unwrap())
            .unwrap()
            .to_ascii_lowercase();
        assert!(
            request.starts_with("get /userinfo?x=1 http/1.1\r\n"),
            "{request}"
        );
        assert!(
            request.contains(&format!("\r\nhost: provider.test:{port}\r\n")),
            "{request}"
        );
        assert!(
            request.contains("\r\nuser-agent: torneo-absurdo-server\r\n"),
            "{request}"
        );
    }

    #[tokio::test]
    async fn https_fails_with_tls_on_an_untrusted_or_mismatched_certificate() {
        let pki = pki(&["provider.test"]);
        // The production roots never trust the test CA.
        let (port, _task) = tls_server(&pki).await;
        let url = format!("https://provider.test:{port}/x");
        let production = production_config().unwrap();
        assert!(matches!(
            send_tls(&url, &production, to_loopback).await,
            Err(ClientError::Tls)
        ));
        // Trusted CA, but the name is not in the certificate.
        let (port, _task) = tls_server(&pki).await;
        let url = format!("https://other.test:{port}/x");
        assert!(matches!(
            send_tls(&url, &trusting(&pki.ca), to_loopback).await,
            Err(ClientError::Tls)
        ));
    }

    #[tokio::test]
    async fn a_server_that_does_not_speak_tls_is_a_tls_error() {
        let pki = pki(&["provider.test"]);
        let (base, _task) = server(
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}".to_vec(),
            false,
        )
        .await;
        let port: u16 = base.rsplit(':').next().unwrap().parse().unwrap();
        let url = format!("https://provider.test:{port}/x");
        assert!(matches!(
            send_tls(&url, &trusting(&pki.ca), to_loopback).await,
            Err(ClientError::Tls)
        ));
    }

    #[tokio::test]
    async fn the_timeout_covers_a_handshake_that_never_ends() {
        let pki = pki(&["provider.test"]);
        let (base, _task) = server(Vec::new(), true).await;
        let port: u16 = base.rsplit(':').next().unwrap().parse().unwrap();
        let url = format!("https://provider.test:{port}/x");
        let result = send_with_config(
            get(&url),
            Duration::from_millis(300),
            Some(&trusting(&pki.ca)),
            to_loopback,
        )
        .await;
        assert!(matches!(result, Err(ClientError::Timeout)));
    }

    /// The first address accepts the connection and never answers the handshake: its attempt
    /// gives up at [`ATTEMPT_TIMEOUT`] and the second address is tried, within [`TIMEOUT`].
    #[tokio::test]
    async fn a_dead_first_address_leaves_time_for_the_second() {
        assert_eq!(ATTEMPT_TIMEOUT, Duration::from_secs(3));
        let pki = pki(&["provider.test"]);
        let silent = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let silent_addr = silent.local_addr().unwrap();
        let (port, task) = tls_server(&pki).await;
        let good = SocketAddr::from(([127, 0, 0, 1], port));
        let started = tokio::time::Instant::now();
        let stream = connect_tls(&[silent_addr, good], "provider.test", &trusting(&pki.ca))
            .await
            .unwrap();
        let elapsed = started.elapsed();
        assert!(elapsed >= ATTEMPT_TIMEOUT, "{elapsed:?}");
        assert!(elapsed < TIMEOUT, "{elapsed:?}");
        drop(stream);
        drop(silent);
        let _ = task.await;
    }

    #[tokio::test]
    async fn https_is_only_for_dns_names() {
        let pki = pki(&["provider.test"]);
        let config = trusting(&pki.ca);
        for url in [
            "https://127.0.0.1/x",
            "https://[::1]/x",
            "https://192.0.2.1/x",
        ] {
            assert!(
                matches!(
                    send_tls(url, &config, to_loopback).await,
                    Err(ClientError::Refused)
                ),
                "{url}"
            );
        }
        // Without a TLS config (no real provider), https is refused too.
        assert!(matches!(
            send_with_config(get("https://provider.test/x"), TIMEOUT, None, to_loopback).await,
            Err(ClientError::Refused)
        ));
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_is_a_dns_error() {
        let pki = pki(&["provider.test"]);
        let config = trusting(&pki.ca);
        for resolve in [unresolvable as Resolve, no_address] {
            assert!(matches!(
                send_tls("https://provider.test/x", &config, resolve).await,
                Err(ClientError::Dns)
            ));
        }
    }

    #[test]
    fn the_production_config_uses_ring_the_webpki_roots_and_only_http_1_1() {
        let config = production_config().unwrap();
        assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
        let ring = rustls::crypto::ring::default_provider();
        let suites = |p: &rustls::crypto::CryptoProvider| -> Vec<_> {
            p.cipher_suites.iter().map(|s| s.suite()).collect()
        };
        assert_eq!(suites(config.crypto_provider()), suites(&ring));
        assert!(!config.crypto_provider().cipher_suites.is_empty());
        assert!(
            production_roots().len() > 100,
            "{}",
            production_roots().len()
        );
        // No client certificate is ever offered.
        assert!(!config.client_auth_cert_resolver.has_certs());
    }

    #[test]
    fn every_error_has_a_stable_code() {
        let codes: Vec<_> = [
            ClientError::Refused,
            ClientError::Dns,
            ClientError::Io,
            ClientError::Tls,
            ClientError::Timeout,
            ClientError::TooLarge,
            ClientError::Http,
        ]
        .iter()
        .map(ClientError::as_code)
        .collect();
        assert_eq!(
            codes,
            [
                "refused",
                "dns",
                "io",
                "tls",
                "timeout",
                "too_large",
                "http"
            ]
        );
    }
}
