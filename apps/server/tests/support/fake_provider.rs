//! A fake Google or Discord for the tests (008 T19, plan-c decision 5): an axum server on
//! `127.0.0.1:0` with `token` and `userinfo`, one per test, never the network. The test reads
//! the authorization URL from the `Location` of `start`, asks the fake for a code bound to its
//! `code_challenge` ([`FakeProvider::code_for`]), and calls the callback itself.
//!
//! `token` checks `grant_type`, the exact `redirect_uri`, `Authorization: Basic` with the test
//! credentials and `S256(code_verifier)`, and answers a canary access token. `userinfo` checks
//! that token and answers the canary identity plus `name`, `email`, `username` and
//! `global_name` canaries, which must never reach a type of the server or a log.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;
use server::auth::providers::ProviderUrls;
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;

use super::app::ORIGIN;
use super::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Google,
    Discord,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Discord => "discord",
        }
    }

    pub fn client_id(self) -> &'static str {
        match self {
            Self::Google => config::GOOGLE_CLIENT_ID,
            Self::Discord => config::DISCORD_CLIENT_ID,
        }
    }

    pub fn client_secret(self) -> &'static str {
        match self {
            Self::Google => config::GOOGLE_CLIENT_SECRET,
            Self::Discord => config::DISCORD_CLIENT_SECRET,
        }
    }

    /// The only scope the authorization URL may ask for (RF-C1).
    pub fn scope(self) -> &'static str {
        match self {
            Self::Google => "openid",
            Self::Discord => "identify",
        }
    }

    /// The field of `userinfo` that carries the identity.
    pub fn identity_field(self) -> &'static str {
        match self {
            Self::Google => "sub",
            Self::Discord => "id",
        }
    }

    /// The canary access token of this fake (a log canary).
    pub fn access_token(self) -> String {
        format!("fake-{}-access-token-c4n4ry", self.name())
    }

    /// The header `Authorization: Basic` the token request must carry (a log canary).
    pub fn basic(self) -> String {
        STANDARD.encode(format!("{}:{}", self.client_id(), self.client_secret()))
    }
}

/// What the fake answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ok,
    /// `token` answers 500.
    TokenStatus500,
    /// `userinfo` answers 500.
    UserinfoStatus500,
    /// `userinfo` answers a body that is not JSON.
    InvalidJson,
    /// `userinfo` answers JSON without the identity field.
    NoIdentity,
    /// The identity has a control character.
    ControlIdentity,
    /// `userinfo` answers more than 64 KB.
    TooLarge,
    /// `userinfo` never answers.
    Silent,
}

/// The profile canaries `userinfo` returns besides the identity (T19).
pub const NAME: &str = "Canary Real Name";
pub const EMAIL: &str = "canary.person@example.invalid";
pub const USERNAME: &str = "canary_username";
pub const GLOBAL_NAME: &str = "Canary Global Name";

struct Inner {
    kind: Kind,
    mode: Mutex<Mode>,
    /// code → (challenge, identity).
    codes: Mutex<HashMap<String, (String, String)>>,
    /// The verifier and the `Authorization` of the last token request.
    last_token_request: Mutex<Option<(String, String)>>,
    token_requests: AtomicUsize,
    userinfo_requests: AtomicUsize,
    next: AtomicUsize,
}

pub struct FakeProvider {
    pub addr: SocketAddr,
    inner: Arc<Inner>,
    task: JoinHandle<()>,
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

fn oauth_error(code: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": code })),
    )
        .into_response()
}

async fn token(
    State(inner): State<Arc<Inner>>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    inner.token_requests.fetch_add(1, Ordering::SeqCst);
    let mode = *inner.mode.lock().unwrap();
    if mode == Mode::TokenStatus500 {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let kind = inner.kind;
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let expected_redirect = format!("{ORIGIN}/api/auth/{}/callback", kind.name());
    // The secret goes in the Basic header only, never in the body.
    if authorization != format!("Basic {}", kind.basic())
        || form.client_secret.is_some()
        || form.client_id.is_some()
        || form.grant_type.as_deref() != Some("authorization_code")
        || form.redirect_uri.as_deref() != Some(expected_redirect.as_str())
    {
        return oauth_error("invalid_client");
    }
    let (Some(code), Some(verifier)) = (form.code, form.code_verifier) else {
        return oauth_error("invalid_request");
    };
    *inner.last_token_request.lock().unwrap() = Some((verifier.clone(), authorization));
    let Some((challenge, identity)) = inner.codes.lock().unwrap().remove(&code) else {
        return oauth_error("invalid_grant");
    };
    if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != challenge {
        return oauth_error("invalid_grant");
    }
    // The token carries the identity, so `userinfo` knows whom to answer.
    let access = format!("{}.{identity}", kind.access_token());
    Json(serde_json::json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": kind.scope(),
    }))
    .into_response()
}

async fn userinfo(State(inner): State<Arc<Inner>>, headers: HeaderMap) -> Response {
    inner.userinfo_requests.fetch_add(1, Ordering::SeqCst);
    let kind = inner.kind;
    let prefix = format!("Bearer {}.", kind.access_token());
    let Some(identity) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(&prefix))
        .map(str::to_owned)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let mode = *inner.mode.lock().unwrap();
    let mut body = serde_json::json!({
        "name": NAME,
        "email": EMAIL,
        "username": USERNAME,
        "global_name": GLOBAL_NAME,
    });
    match mode {
        Mode::Ok | Mode::TokenStatus500 => {
            body[kind.identity_field()] = serde_json::Value::from(identity);
        }
        Mode::UserinfoStatus500 => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Mode::InvalidJson => return (StatusCode::OK, "{not json").into_response(),
        Mode::NoIdentity => {}
        Mode::ControlIdentity => {
            body[kind.identity_field()] = serde_json::Value::from(format!("{identity}\u{7}"));
        }
        Mode::TooLarge => {
            body[kind.identity_field()] = serde_json::Value::from(identity);
            body["padding"] = serde_json::Value::from("x".repeat(70 * 1024));
        }
        Mode::Silent => {
            std::future::pending::<()>().await;
        }
    }
    Json(body).into_response()
}

impl FakeProvider {
    pub async fn start(kind: Kind) -> Self {
        let inner = Arc::new(Inner {
            kind,
            mode: Mutex::new(Mode::Ok),
            codes: Mutex::new(HashMap::new()),
            last_token_request: Mutex::new(None),
            token_requests: AtomicUsize::new(0),
            userinfo_requests: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
        });
        let router = Router::new()
            .route("/token", post(token))
            .route("/userinfo", get(userinfo))
            .with_state(inner.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self { addr, inner, task }
    }

    pub fn kind(&self) -> Kind {
        self.inner.kind
    }

    /// The URLs the app is built with (`Settings.oauth`). `authorize` is never called: the test
    /// reads the `Location` of `start`.
    pub fn urls(&self) -> ProviderUrls {
        ProviderUrls {
            authorize: format!("http://{}/authorize", self.addr),
            token: format!("http://{}/token", self.addr),
            userinfo: format!("http://{}/userinfo", self.addr),
        }
    }

    pub fn set_mode(&self, mode: Mode) {
        *self.inner.mode.lock().unwrap() = mode;
    }

    /// A code for `identity`, bound to the `code_challenge` of an authorization URL.
    pub fn code_for(&self, challenge: &str, identity: &str) -> String {
        let n = self.inner.next.fetch_add(1, Ordering::SeqCst);
        let code = format!("fake-code-{n}-{}", uuid::Uuid::new_v4().simple());
        self.inner
            .codes
            .lock()
            .unwrap()
            .insert(code.clone(), (challenge.to_owned(), identity.to_owned()));
        code
    }

    /// The `code_verifier` and the `Authorization` header of the last token request.
    pub fn last_token_request(&self) -> Option<(String, String)> {
        self.inner.last_token_request.lock().unwrap().clone()
    }

    pub fn token_requests(&self) -> usize {
        self.inner.token_requests.load(Ordering::SeqCst)
    }

    pub fn userinfo_requests(&self) -> usize {
        self.inner.userinfo_requests.load(Ordering::SeqCst)
    }
}
