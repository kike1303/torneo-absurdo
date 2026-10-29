//! `GET /api/auth/{provider}/callback` (ADR-0008 D2 and its 2026-10-28 amendment, 008 RF-B5).
//!
//! Consumes the flow of the cookie (`DELETE … RETURNING`, one use), compares `state` in constant
//! time, redeems the `code` with PKCE, reads the identity from `userinfo`, drops the provider's
//! tokens and logs in. Success: 303 to the `next` of the flow, with the session cookie. Any
//! failure: 303 to `/login` with an `error` query that is a `CallbackErrorCode` and nothing
//! else; no session, never `next`. Both always expire the flow cookie and say `no-store`.

use axum::Extension;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use oauth2::basic::BasicClient;
use oauth2::{AuthorizationCode, ClientId, PkceCodeVerifier, RedirectUrl, TokenResponse, TokenUrl};
use protocol::{ApiErrorCode, CallbackErrorCode, Provider};
use serde::Deserialize;
use sqlx::Row;
use subtle::ConstantTimeEq;

use super::login::{self, Identity, LoginError};
use super::{client, flow_hash};
use crate::app::AppState;
use crate::http::cookies::CookiePolicy;
use crate::http::errors::error_response;
use crate::net::{ClientContext, IpKey, LocalAddr};
use crate::sessions::{Issued, TokenHash, UaFamily};

/// The longest `sub` the server accepts from a provider.
const MAX_SUBJECT_LEN: usize = 255;

/// The login page with an error code from the closed list: the only place that builds this
/// `Location` (the callback errors and a logout that could not revoke).
pub fn error_location(code: CallbackErrorCode) -> HeaderValue {
    let location = format!("/login?error={}", code.as_str());
    HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/"))
}

/// The error redirect of the callback.
pub fn error_redirect(cookies: &CookiePolicy, code: CallbackErrorCode) -> Response {
    let location = error_location(code);
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::SET_COOKIE, cookies.expire_flow()),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    query: Result<Query<CallbackQuery>, QueryRejection>,
    client: Option<Extension<ClientContext>>,
    local: Option<Extension<LocalAddr>>,
    headers: HeaderMap,
) -> Response {
    let Some(provider) = state.auth.provider(&segment) else {
        return error_response(StatusCode::NOT_FOUND, ApiErrorCode::NotFound);
    };
    let cookies = state.auth.cookies;
    let key = client.map_or_else(
        || IpKey::from(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
        |c| c.0.key,
    );
    let context = Context {
        ip_hash: key.hash(&state.auth.ip_hash_key),
        ua: UaFamily::of(
            headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok()),
        ),
        previous: cookies.read_session(&headers).map(TokenHash::of),
        local: local.map(|l| l.0.0),
    };
    let flow = cookies.read_flow(&headers).map(flow_hash);
    match run(&state, provider, flow, query.ok().map(|q| q.0), context).await {
        Ok((issued, next)) => success(&cookies, &issued, &next),
        Err(code) => error_redirect(&cookies, code),
    }
}

struct Context {
    ip_hash: [u8; 32],
    ua: UaFamily,
    previous: Option<TokenHash>,
    local: Option<std::net::SocketAddr>,
}

fn success(cookies: &CookiePolicy, issued: &Issued, next: &str) -> Response {
    let Ok(location) = HeaderValue::from_str(next) else {
        return error_redirect(cookies, CallbackErrorCode::TryAgain);
    };
    let mut response = (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response();
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, cookies.expire_flow());
    headers.append(
        header::SET_COOKIE,
        cookies.set_session(issued.token.as_str(), issued.max_age_secs),
    );
    response
}

struct Flow {
    provider: String,
    purpose: String,
    state: String,
    code_verifier: String,
    next: String,
    live: bool,
}

async fn run(
    state: &AppState,
    provider: Provider,
    flow: Option<[u8; 32]>,
    query: Option<CallbackQuery>,
    context: Context,
) -> Result<(Issued, String), CallbackErrorCode> {
    use CallbackErrorCode::{InvalidState, ProviderError};
    let flow = flow.ok_or(InvalidState)?;
    let row = sqlx::query(
        "DELETE FROM oauth_flows WHERE id_hash = $1 \
         RETURNING provider, purpose, state, code_verifier, next, expires_at > now()",
    )
    .bind(flow.as_slice())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| CallbackErrorCode::TryAgain)?
    .ok_or(InvalidState)?;
    let get = |i| row.try_get::<String, _>(i).map_err(|_| InvalidState);
    let flow = Flow {
        provider: get(0)?,
        purpose: get(1)?,
        state: get(2)?,
        code_verifier: get(3)?,
        next: get(4)?,
        live: row.try_get(5).map_err(|_| InvalidState)?,
    };
    let query = query.ok_or(InvalidState)?;
    let same_state = query
        .state
        .as_deref()
        .is_some_and(|s| bool::from(s.as_bytes().ct_eq(flow.state.as_bytes())));
    // Linking is phase C: a `link` flow never logs in.
    if !flow.live || flow.provider != provider.as_str() || flow.purpose != "login" || !same_state {
        return Err(InvalidState);
    }
    // The provider refused (for example `access_denied`): its text never goes anywhere.
    if query.error.is_some() {
        return Err(ProviderError);
    }
    let code = query.code.ok_or(ProviderError)?;
    let subject = identity(state, provider, code, flow.code_verifier, context.local).await?;
    let identity = Identity { provider, subject };
    let issued = login::login(
        &state.auth,
        &state.db,
        &identity,
        context.ip_hash,
        context.ua,
        context.previous,
    )
    .await
    .map_err(|error| match error {
        LoginError::Banned => CallbackErrorCode::Banned,
        LoginError::TryAgain => CallbackErrorCode::TryAgain,
    })?;
    Ok((issued, flow.next))
}

#[derive(Deserialize)]
struct UserInfo {
    sub: String,
}

/// Redeems the code with PKCE and reads `sub` from `userinfo`. The provider's tokens are
/// dropped here and never stored (D1).
async fn identity(
    state: &AppState,
    provider: Provider,
    code: String,
    verifier: String,
    local: Option<std::net::SocketAddr>,
) -> Result<String, CallbackErrorCode> {
    use CallbackErrorCode::ProviderError;
    let endpoints = state.auth.endpoints(provider, local).ok_or(ProviderError)?;
    let token_url =
        TokenUrl::new(endpoints.token.ok_or(ProviderError)?).map_err(|_| ProviderError)?;
    let redirect = RedirectUrl::new(endpoints.redirect).map_err(|_| ProviderError)?;
    let oauth = BasicClient::new(ClientId::new(endpoints.client_id))
        .set_token_uri(token_url)
        .set_redirect_uri(redirect);
    let token = oauth
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(verifier))
        .request_async(&client::send)
        .await
        .map_err(|_| ProviderError)?;
    let userinfo = axum::http::Request::builder()
        .method("GET")
        .uri(endpoints.userinfo.ok_or(ProviderError)?)
        .header(header::ACCEPT, "application/json")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", token.access_token().secret()),
        )
        .body(Vec::new())
        .map_err(|_| ProviderError)?;
    drop(token);
    let response = client::send(userinfo).await.map_err(|_| ProviderError)?;
    if response.status() != StatusCode::OK {
        return Err(ProviderError);
    }
    let info: UserInfo = serde_json::from_slice(response.body()).map_err(|_| ProviderError)?;
    if info.sub.is_empty()
        || info.sub.len() > MAX_SUBJECT_LEN
        || info.sub.chars().any(char::is_control)
    {
        return Err(ProviderError);
    }
    Ok(info.sub)
}
