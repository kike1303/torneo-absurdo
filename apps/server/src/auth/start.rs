//! `POST /api/auth/{provider}/start` (ADR-0008 D2, 008 RF-B5): a form from the login page. It
//! creates the flow (128-bit id and `state`, PKCE `code_verifier`, all from `OsRng`), sets the
//! flow cookie and answers 303 to the provider's authorization URL.

use axum::extract::rejection::FormRejection;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};
use protocol::{ApiErrorCode, CallbackErrorCode, Provider};
use serde::Deserialize;

use super::callback::error_redirect;
use super::{flow_hash, next, random_text};
use crate::app::AppState;
use crate::http::errors::error_response;

#[derive(Deserialize)]
pub struct StartForm {
    next: Option<String>,
    dev_identity: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    form: Result<Form<StartForm>, FormRejection>,
) -> Response {
    let Some(provider) = state.auth.provider(&segment) else {
        return error_response(StatusCode::NOT_FOUND, ApiErrorCode::NotFound);
    };
    match start(&state, provider, form).await {
        Ok(response) => response,
        Err(code) => error_redirect(&state.auth.cookies, code),
    }
}

async fn start(
    state: &AppState,
    provider: Provider,
    form: Result<Form<StartForm>, FormRejection>,
) -> Result<Response, CallbackErrorCode> {
    let Ok(Form(form)) = form else {
        return Err(CallbackErrorCode::InvalidState);
    };
    // `dev_identity` only for `dev`, and always for it.
    let identity = match (provider, form.dev_identity) {
        (Provider::Dev, Some(identity)) if is_dev_identity(&identity) => Some(identity),
        (Provider::Dev, _) | (_, Some(_)) => return Err(CallbackErrorCode::InvalidState),
        (_, None) => None,
    };
    let endpoints = state
        .auth
        .endpoints(provider, None)
        .ok_or(CallbackErrorCode::ProviderError)?;
    let next = next::sanitize(form.next.as_deref());
    let (Some(flow_id), Some(state_text), Some(verifier)) =
        (random_text(16), random_text(16), random_text(32))
    else {
        return Err(CallbackErrorCode::TryAgain);
    };

    // Opportunistic cleanup of expired flows (D2), then the new one.
    let stored = async {
        sqlx::query("DELETE FROM oauth_flows WHERE expires_at < now()")
            .execute(&state.db)
            .await?;
        sqlx::query(
            "INSERT INTO oauth_flows \
             (id_hash, provider, purpose, state, code_verifier, next, dev_identity, expires_at) \
             VALUES ($1, $2, 'login', $3, $4, $5, $6, now() + $7::bigint * interval '1 second')",
        )
        .bind(flow_hash(&flow_id).as_slice())
        .bind(provider.as_str())
        .bind(&state_text)
        .bind(&verifier)
        .bind(&next)
        .bind(identity.as_deref())
        .bind(i64::try_from(crate::http::cookies::FLOW_MAX_AGE_SECS).unwrap_or(600))
        .execute(&state.db)
        .await
    };
    stored.await.map_err(|_| CallbackErrorCode::TryAgain)?;

    let auth_url =
        AuthUrl::new(endpoints.authorize).map_err(|_| CallbackErrorCode::ProviderError)?;
    let redirect =
        RedirectUrl::new(endpoints.redirect).map_err(|_| CallbackErrorCode::ProviderError)?;
    let client = BasicClient::new(ClientId::new(endpoints.client_id))
        .set_auth_uri(auth_url)
        .set_redirect_uri(redirect);
    let challenge = PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(verifier));
    let mut request = client
        .authorize_url(|| CsrfToken::new(state_text))
        .set_pkce_challenge(challenge);
    // The only scope of the provider (RF-C1); `dev` has none and gets its `login_hint`.
    if let Some(scope) = endpoints.scope {
        request = request.add_scope(Scope::new(scope.to_owned()));
    }
    if let Some(identity) = identity.as_deref() {
        request = request.add_extra_param("login_hint", identity);
    }
    let (url, _) = request.url();
    let location =
        HeaderValue::from_str(url.as_str()).map_err(|_| CallbackErrorCode::ProviderError)?;
    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::SET_COOKIE, state.auth.cookies.set_flow(&flow_id)),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response())
}

#[cfg(feature = "dev-auth")]
fn is_dev_identity(text: &str) -> bool {
    super::dev::is_identity(text)
}

#[cfg(not(feature = "dev-auth"))]
fn is_dev_identity(_text: &str) -> bool {
    false
}
