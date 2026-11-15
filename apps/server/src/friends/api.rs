//! The routes of friends (023, phase 1). Every answer is private (`private, no-store`, `Vary:
//! Cookie`) and names friends only by `display_name`. A rejection of an invite is always the same
//! 404, so the routes are no oracle of bans nor of blocks. No log carries a code, a key, a name or
//! a seed: only `event` and `error_kind` (ADR-0012 A8).

use axum::Extension;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Json, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use protocol::{
    BotargaErrorCode, ErrorCode, FriendChallengeRequest, FriendFight, FriendInvitePreview,
    FriendInviteRequest, FriendRemoveRequest, FriendsErrorCode, FriendsView,
};
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{DAILY_CHALLENGES, MAX_FRIENDS, name_key, store, well_formed};
use crate::app::AppState;
use crate::bets::audit::{self, Event};
use crate::bets::http::{Quota, authorize, ip_key, lock_user, private, refused, unavailable};
use crate::botarga::{self, build};
use crate::db::{ACQUIRE_TIMEOUT, error_kind};
use crate::http::session;
use crate::net::ClientContext;

/// What a route ends in. `Gone`: no user, or a banned one (as `banned_writes` expects).
enum Outcome<T> {
    Done(StatusCode, T),
    Refused(StatusCode, ErrorCode),
    Gone,
}

fn refusal<T>(status: StatusCode, code: impl Into<ErrorCode>) -> Result<Outcome<T>, sqlx::Error> {
    Ok(Outcome::Refused(status, code.into()))
}

fn not_found_invite<T>() -> Result<Outcome<T>, sqlx::Error> {
    refusal(StatusCode::NOT_FOUND, FriendsErrorCode::InviteNotFound)
}

fn invalid() -> Response {
    refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        FriendsErrorCode::InvalidRequest,
    )
}

/// The end of a route: its answer, its refusal, or the failure of the database by its kind.
fn answer<T: serde::Serialize>(
    done: Result<Outcome<T>, sqlx::Error>,
    state: &AppState,
    event: &'static str,
) -> Response {
    match done {
        Ok(Outcome::Done(status, body)) => private((status, Json(body)).into_response()),
        Ok(Outcome::Refused(status, code)) => refused(status, code),
        Ok(Outcome::Gone) => session::unauthenticated(state),
        Err(error) => {
            let kind = error_kind(&error);
            tracing::warn!(event, error_kind = kind);
            if kind == "lock_timeout" {
                unavailable(FriendsErrorCode::Busy)
            } else {
                unavailable(FriendsErrorCode::Unavailable)
            }
        }
    }
}

/// A canonical UUID: the text a client sends is the one it reads back.
fn canonical_key(text: &str) -> Option<Uuid> {
    Uuid::parse_str(text)
        .ok()
        .filter(|key| key.to_string() == text)
}

async fn session_of(
    state: &AppState,
    quota: &Quota<'_>,
    client: Option<&Extension<ClientContext>>,
    headers: &HeaderMap,
) -> Result<(Uuid, [u8; 32]), Box<Response>> {
    let key = ip_key(client);
    let info = authorize(state, quota, &key, headers, FriendsErrorCode::Unavailable).await?;
    Ok((info.user_id, key.hash(&state.auth.ip_hash_key)))
}

/// The view of the friends of `user_id`, or `None` without a botarga.
async fn view_of(
    conn: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
) -> Result<Option<FriendsView>, sqlx::Error> {
    if botarga::store::view(&mut *conn, user_id).await?.is_none() {
        return Ok(None);
    }
    Ok(Some(FriendsView {
        friends: store::friends_of(&mut *conn, &state.botarga, user_id).await?,
        invite: store::invite_of(&mut *conn, user_id).await?,
    }))
}

/// `GET /api/friends`. Never writes.
pub async fn get(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let quota = Quota {
        ip: &state.rate.botarga_read_ip,
        user: &state.rate.botarga_read_user,
    };
    let (user_id, _) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        Ok(match view_of(&mut conn, &state, user_id).await? {
            Some(view) => Outcome::Done(StatusCode::OK, view),
            None => Outcome::Refused(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated.into()),
        })
    }
    .await;
    answer(read, &state, "friends.read_failed")
}

/// `POST /api/friends/invite/rotate`: a new code; the old one stops working.
pub async fn rotate(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let quota = Quota {
        ip: &state.rate.mutation_ip,
        user: &state.rate.mutation_user,
    };
    let (user_id, _) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let done = async {
        let mut tx = state.db.begin().await?;
        if !lock_user(&mut tx, user_id).await? {
            return Ok(Outcome::Gone);
        }
        if botarga::store::view_locked(&mut tx, user_id)
            .await?
            .is_none()
        {
            return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
        }
        store::rotate_invite(&mut tx, user_id).await?;
        tx.commit().await?;
        // Read after the commit, so the locks are not held while the list is built.
        let mut conn = state.db.acquire().await?;
        let view = view_of(&mut conn, &state, user_id).await?;
        Ok(view.map_or(Outcome::Gone, |view| Outcome::Done(StatusCode::OK, view)))
    }
    .await;
    answer(done, &state, "friends.rotate_failed")
}

fn accept_quota(state: &AppState) -> Quota<'_> {
    Quota {
        ip: &state.rate.friends_accept_ip,
        user: &state.rate.friends_accept_user,
    }
}

/// `POST /api/friends/invite/preview`: whose invite this is, before accepting it. The same 404 as
/// `accept` for every invite that cannot be accepted.
pub async fn preview(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<FriendInviteRequest>, JsonRejection>,
) -> Response {
    let quota = accept_quota(&state);
    let (user_id, _) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    if !well_formed(&request.code) {
        return refused(StatusCode::NOT_FOUND, FriendsErrorCode::InviteNotFound);
    }
    let read = async {
        let mut conn = state.db.acquire().await?;
        // The same order as `accept`: without a botarga, no invite is read.
        if botarga::store::view(&mut conn, user_id).await?.is_none() {
            return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
        }
        match store::invite_owner(&mut conn, user_id, &request.code).await? {
            Some((_, display_name)) => Ok(Outcome::Done(
                StatusCode::OK,
                FriendInvitePreview { display_name },
            )),
            None => not_found_invite(),
        }
    }
    .await;
    answer(read, &state, "friends.preview_failed")
}

/// `POST /api/friends/accept`: makes the reader a friend of the owner of the code. Locks both
/// users in the order of their ids and counts the cap of both under that lock.
pub async fn accept(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<FriendInviteRequest>, JsonRejection>,
) -> Response {
    let quota = accept_quota(&state);
    let (user_id, ip_hash) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    if !well_formed(&request.code) {
        return refused(StatusCode::NOT_FOUND, FriendsErrorCode::InviteNotFound);
    }
    let done = async {
        let mut tx = state.db.begin().await?;
        // First the reader's own botarga, so the answer never depends on the code alone.
        if botarga::store::view(&mut tx, user_id).await?.is_none() {
            return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
        }
        let Some((owner, _)) = store::invite_owner(&mut tx, user_id, &request.code).await? else {
            return not_found_invite();
        };
        if !store::lock_pair(&mut tx, user_id, owner, true).await? {
            // The reader banned in between, or the owner: one answer for the owner's side.
            return if lock_user(&mut tx, user_id).await? {
                not_found_invite()
            } else {
                Ok(Outcome::Gone)
            };
        }
        // Under the locks: the code may have been rotated, or the owner may have blocked.
        if store::invite_owner(&mut tx, user_id, &request.code)
            .await?
            .map(|(id, _)| id)
            != Some(owner)
        {
            return not_found_invite();
        }
        if store::are_friends(&mut tx, user_id, owner).await? {
            return refusal(StatusCode::CONFLICT, FriendsErrorCode::AlreadyFriends);
        }
        if store::count(&mut tx, user_id).await? >= MAX_FRIENDS
            || store::count(&mut tx, owner).await? >= MAX_FRIENDS
        {
            return refusal(StatusCode::UNPROCESSABLE_ENTITY, FriendsErrorCode::Limit);
        }
        store::befriend(&mut tx, user_id, owner).await?;
        audit::record(&mut tx, user_id, Event::FriendAccept, &ip_hash).await?;
        tx.commit().await?;
        // Read after the commit, so the locks are not held while the list is built.
        let mut conn = state.db.acquire().await?;
        let view = view_of(&mut conn, &state, user_id).await?;
        Ok(view.map_or(Outcome::Gone, |view| Outcome::Done(StatusCode::OK, view)))
    }
    .await;
    answer(done, &state, "friends.accept_failed")
}

/// `POST /api/friends/remove`: ends the friendship and blocks the other from accepting the
/// reader's invite again (owner, 2026-11-15).
pub async fn remove(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<FriendRemoveRequest>, JsonRejection>,
) -> Response {
    let quota = Quota {
        ip: &state.rate.mutation_ip,
        user: &state.rate.mutation_user,
    };
    let (user_id, _) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    let Some(key) = name_key(&request.display_name) else {
        return refused(StatusCode::NOT_FOUND, FriendsErrorCode::NotFriend);
    };
    let done = async {
        let mut tx = state.db.begin().await?;
        let Some(other) = store::friend_by_key(&mut tx, user_id, &key, false).await? else {
            return refusal(StatusCode::NOT_FOUND, FriendsErrorCode::NotFriend);
        };
        // A banned friend can still be removed: only the reader has to be active.
        if !store::lock_pair(&mut tx, user_id, other, false).await?
            || !lock_user(&mut tx, user_id).await?
        {
            return Ok(Outcome::Gone);
        }
        if !store::unfriend(&mut tx, user_id, other).await? {
            return refusal(StatusCode::NOT_FOUND, FriendsErrorCode::NotFriend);
        }
        tx.commit().await?;
        // Read after the commit, so the locks are not held while the list is built.
        let mut conn = state.db.acquire().await?;
        let view = view_of(&mut conn, &state, user_id).await?;
        Ok(view.map_or(Outcome::Gone, |view| Outcome::Done(StatusCode::OK, view)))
    }
    .await;
    answer(done, &state, "friends.remove_failed")
}

/// `POST /api/friends/challenge`: a fight against the botarga of a friend, resolved at once.
/// Locks only the challenger; the friend's botarga is read, never written. Nothing of it moves
/// rating, XP, coins, `pvp_daily` or the draws.
pub async fn challenge(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<FriendChallengeRequest>, JsonRejection>,
) -> Response {
    let quota = Quota {
        ip: &state.rate.friends_ip,
        user: &state.rate.friends_user,
    };
    let (user_id, ip_hash) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    let Some(client_key) = canonical_key(&request.client_key) else {
        return invalid();
    };
    let key = name_key(&request.display_name);
    let done = async {
        let mut tx = state.db.begin().await?;
        if !lock_user(&mut tx, user_id).await? {
            return Ok(Outcome::Gone);
        }
        let Some(me) = botarga::store::view_locked(&mut tx, user_id).await? else {
            return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
        };
        // Under the lock, a retry cannot race its first try.
        if let Some(id) = store::by_key(&mut tx, user_id, client_key).await? {
            let detail = store::detail(&mut tx, user_id, id).await?;
            return Ok(detail.map_or(Outcome::Gone, |fight| {
                Outcome::Done(StatusCode::OK, FriendFight { fight })
            }));
        }
        let Some(key) = key else {
            return refusal(StatusCode::NOT_FOUND, FriendsErrorCode::NotFriend);
        };
        let Some(friend) = store::friend_by_key(&mut tx, user_id, &key, true).await? else {
            return refusal(StatusCode::NOT_FOUND, FriendsErrorCode::NotFriend);
        };
        if store::challenges_today(&mut tx, user_id).await? >= DAILY_CHALLENGES {
            return refusal(StatusCode::TOO_MANY_REQUESTS, FriendsErrorCode::DailyLimit);
        }
        let catalog = &state.botarga;
        let Some(them) = botarga::store::view(&mut tx, friend).await? else {
            return refusal(StatusCode::CONFLICT, FriendsErrorCode::BuildInvalid);
        };
        let (Ok(spec_a), Ok(spec_b)) = (
            build::resolve(catalog, me.progress.level, &me.build),
            build::resolve(catalog, them.progress.level, &them.build),
        ) else {
            return refusal(StatusCode::CONFLICT, FriendsErrorCode::BuildInvalid);
        };
        let ruleset = catalog.ruleset();
        let fight = match crate::pvp::resolve::resolve(&spec_a, &spec_b, ruleset) {
            Ok(fight) => fight,
            Err(error_kind) => {
                tracing::error!(event = "friends.challenge_failed", error_kind);
                return refusal(
                    StatusCode::SERVICE_UNAVAILABLE,
                    FriendsErrorCode::Unavailable,
                );
            }
        };
        let id = store::insert_challenge(
            &mut tx,
            &store::NewChallenge {
                attacker_id: user_id,
                defender_id: friend,
                client_key,
                ruleset_version: ruleset.version(),
                fight: &fight,
                look_a: &botarga::look_of(&me.build),
                look_b: &botarga::look_of(&them.build),
            },
        )
        .await?;
        store::count_challenge(&mut tx, user_id).await?;
        audit::record(
            &mut tx,
            user_id,
            Event::FriendChallenge { fight: id },
            &ip_hash,
        )
        .await?;
        let detail = store::detail(&mut tx, user_id, id).await?;
        tx.commit().await?;
        Ok(detail.map_or(Outcome::Gone, |fight| {
            Outcome::Done(StatusCode::CREATED, FriendFight { fight })
        }))
    }
    .await;
    answer(done, &state, "friends.challenge_failed")
}

#[derive(Deserialize)]
pub struct ListQuery {
    before: Option<String>,
}

fn read_quota(state: &AppState) -> Quota<'_> {
    Quota {
        ip: &state.rate.botarga_read_ip,
        user: &state.rate.botarga_read_user,
    }
}

/// `GET /api/friends/fights?before=`: the fights of the session's user, never a user parameter.
pub async fn fights(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    let quota = read_quota(&state);
    let (user_id, _) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let before = match query {
        Ok(Query(ListQuery { before: None })) => None,
        Ok(Query(ListQuery { before: Some(text) })) => match crate::fights::canonical_id(&text) {
            Some(id) => Some(id),
            None => return invalid(),
        },
        Err(_) => return invalid(),
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        Ok(Outcome::Done(
            StatusCode::OK,
            store::page(&mut conn, user_id, before).await?,
        ))
    };
    let done = tokio::time::timeout(ACQUIRE_TIMEOUT.saturating_mul(3), read)
        .await
        .unwrap_or(Err(sqlx::Error::PoolTimedOut));
    answer(done, &state, "friends.list_failed")
}

/// `GET /api/friends/fights/{id}`: only for its two sides; for anybody else, 404.
pub async fn fight_detail(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    segment: Result<Path<String>, PathRejection>,
) -> Response {
    let quota = read_quota(&state);
    let (user_id, _) = match session_of(&state, &quota, client.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let Some(id) = segment
        .ok()
        .and_then(|Path(segment)| crate::fights::canonical_id(&segment))
    else {
        return refused(StatusCode::NOT_FOUND, FriendsErrorCode::NotFound);
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        match store::detail(&mut conn, user_id, id).await? {
            Some(fight) => Ok(Outcome::Done(StatusCode::OK, FriendFight { fight })),
            None => refusal(StatusCode::NOT_FOUND, FriendsErrorCode::NotFound),
        }
    };
    let done = tokio::time::timeout(ACQUIRE_TIMEOUT.saturating_mul(3), read)
        .await
        .unwrap_or(Err(sqlx::Error::PoolTimedOut));
    answer(done, &state, "friends.detail_failed")
}

#[cfg(test)]
mod tests {
    use super::canonical_key;

    #[test]
    fn a_key_is_a_canonical_uuid() {
        assert!(canonical_key("0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10").is_some());
        assert!(canonical_key("0B0E8E5E-8A43-4C3C-9B55-0E6E3E1B2A10").is_none());
        assert!(canonical_key("").is_none());
    }
}
