//! The dev OAuth provider (008 RF-B6). Only with the `dev-auth` feature, whose config gate only
//! lets the server start on a loopback `http` origin and a loopback `BIND_ADDR`.
//!
//! It serves `authorize`, `token` and `userinfo` in the same process, and the OAuth client
//! calls them over HTTP exactly as it will call Google and Discord, PKCE `S256` included. The
//! identity is the `dev_identity` of `start`, carried in `login_hint`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::Json;
use axum::extract::rejection::{FormRejection, QueryRejection};
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::AuthErrorCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::time::Instant;

use super::{Endpoints, random_text};
use crate::app::AppState;
use crate::http::errors::error_response;

pub const CLIENT_ID: &str = "torneo-dev";
/// The fixed prefix of every dev access token: a canary of the log test (RF-B17).
pub const ACCESS_TOKEN_CANARY: &str = "ta-dev-access-token-canary.";
const CODE_TTL: Duration = Duration::from_secs(60);
/// Codes and tokens kept in memory at most (each map).
const MAX_ENTRIES: usize = 1000;

pub fn endpoints(public_origin: &str, local: Option<SocketAddr>) -> Endpoints {
    let local = local.map(|addr| format!("http://{addr}"));
    Endpoints {
        client_id: CLIENT_ID.to_owned(),
        authorize: format!("{public_origin}/api/auth/dev/oauth/authorize"),
        token: local
            .as_ref()
            .map(|l| format!("{l}/api/auth/dev/oauth/token")),
        userinfo: local.map(|l| format!("{l}/api/auth/dev/oauth/userinfo")),
        redirect: redirect_uri(public_origin),
    }
}

fn redirect_uri(public_origin: &str) -> String {
    format!("{public_origin}/api/auth/dev/callback")
}

/// `^[a-z0-9_-]{1,32}$` (RF-B6).
pub fn is_identity(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn is_base64url(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

struct Code {
    identity: String,
    challenge: String,
    expires: Instant,
}

struct Token {
    identity: String,
    expires: Instant,
}

/// The in-memory state of the dev provider: codes and access tokens, each single use.
#[derive(Default)]
pub struct DevProvider {
    codes: Mutex<HashMap<String, Code>>,
    tokens: Mutex<HashMap<String, Token>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Inserts unless the map is full of live entries.
fn insert<T>(
    map: &mut HashMap<String, T>,
    key: String,
    value: T,
    expires: impl Fn(&T) -> Instant,
) -> bool {
    if map.len() >= MAX_ENTRIES {
        let now = Instant::now();
        map.retain(|_, v| expires(v) > now);
        if map.len() >= MAX_ENTRIES {
            return false;
        }
    }
    map.insert(key, value);
    true
}

fn refused() -> Response {
    error_response(StatusCode::BAD_REQUEST, AuthErrorCode::ProviderError)
}

/// An OAuth error body (RFC 6749 §5.2): a code, never text.
fn oauth_error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

#[derive(Deserialize)]
pub struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    login_hint: Option<String>,
}

/// `GET /api/auth/dev/oauth/authorize`: validates the request and redirects to the callback
/// with a new single-use code.
pub async fn authorize(
    State(state): State<AppState>,
    query: Result<Query<AuthorizeQuery>, QueryRejection>,
) -> Response {
    let Ok(Query(q)) = query else {
        return refused();
    };
    let expected_redirect = redirect_uri(&state.auth.public_origin);
    let valid = q.response_type.as_deref() == Some("code")
        && q.client_id.as_deref() == Some(CLIENT_ID)
        && q.redirect_uri.as_deref() == Some(expected_redirect.as_str())
        && q.code_challenge_method.as_deref() == Some("S256")
        && q.code_challenge
            .as_deref()
            .is_some_and(|c| c.len() == 43 && is_base64url(c, 43))
        && q.state.as_deref().is_some_and(|s| is_base64url(s, 64))
        && q.login_hint.as_deref().is_some_and(is_identity);
    let (Some(state_text), Some(challenge), Some(identity), true) =
        (q.state, q.code_challenge, q.login_hint, valid)
    else {
        return refused();
    };
    let Some(code) = random_text(16) else {
        return refused();
    };
    let stored = insert(
        &mut lock(&state.auth.dev.codes),
        code.clone(),
        Code {
            identity,
            challenge,
            expires: Instant::now() + CODE_TTL,
        },
        |c| c.expires,
    );
    if !stored {
        return refused();
    }
    let Ok(mut location) = url::Url::parse(&expected_redirect) else {
        return refused();
    };
    location
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &state_text);
    let Ok(value) = HeaderValue::from_str(location.as_str()) else {
        return refused();
    };
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, value),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct TokenForm {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
}

/// `POST /api/auth/dev/oauth/token`: consumes the code once and checks `S256(code_verifier)`.
pub async fn token(
    State(state): State<AppState>,
    form: Result<Form<TokenForm>, FormRejection>,
) -> Response {
    let invalid = || oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
    let Ok(Form(f)) = form else {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    if f.grant_type.as_deref() != Some("authorization_code") {
        return oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type");
    }
    let (Some(code), Some(verifier)) = (f.code, f.code_verifier) else {
        return invalid();
    };
    if f.redirect_uri
        .is_some_and(|r| r != redirect_uri(&state.auth.public_origin))
    {
        return invalid();
    }
    // Single use: removed before any check, so a failed attempt burns the code too.
    let Some(entry) = lock(&state.auth.dev.codes).remove(&code) else {
        return invalid();
    };
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let matches = bool::from(computed.as_bytes().ct_eq(entry.challenge.as_bytes()));
    if entry.expires <= Instant::now() || !matches {
        return invalid();
    }
    let Some(random) = random_text(16) else {
        return invalid();
    };
    let access = format!("{ACCESS_TOKEN_CANARY}{random}");
    let stored = insert(
        &mut lock(&state.auth.dev.tokens),
        access.clone(),
        Token {
            identity: entry.identity,
            expires: Instant::now() + CODE_TTL,
        },
        |t| t.expires,
    );
    if !stored {
        return invalid();
    }
    (
        [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(serde_json::json!({
            "access_token": access,
            "token_type": "bearer",
            "expires_in": CODE_TTL.as_secs(),
        })),
    )
        .into_response()
}

/// `GET /api/auth/dev/oauth/userinfo`: `{"sub": <dev_identity>}` for a bearer token (single
/// use: the client reads the identity once and drops the token).
pub async fn userinfo(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let unauthorized = || oauth_error(StatusCode::UNAUTHORIZED, "invalid_token");
    let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return unauthorized();
    };
    let Some(entry) = lock(&state.auth.dev.tokens).remove(token) else {
        return unauthorized();
    };
    if entry.expires <= Instant::now() {
        return unauthorized();
    }
    Json(serde_json::json!({ "sub": entry.identity })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        for ok in ["a", "ana", "ana_bel-2", &"a".repeat(32)] {
            assert!(is_identity(ok), "{ok}");
        }
        for bad in ["", "Ana", "a b", "é", "a;b", &"a".repeat(33)] {
            assert!(!is_identity(bad), "{bad}");
        }
    }

    #[test]
    fn a_full_map_refuses_new_live_entries() {
        let mut map = HashMap::new();
        let later = Instant::now() + Duration::from_secs(60);
        for i in 0..MAX_ENTRIES {
            assert!(insert(&mut map, i.to_string(), later, |e| *e));
        }
        assert!(!insert(&mut map, "x".to_owned(), later, |e| *e));
        // Expired entries make room.
        let mut map: HashMap<String, Instant> = (0..MAX_ENTRIES)
            .map(|i| (i.to_string(), Instant::now()))
            .collect();
        assert!(insert(&mut map, "x".to_owned(), later, |e| *e));
        assert_eq!(map.len(), 1);
    }
}
