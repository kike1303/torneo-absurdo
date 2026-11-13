//! `POST /api/pvp/fights`, `GET /api/pvp/fights` and `GET /api/pvp/fights/{id}` (021 "Buscar pelea"
//! and "Lectura").
//!
//! The search runs in one transaction with a `lock_timeout` of 1 s: the attacker locked first (a
//! ban in between answers 401, like `banned_writes` expects), the retry of a `client_key` found
//! under that lock, the defender locked `SKIP LOCKED`, the fight resolved with the engine, and the
//! row, the counter of the day, the event and the entry of the coins written before the commit.
//! The two private answers are `private, no-store` with `Vary: Cookie`; the public detail is
//! `no-store` with the limit of the tournament fights. No log carries a seed, a spec or a key.

use axum::Extension;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Json, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use engine::CharacterSpec;
use protocol::{BotargaErrorCode, ErrorCode, PvpErrorCode, PvpFightCreated, PvpFightRequest};
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::matchmaking::{self, Seeker};
use super::rewards::{self, PAIR_FIGHTS_PER_DAY, Rival};
use super::store::{self, DefenderRef, NewFight};
use crate::app::AppState;
use crate::bets::audit::{self, Event};
use crate::bets::http::{Quota, authorize, ip_key, lock_user, private, refused, unavailable};
use crate::botarga::{self, build};
use crate::db::{ACQUIRE_TIMEOUT, error_kind};
use crate::http::errors::error_response;
use crate::http::session;
use crate::ledger::{self, Coins};
use crate::net::ClientContext;

/// What a search ends in. `Gone`: no user, or a banned one.
enum Outcome {
    Created(Box<PvpFightCreated>),
    Repeated(Box<PvpFightCreated>),
    Refused(StatusCode, ErrorCode),
    Gone,
}

fn refusal(status: StatusCode, code: impl Into<ErrorCode>) -> Result<Outcome, sqlx::Error> {
    Ok(Outcome::Refused(status, code.into()))
}

/// A canonical UUID: the text a client sends is the one it reads back.
fn canonical_key(text: &str) -> Option<Uuid> {
    Uuid::parse_str(text)
        .ok()
        .filter(|key| key.to_string() == text)
}

/// The side `b` of an attack, and what it means for the rewards.
struct Opponent {
    spec: CharacterSpec,
    defender: Option<Uuid>,
    rival: Rival,
}

/// `POST /api/pvp/fights`.
pub async fn create(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<PvpFightRequest>, JsonRejection>,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.pvp_ip,
        user: &state.rate.pvp_user,
    };
    let info = match authorize(&state, &quota, &key, &headers, PvpErrorCode::Unavailable).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    // Never axum's or serde's text, which copies parts of the body.
    let invalid = || {
        refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            PvpErrorCode::InvalidRequest,
        )
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    let Some(client_key) = canonical_key(&request.client_key) else {
        return invalid();
    };
    let ip_hash = key.hash(&state.auth.ip_hash_key);
    // Read (once per process) before any lock is taken.
    let rivals = state.pvp_rivals.specs().await;
    let done = async {
        let mut tx = state.db.begin().await?;
        let outcome = search(&mut tx, &state, info.user_id, client_key, &rivals, &ip_hash).await?;
        if matches!(outcome, Outcome::Created(_)) {
            tx.commit().await?;
        }
        Ok::<_, sqlx::Error>(outcome)
    }
    .await;
    match done {
        Ok(Outcome::Created(answer)) => {
            private((StatusCode::CREATED, Json(*answer)).into_response())
        }
        Ok(Outcome::Repeated(answer)) => private(Json(*answer).into_response()),
        Ok(Outcome::Refused(status, code)) => refused(status, code),
        Ok(Outcome::Gone) => session::unauthenticated(&state),
        Err(error) => {
            let kind = error_kind(&error);
            tracing::warn!(event = "pvp.attack_failed", error_kind = kind);
            if kind == "lock_timeout" {
                unavailable(PvpErrorCode::Busy)
            } else {
                unavailable(PvpErrorCode::Unavailable)
            }
        }
    }
}

/// The steps of 021 "Buscar pelea", inside `tx`. The caller commits only on `Created`.
async fn search(
    tx: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
    client_key: Uuid,
    rivals: &[CharacterSpec],
    ip_hash: &[u8; 32],
) -> Result<Outcome, sqlx::Error> {
    // Step 1: the user, then its botarga, both locked.
    if !lock_user(&mut *tx, user_id).await? {
        return Ok(Outcome::Gone);
    }
    let Some(me) = botarga::store::view_locked(&mut *tx, user_id).await? else {
        return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
    };
    // Step 2: under the lock, a retry cannot race its first try.
    if let Some(id) = store::by_key(&mut *tx, user_id, client_key).await? {
        return Ok(created(&mut *tx, id, user_id)
            .await?
            .map_or(Outcome::Gone, |answer| Outcome::Repeated(Box::new(answer))));
    }
    let attacks = store::attacks_today(&mut *tx, user_id).await?;
    if attacks >= rewards::DAILY_ATTACKS_MAX {
        return refusal(StatusCode::TOO_MANY_REQUESTS, PvpErrorCode::DailyLimit);
    }
    let daily_index = attacks.saturating_add(1);
    let level = me.progress.level;
    // 021 "Ajustes": the saved build, with the catalog of this binary.
    let Ok(spec_a) = build::resolve(&state.botarga, level, &me.build) else {
        return refusal(StatusCode::CONFLICT, PvpErrorCode::BuildInvalid);
    };
    // Step 3.
    let recent = matchmaking::recent_rivals(&mut *tx, user_id).await?;
    let seeker = Seeker {
        user_id,
        level,
        rating: me.rating,
    };
    let opponent = match matchmaking::pick(&mut *tx, &state.botarga, seeker, &recent).await? {
        Some(defender) => {
            let pair = store::pair_fights_today(&mut *tx, user_id, defender.user_id).await?;
            let probation = matchmaking::probation(&mut *tx, user_id).await?;
            let pair_open = pair < PAIR_FIGHTS_PER_DAY;
            Opponent {
                spec: defender.spec,
                defender: Some(defender.user_id),
                rival: Rival::Player {
                    counts: !probation && !defender.probation && pair_open,
                    pair_open,
                    rating_gap: i32::from(defender.rating) - i32::from(me.rating),
                },
            }
        }
        None => {
            let ruleset = state.botarga.ruleset();
            let candidates: Vec<CharacterSpec> = rivals
                .iter()
                .filter_map(|spec| super::rival::scaled(spec, level, ruleset))
                .collect();
            let picked = u64::try_from(candidates.len())
                .ok()
                .and_then(super::uniform)
                .and_then(|at| usize::try_from(at).ok())
                .and_then(|at| candidates.into_iter().nth(at));
            let Some(spec) = picked else {
                return refusal(StatusCode::SERVICE_UNAVAILABLE, PvpErrorCode::NoRival);
            };
            Opponent {
                spec,
                defender: None,
                rival: Rival::Tournament,
            }
        }
    };
    // Step 4.
    let ruleset = state.botarga.ruleset();
    let fight = match super::resolve::resolve(&spec_a, &opponent.spec, ruleset) {
        Ok(fight) => fight,
        Err(error_kind) => {
            tracing::error!(event = "pvp.attack_failed", error_kind);
            return refusal(StatusCode::SERVICE_UNAVAILABLE, PvpErrorCode::Unavailable);
        }
    };
    // Step 5.
    let rewards = rewards::compute(
        daily_index,
        fight.winner,
        opponent.rival,
        state.pvp.win_coins(),
    );
    let defender = match opponent.defender {
        Some(id) => DefenderRef::Player(id),
        None => DefenderRef::Tournament(&opponent.spec.slug),
    };
    let id = store::insert(
        &mut *tx,
        &NewFight {
            attacker_id: user_id,
            defender,
            client_key,
            ruleset_version: ruleset.version(),
            spec_a: &fight.spec_a,
            spec_b: &fight.spec_b,
            seed: &fight.seed,
            log: &fight.log,
            winner: fight.winner,
            end_reason: fight.end_reason,
            rewards: &rewards,
            daily_index,
        },
    )
    .await?;
    store::count_attack(&mut *tx, user_id).await?;
    if rewards.coins_attacker > 0 {
        let coins = Coins(i64::from(rewards.coins_attacker));
        ledger::pvp_reward(&mut *tx, id, user_id, coins).await?;
    }
    audit::record(&mut *tx, user_id, Event::PvpAttack { fight: id }, ip_hash).await?;
    // The XP, the levels and their draws of both botargas, whose rows this transaction holds.
    // TA-64 adds the ratings here.
    let catalog = &state.botarga;
    botarga::store::grant_xp(&mut *tx, catalog, user_id, rewards.xp_attacker).await?;
    if let Some(defender_id) = opponent.defender {
        botarga::store::grant_xp(&mut *tx, catalog, defender_id, rewards.xp_defender).await?;
    }
    Ok(created(&mut *tx, id, user_id)
        .await?
        .map_or(Outcome::Gone, |answer| Outcome::Created(Box::new(answer))))
}

/// The answer for fight `id` of `attacker_id`, read back inside the transaction.
async fn created(
    conn: &mut PgConnection,
    id: i32,
    attacker_id: Uuid,
) -> Result<Option<PvpFightCreated>, sqlx::Error> {
    let (Some(fight), Some(reward), Some(view)) = (
        store::detail(&mut *conn, id).await?,
        store::attacker_reward(&mut *conn, id, attacker_id).await?,
        botarga::store::view(&mut *conn, attacker_id).await?,
    ) else {
        return Ok(None);
    };
    Ok(Some(PvpFightCreated {
        fight,
        reward,
        progress: view.progress,
    }))
}

#[derive(Deserialize)]
pub struct ListQuery {
    before: Option<String>,
}

/// `GET /api/pvp/fights?before=`: the fights of the session's user, never a user parameter.
pub async fn list(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.botarga_read_ip,
        user: &state.rate.botarga_read_user,
    };
    let info = match authorize(&state, &quota, &key, &headers, PvpErrorCode::Unavailable).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let before = match query {
        Ok(Query(ListQuery { before: None })) => None,
        Ok(Query(ListQuery { before: Some(text) })) => match crate::fights::canonical_id(&text) {
            Some(id) => Some(id),
            None => {
                return refused(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    PvpErrorCode::InvalidRequest,
                );
            }
        },
        Err(_) => {
            return refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                PvpErrorCode::InvalidRequest,
            );
        }
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        store::page(&mut conn, info.user_id, before).await
    };
    match tokio::time::timeout(ACQUIRE_TIMEOUT.saturating_mul(3), read).await {
        Ok(Ok(page)) => private(Json(page).into_response()),
        Ok(Err(error)) => {
            tracing::warn!(event = "pvp.list_failed", error_kind = error_kind(&error));
            unavailable(PvpErrorCode::Unavailable)
        }
        Err(_) => {
            tracing::warn!(event = "pvp.list_failed", error_kind = "timeout");
            unavailable(PvpErrorCode::Unavailable)
        }
    }
}

/// `GET /api/pvp/fights/{id}`: public. The route adds `no-store` and the limit of the tournament
/// fights.
pub async fn detail(
    State(state): State<AppState>,
    segment: Result<Path<String>, PathRejection>,
) -> Response {
    let Some(id) = segment
        .ok()
        .and_then(|Path(segment)| crate::fights::canonical_id(&segment))
    else {
        return error_response(StatusCode::NOT_FOUND, PvpErrorCode::NotFound);
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        store::detail(&mut conn, id).await
    };
    match tokio::time::timeout(ACQUIRE_TIMEOUT.saturating_mul(3), read).await {
        Ok(Ok(Some(detail))) => Json(detail).into_response(),
        Ok(Ok(None)) => error_response(StatusCode::NOT_FOUND, PvpErrorCode::NotFound),
        Ok(Err(error)) => {
            tracing::warn!(event = "pvp.detail_failed", error_kind = error_kind(&error));
            error_response(StatusCode::SERVICE_UNAVAILABLE, PvpErrorCode::Unavailable)
        }
        Err(_) => {
            tracing::warn!(event = "pvp.detail_failed", error_kind = "timeout");
            error_response(StatusCode::SERVICE_UNAVAILABLE, PvpErrorCode::Unavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_key;

    #[test]
    fn a_key_is_a_canonical_uuid() {
        let key = "0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10";
        assert!(canonical_key(key).is_some());
        for bad in [
            "0B0E8E5E-8A43-4C3C-9B55-0E6E3E1B2A10",
            "0b0e8e5e8a434c3c9b550e6e3e1b2a10",
            "{0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10}",
            "",
            "key",
        ] {
            assert!(canonical_key(bad).is_none(), "{bad}");
        }
    }
}
