//! `GET /api/botarga`, `POST /api/botarga` and `PUT /api/botarga/build` (021 "Alta" and "Build").
//! Private: the session's user is the only user any of them reads or writes, and every answer is
//! `private, no-store` with `Vary: Cookie`, like `/api/wallet`.
//!
//! The `GET` never writes (ADR-0008 D4): a user without a botarga gets 404 `botarga.not_created`
//! and creates it with the `POST`. The two writes lock the user if not banned (a ban in between
//! answers 401, as `banned_writes` expects) with a `lock_timeout` of 1 s.

use axum::Extension;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use protocol::{BotargaBuild, BotargaErrorCode, BotargaView};
use uuid::Uuid;

use super::{build, store};
use crate::app::AppState;
use crate::bets::http::{Quota, authorize, ip_key, lock_user, private, refused, unavailable};
use crate::http::session;
use crate::net::ClientContext;

/// What a write ends in. `Gone`: no user, or a banned one.
enum Outcome {
    Done(Box<BotargaView>),
    Refused(BotargaErrorCode),
    Gone,
}

fn failed(event: &'static str, error: &sqlx::Error) -> Response {
    tracing::warn!(event, error_kind = crate::db::error_kind(error));
    unavailable(BotargaErrorCode::Unavailable)
}

fn answer(
    outcome: Result<Outcome, sqlx::Error>,
    state: &AppState,
    event: &'static str,
) -> Response {
    match outcome {
        Ok(Outcome::Done(view)) => private(Json(*view).into_response()),
        Ok(Outcome::Refused(BotargaErrorCode::NotCreated)) => {
            refused(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated)
        }
        Ok(Outcome::Refused(code)) => refused(StatusCode::UNPROCESSABLE_ENTITY, code),
        Ok(Outcome::Gone) => session::unauthenticated(state),
        Err(error) => failed(event, &error),
    }
}

pub async fn get(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.botarga_read_ip,
        user: &state.rate.botarga_read_user,
    };
    let unavailable_code = BotargaErrorCode::Unavailable;
    let info = match authorize(&state, &quota, &key, &headers, unavailable_code).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        store::view(&mut conn, info.user_id).await
    }
    .await;
    match read {
        Ok(Some(view)) => private(Json(view).into_response()),
        Ok(None) => refused(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated),
        Err(error) => failed("botarga.read_failed", &error),
    }
}

/// The quotas of "other mutations" and the session.
async fn authorize_write(
    state: &AppState,
    client: Option<&Extension<ClientContext>>,
    headers: &HeaderMap,
) -> Result<Uuid, Box<Response>> {
    let key = ip_key(client);
    let quota = Quota {
        ip: &state.rate.mutation_ip,
        user: &state.rate.mutation_user,
    };
    let unavailable_code = BotargaErrorCode::Unavailable;
    authorize(state, &quota, &key, headers, unavailable_code)
        .await
        .map(|info| info.user_id)
}

/// The botarga of a user that has none yet (one of before 021, or one an old binary created).
/// Idempotent: `ON CONFLICT DO NOTHING`, and the botarga as it is either way.
pub async fn create(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let user_id = match authorize_write(&state, client.as_ref(), &headers).await {
        Ok(user_id) => user_id,
        Err(response) => return *response,
    };
    let created = async {
        let mut tx = state.db.begin().await?;
        if !lock_user(&mut tx, user_id).await? {
            return Ok(Outcome::Gone);
        }
        store::create(&mut tx, user_id, state.botarga.starter()).await?;
        let Some(view) = store::view(&mut tx, user_id).await? else {
            return Ok(Outcome::Refused(BotargaErrorCode::NotCreated));
        };
        tx.commit().await?;
        Ok(Outcome::Done(Box::new(view)))
    }
    .await;
    answer(created, &state, "botarga.create_failed")
}

pub async fn put_build(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<BotargaBuild>, JsonRejection>,
) -> Response {
    let user_id = match authorize_write(&state, client.as_ref(), &headers).await {
        Ok(user_id) => user_id,
        Err(response) => return *response,
    };
    // Never axum's or serde's text, which copies parts of the body.
    let invalid = || {
        refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            BotargaErrorCode::InvalidRequest,
        )
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    if !build::well_formed(&state.botarga, &request) {
        return invalid();
    }
    let saved = async {
        let mut tx = state.db.begin().await?;
        if !lock_user(&mut tx, user_id).await? {
            return Ok(Outcome::Gone);
        }
        let Some(current) = store::view_locked(&mut tx, user_id).await? else {
            return Ok(Outcome::Refused(BotargaErrorCode::NotCreated));
        };
        let level = current.progress.level;
        if let Err(code) = build::check(&state.botarga, level, &current.unlocks, &request) {
            return Ok(Outcome::Refused(code));
        }
        store::save_build(&mut tx, user_id, &request).await?;
        let Some(view) = store::view(&mut tx, user_id).await? else {
            return Ok(Outcome::Refused(BotargaErrorCode::NotCreated));
        };
        tx.commit().await?;
        Ok(Outcome::Done(Box::new(view)))
    }
    .await;
    answer(saved, &state, "botarga.build_failed")
}
