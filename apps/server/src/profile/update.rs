//! `PUT /api/me/display-name` and `PUT /api/me/preferences` (008 RF-C4, RF-C5; plan-c
//! decision 8). Every SQL that writes `users` from HTTP lives in this file, and it only touches
//! the name and the preferences (RF-C8).
//!
//! Both: the "other mutations" quota (IP before the session, user after), the session (401 or
//! 503), a JSON body of 4 KB at most whose every rejection is 422 `http.invalid_body`, and the
//! profile as the answer, with `Cache-Control: private, no-store`.

use std::net::{IpAddr, Ipv4Addr};

use axum::Extension;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{
    ApiErrorCode, DisplayNameRequest, ErrorCode, Locale, PreferencesRequest, ProfileErrorCode,
    SelfEvent, Theme,
};

use super::me;
use super::name::{self, NameError};
use crate::app::AppState;
use crate::http::errors::{error_response, invalid_body, too_many_requests};
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::net::{ClientContext, IpKey};
use crate::sessions::SessionInfo;

/// The unique index of the comparison key (D1): it decides a taken name.
const NAME_KEY_INDEX: &str = "users_display_name_key_uq";

fn private(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, PRIVATE_NO_STORE);
    response
}

fn refused(status: StatusCode, code: impl Into<ErrorCode>) -> Response {
    private(error_response(status, code))
}

/// The quotas and the session, in that order.
async fn authorize(
    state: &AppState,
    client: Option<Extension<ClientContext>>,
    headers: &HeaderMap,
) -> Result<SessionInfo, Box<Response>> {
    let key = client.map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.0.key);
    if let Some(secs) = state.rate.mutation_ip.check(&key).retry_after_secs() {
        return Err(Box::new(too_many_requests(ApiErrorCode::RateLimited, secs)));
    }
    let info = match session::current(state, headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return Err(Box::new(session::unauthenticated(state))),
        Err(_) => return Err(Box::new(session::try_again())),
    };
    if let Some(secs) = state
        .rate
        .mutation_user
        .check(&info.user_id)
        .retry_after_secs()
    {
        return Err(Box::new(too_many_requests(ApiErrorCode::RateLimited, secs)));
    }
    Ok(info)
}

fn update_failed(error: &sqlx::Error) -> Response {
    tracing::warn!(
        event = "profile.update_failed",
        error_kind = crate::db::error_kind(error)
    );
    session::try_again()
}

/// The profile after a change.
async fn answer(state: &AppState, info: &SessionInfo) -> Response {
    match me::me(state, info.user_id).await {
        Ok(Some(me)) => private(Json(me).into_response()),
        Ok(None) => session::unauthenticated(state),
        Err(()) => session::try_again(),
    }
}

pub async fn display_name(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<DisplayNameRequest>, JsonRejection>,
) -> Response {
    let info = match authorize(&state, client, &headers).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let request = match body {
        Ok(Json(request)) => request,
        Err(rejection) => return private(invalid_body(&rejection)),
    };
    let name = match name::validate(&request.display_name) {
        Ok(name) => name,
        Err(NameError::Invalid) => {
            return refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                ProfileErrorCode::DisplayNameInvalid,
            );
        }
        Err(NameError::Reserved) => {
            return refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                ProfileErrorCode::DisplayNameReserved,
            );
        }
    };
    // No "exists?" first (D1): the unique index decides, so two renames at once cannot both
    // win. A violation never logs the key (ADR-0012 A9).
    let updated = sqlx::query(
        "UPDATE users SET display_name = $1, display_name_key = $2, \
         display_name_is_generated = false WHERE id = $3",
    )
    .bind(name.as_str())
    .bind(name.key())
    .bind(info.user_id)
    .execute(&state.db)
    .await;
    match updated {
        Ok(_) => {}
        Err(error) if crate::db::is_unique_violation(&error, NAME_KEY_INDEX) => {
            return refused(StatusCode::CONFLICT, ProfileErrorCode::DisplayNameTaken);
        }
        Err(error) => return update_failed(&error),
    }
    state.ws.self_hub().publish(
        info.user_id,
        SelfEvent::ProfileChanged {
            display_name: name.as_str().to_owned(),
        },
    );
    answer(&state, &info).await
}

fn locale_code(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "en",
        Locale::Es => "es",
    }
}

fn theme_code(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
        Theme::System => "system",
    }
}

pub async fn preferences(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<PreferencesRequest>, JsonRejection>,
) -> Response {
    let info = match authorize(&state, client, &headers).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let request = match body {
        Ok(Json(request)) => request,
        Err(rejection) => return private(invalid_body(&rejection)),
    };
    // One statement: a missing field keeps the column, `null` clears it. The `CHECK`s of the
    // table are the second barrier of the enums.
    let updated = sqlx::query(
        "UPDATE users SET locale = CASE WHEN $2 THEN $3 ELSE locale END, \
         theme = CASE WHEN $4 THEN $5 ELSE theme END WHERE id = $1",
    )
    .bind(info.user_id)
    .bind(request.locale.is_some())
    .bind(request.locale.flatten().map(locale_code))
    .bind(request.theme.is_some())
    .bind(request.theme.flatten().map(theme_code))
    .execute(&state.db)
    .await;
    if let Err(error) = updated {
        return update_failed(&error);
    }
    answer(&state, &info).await
}
