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
use oauth2::{
    AuthorizationCode, ClientId, ClientSecret, PkceCodeVerifier, RedirectUrl, RequestTokenError,
    TokenResponse, TokenUrl,
};
use protocol::{ApiErrorCode, CallbackErrorCode, Provider};
use serde::Deserialize;
use sqlx::Row;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::link::{self, LinkRequest};
use super::login::{self, Identity, LoginError};
use super::reauth::{self, ReauthRequest};
use super::{client, flow_hash, providers};
use crate::app::AppState;
use crate::http::cookies::CookiePolicy;
use crate::http::errors::error_response;
use crate::net::{ClientContext, IpKey, LocalAddr};
use crate::sessions::{self, Issued, TokenHash, UaFamily};

/// The login page with an error code from the closed list: the only place that builds this
/// `Location` (the callback errors and a logout that could not revoke).
pub fn error_location(code: CallbackErrorCode) -> HeaderValue {
    let location = format!("/login?error={}", code.as_str());
    HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/"))
}

/// The login page with no query: where `link` without a session sends the browser (008
/// "Enmienda 2026-10-30" of RF-C2). Built here, beside [`error_location`], so every `Location`
/// to the login page has one home.
pub fn login_location() -> HeaderValue {
    HeaderValue::from_static("/login")
}

/// 303 to `auth.try_again` with no cookie change, for a 429 or a 503 of a route the browser
/// navigates to (008 "Enmienda 2026-10-30" of RF-B5): no `Retry-After`, no body, never `next`.
/// `cache_control` is `no-store`, or `private, no-store` where the session was read.
pub fn try_again_redirect(cache_control: HeaderValue) -> Response {
    code_redirect(CallbackErrorCode::TryAgain, cache_control)
}

/// 303 to the login page with `code` and no cookie change: what a route the browser posts to
/// answers before any flow exists (`auth.try_again`, and 009 RF8 and RF11: `auth.reauth_required`
/// of a link without a confirmation, `auth.reauth_mismatch` of a reauth without that account).
/// Unlike [`error_redirect`], the flow cookie is not touched.
pub fn code_redirect(code: CallbackErrorCode, cache_control: HeaderValue) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, error_location(code)),
            (header::CACHE_CONTROL, cache_control),
        ],
    )
        .into_response()
}

/// The error redirect of the callback: it always expires the flow cookie. The quota layer of the
/// callback (`rate::http::redirect_layer`) answers with this very function.
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
        Ok((issued, location)) => success(&cookies, &issued, location),
        Err(code) => error_redirect(&cookies, code),
    }
}

struct Context {
    ip_hash: [u8; 32],
    ua: UaFamily,
    previous: Option<TokenHash>,
    local: Option<std::net::SocketAddr>,
}

/// `next` as a `Location`, built before the login commits so the response cannot fail after
/// it. `sanitize` already refuses what a header cannot carry; a flow stored before that rule
/// falls back to `/`.
fn location_of(next: &str) -> HeaderValue {
    HeaderValue::from_str(next).unwrap_or(HeaderValue::from_static("/"))
}

fn success(cookies: &CookiePolicy, issued: &Issued, location: HeaderValue) -> Response {
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
    link_user_id: Option<Uuid>,
}

/// What the flow is for. Exhaustive: a `login` flow never links, a `link` flow never logs in, a
/// `reauth` flow never creates anything (009 RF12), and any other text or a user where it does
/// not belong (the `CHECK`s forbid both) is `auth.invalid_state`.
enum FlowPurpose {
    Login,
    Link(Uuid),
    Reauth(Uuid),
}

fn purpose_of(flow: &Flow) -> Option<FlowPurpose> {
    match (flow.purpose.as_str(), flow.link_user_id) {
        ("login", None) => Some(FlowPurpose::Login),
        ("link", Some(user)) => Some(FlowPurpose::Link(user)),
        ("reauth", Some(user)) => Some(FlowPurpose::Reauth(user)),
        _ => None,
    }
}

async fn run(
    state: &AppState,
    provider: Provider,
    flow: Option<[u8; 32]>,
    query: Option<CallbackQuery>,
    context: Context,
) -> Result<(Issued, HeaderValue), CallbackErrorCode> {
    use CallbackErrorCode::{InvalidState, ProviderError};
    let flow = flow.ok_or(InvalidState)?;
    let row = sqlx::query(
        "DELETE FROM oauth_flows WHERE id_hash = $1 \
         RETURNING provider, purpose, state, code_verifier, next, expires_at > now(), \
         link_user_id",
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
        link_user_id: row.try_get(6).map_err(|_| InvalidState)?,
    };
    let query = query.ok_or(InvalidState)?;
    let same_state = query
        .state
        .as_deref()
        .is_some_and(|s| bool::from(s.as_bytes().ct_eq(flow.state.as_bytes())));
    let purpose = purpose_of(&flow);
    if !flow.live || flow.provider != provider.as_str() || !same_state {
        return Err(InvalidState);
    }
    let Some(purpose) = purpose else {
        return Err(InvalidState);
    };
    // The provider refused (for example `access_denied`): its text never goes anywhere.
    if query.error.is_some() {
        return Err(ProviderError);
    }
    let code = query.code.ok_or(ProviderError)?;
    let location = location_of(&flow.next);
    let issued = match purpose {
        FlowPurpose::Login => {
            let subject =
                identity(state, provider, code, flow.code_verifier, context.local).await?;
            let identity = Identity { provider, subject };
            login::login(
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
            })?
        }
        FlowPurpose::Link(user) => {
            // T4: the session of the callback is the one of the user who started the link.
            // Only a first check: the guarantee is the rotation inside the transaction, since
            // the session can close while the provider answers (up to 20 s).
            let hash = context.previous.ok_or(InvalidState)?;
            let session = sessions::validate(&state.db, &hash)
                .await
                .map_err(|_| CallbackErrorCode::TryAgain)?
                .filter(|info| info.user_id == user)
                .ok_or(InvalidState)?;
            let subject =
                identity(state, provider, code, flow.code_verifier, context.local).await?;
            let identity = Identity { provider, subject };
            state.auth.link_gate.0.before_link().await;
            link::link(
                state,
                LinkRequest {
                    user,
                    session: session.id,
                    identity: &identity,
                    ip_hash: context.ip_hash,
                    ua: context.ua,
                },
            )
            .await?
        }
        FlowPurpose::Reauth(user) => {
            // As a link: a first check of the session, and the rotation inside the transaction.
            let hash = context.previous.ok_or(InvalidState)?;
            let session = sessions::validate(&state.db, &hash)
                .await
                .map_err(|_| CallbackErrorCode::TryAgain)?
                .filter(|info| info.user_id == user)
                .ok_or(InvalidState)?;
            let subject =
                identity(state, provider, code, flow.code_verifier, context.local).await?;
            let identity = Identity { provider, subject };
            state.auth.link_gate.0.before_link().await;
            reauth::reauth(
                state,
                ReauthRequest {
                    user,
                    session: session.id,
                    identity: &identity,
                    ip_hash: context.ip_hash,
                    ua: context.ua,
                },
            )
            .await?
        }
    };
    Ok((issued, location))
}

/// Why the provider step failed, as the `error_kind` of `auth.provider_failed` (plan-c decision
/// 12): a code of the client, or one of these. Never the provider's text.
fn provider_failed(kind: &'static str) -> CallbackErrorCode {
    tracing::warn!(event = "auth.provider_failed", error_kind = kind);
    CallbackErrorCode::ProviderError
}

/// The code of a failed token request: the transport error of our client, or the provider's
/// answer (an error status or a body that is not a token).
fn token_failure<E>(error: &RequestTokenError<client::ClientError, E>) -> &'static str
where
    E: oauth2::ErrorResponse + 'static,
{
    match error {
        RequestTokenError::Request(error) => error.as_code(),
        _ => "token_status",
    }
}

/// Redeems the code with PKCE (and Basic with the client secret, for Google and Discord) and
/// reads the identity from `userinfo`. The provider's tokens are dropped here and never stored
/// (D1).
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
    let mut oauth = BasicClient::new(ClientId::new(endpoints.client_id))
        .set_token_uri(token_url)
        .set_redirect_uri(redirect);
    if let Some(secret) = &endpoints.client_secret {
        oauth = oauth.set_client_secret(ClientSecret::new(secret.expose().to_owned()));
    }
    let http = &state.auth.http;
    let token = oauth
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(verifier))
        .request_async(&|request| http.send(request))
        .await
        .map_err(|error| provider_failed(token_failure(&error)))?;
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
    let response = http
        .send(userinfo)
        .await
        .map_err(|error| provider_failed(error.as_code()))?;
    if response.status() != StatusCode::OK {
        return Err(provider_failed("userinfo_status"));
    }
    providers::identity(provider, response.body())
        .ok_or_else(|| provider_failed("userinfo_invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(purpose: &str, user: Option<Uuid>) -> Flow {
        Flow {
            provider: "google".to_owned(),
            purpose: purpose.to_owned(),
            state: String::new(),
            code_verifier: String::new(),
            next: "/".to_owned(),
            live: true,
            link_user_id: user,
        }
    }

    /// 009 RF4, RF12: a `reauth` flow always names its user, and only `link` and `reauth` do. The
    /// `CHECK` of the database forbids the rest, so this is where a mistake would show.
    #[test]
    fn every_purpose_needs_exactly_its_user() {
        let user = Uuid::from_u128(7);
        assert!(matches!(
            purpose_of(&flow("login", None)),
            Some(FlowPurpose::Login)
        ));
        assert!(matches!(
            purpose_of(&flow("link", Some(user))),
            Some(FlowPurpose::Link(u)) if u == user
        ));
        assert!(matches!(
            purpose_of(&flow("reauth", Some(user))),
            Some(FlowPurpose::Reauth(u)) if u == user
        ));
        for (purpose, user) in [
            ("reauth", None),
            ("link", None),
            ("login", Some(user)),
            ("other", Some(user)),
            ("other", None),
        ] {
            assert!(
                purpose_of(&flow(purpose, user)).is_none(),
                "{purpose} {user:?}"
            );
        }
    }
}
