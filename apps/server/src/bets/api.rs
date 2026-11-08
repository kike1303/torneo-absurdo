//! `POST /api/bets` and `GET /api/bets/mine` (012 RF25 to RF28, RF33). A bet is one transaction, with
//! the checks in the exact order of the table of RF25: the user is locked first (RF28), the clock is
//! read only after that lock, and the fight is held `FOR SHARE` by `lock_open_for_bet` (010 RF29).

use axum::Extension;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use protocol::{
    BetErrorCode, BetOutcome, BetPlaced, BetRequest, BetView, CoinAmount, FightErrorCode,
    MyFightBets, MyFightResult, PoolOutcome, Side, TimestampMs,
};
use serde::Deserialize;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::audit::{self, Event};
use super::facts::{self, Facts};
use super::http::{Quota, authorize, ip_key, lock_user, private, refused, unavailable};
use crate::app::AppState;
use crate::http::session;
use crate::ledger::{self, Coins};
use crate::net::ClientContext;

/// Bets of one user on one fight (012 RF15): checked under the user's lock.
pub const BETS_MAX_PER_USER_PER_FIGHT: i64 = 10;
/// Bets of one fight (012 RF15): what the hook is sized for. Counted without a lock: a few rows
/// over it by concurrency do not matter.
pub const BETS_MAX_PER_FIGHT: i64 = 20_000;

/// What a bet ends in.
#[derive(Debug)]
pub enum Placed {
    /// 201: the bet is in.
    Created(Box<BetPlaced>),
    /// 200: the same `client_key` with the same body, and nothing written (RF25, row 3).
    Repeated(Box<BetPlaced>),
    Refused(StatusCode, BetErrorCode),
    /// No such user, or a banned one.
    Gone,
}

/// `client_key` as `Uuid::to_string` writes it: any other form is not a key (RF25).
fn canonical_key(text: &str) -> Option<Uuid> {
    Uuid::parse_str(text)
        .ok()
        .filter(|key| key.to_string() == text)
}

const fn side_code(side: Side) -> &'static str {
    match side {
        Side::A => "a",
        Side::B => "b",
    }
}

fn side_of(code: &str) -> Option<Side> {
    match code {
        "a" => Some(Side::A),
        "b" => Some(Side::B),
        _ => None,
    }
}

fn coins(value: i64) -> Result<CoinAmount, sqlx::Error> {
    CoinAmount::new(value).ok_or_else(|| sqlx::Error::Protocol("coin amount".to_owned()))
}

/// The bets of a user on a fight, oldest first.
async fn bets_of(
    conn: &mut PgConnection,
    user_id: Uuid,
    fight_id: i32,
) -> Result<Vec<(Uuid, Side, i64, i64)>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, side, stake, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint FROM bets \
         WHERE user_id = $1 AND fight_id = $2 ORDER BY created_at, id",
    )
    .bind(user_id)
    .bind(fight_id)
    .fetch_all(&mut *conn)
    .await?;
    rows.iter()
        .map(|row| {
            let side: String = row.try_get(1)?;
            let side = side_of(&side).ok_or_else(|| sqlx::Error::Protocol("side".to_owned()))?;
            Ok((row.try_get(0)?, side, row.try_get(2)?, row.try_get(3)?))
        })
        .collect()
}

async fn fight_bets(conn: &mut PgConnection, fight_id: i32) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM bets WHERE fight_id = $1")
        .bind(fight_id)
        .fetch_one(conn)
        .await
}

/// The window of the fight is open now, as `lock_open_for_bet` would see it, without its lock.
async fn window_open(
    conn: &mut PgConnection,
    state: &AppState,
    fight_id: i32,
    now_ms: i64,
) -> Result<bool, sqlx::Error> {
    if !state.fights.accepts_bets() {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM fights WHERE id = $1 AND state = 'betting_open' \
         AND closes_at > timestamptz 'epoch' + $2::bigint * interval '1 millisecond')",
    )
    .bind(fight_id)
    .bind(now_ms)
    .fetch_one(conn)
    .await
}

/// The result of a settled fight for this user: every bet of one user on one fight is of one side
/// (RF26), so they share one outcome.
async fn result_of(
    conn: &mut PgConnection,
    user_id: Uuid,
    fight_id: i32,
) -> Result<Option<MyFightResult>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT kind, SUM(payout)::bigint, SUM(net)::bigint, \
         (SELECT outcome FROM pool_settlements WHERE fight_id = $2) \
         FROM (SELECT outcome AS kind, payout, net FROM bet_settlements \
         WHERE user_id = $1 AND fight_id = $2) AS mine GROUP BY kind",
    )
    .bind(user_id)
    .bind(fight_id)
    .fetch_optional(conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let outcome = match row.try_get::<&str, _>(0)? {
        "won" => BetOutcome::Won,
        "lost" => BetOutcome::Lost,
        "refunded" => BetOutcome::Refunded,
        _ => return Err(sqlx::Error::Protocol("outcome".to_owned())),
    };
    let pool = PoolOutcome::parse(row.try_get(3)?)
        .ok_or_else(|| sqlx::Error::Protocol("pool outcome".to_owned()))?;
    Ok(Some(MyFightResult {
        outcome,
        payout: coins(row.try_get(1)?)?,
        net: coins(row.try_get(2)?)?,
        refund_reason: if outcome == BetOutcome::Refunded {
            pool.refund_reason()
        } else {
            None
        },
    }))
}

/// `max_stake` of RF33: what the server would take now, by the same rules as rows 7 to 10, or 0.
fn max_stake(
    facts: &Facts,
    state: &AppState,
    staked: i64,
    mine: i64,
    of_fight: i64,
    open: bool,
) -> i64 {
    if !open || mine >= BETS_MAX_PER_USER_PER_FIGHT || of_fight >= BETS_MAX_PER_FIGHT {
        return 0;
    }
    let room = facts
        .fight_cap(state.bets)
        .0
        .saturating_sub(staked)
        .min(facts.balance.0);
    if room < state.bets.min_bet.0 { 0 } else { room }
}

/// `MyFightBets` of one user and fight, with the facts already read.
async fn mine_of(
    conn: &mut PgConnection,
    state: &AppState,
    facts: &Facts,
    user_id: Uuid,
    fight_id: i32,
    now_ms: i64,
) -> Result<MyFightBets, sqlx::Error> {
    let bets = bets_of(&mut *conn, user_id, fight_id).await?;
    let staked = bets.iter().map(|bet| bet.2).sum::<i64>();
    let open = window_open(&mut *conn, state, fight_id, now_ms).await?;
    let of_fight = if open {
        fight_bets(&mut *conn, fight_id).await?
    } else {
        0
    };
    let mine = i64::try_from(bets.len()).unwrap_or(i64::MAX);
    let max = max_stake(facts, state, staked, mine, of_fight, open);
    Ok(MyFightBets {
        fight_id: u32::try_from(fight_id).map_err(|_| sqlx::Error::Protocol("id".to_owned()))?,
        side: bets.first().map(|bet| bet.1),
        staked: coins(staked)?,
        max_stake: coins(max)?,
        bets: bets
            .iter()
            .map(|(id, side, stake, at)| {
                Ok(BetView {
                    id: id.to_string(),
                    side: *side,
                    stake: coins(*stake)?,
                    placed_at: TimestampMs::from_ms(*at),
                })
            })
            .collect::<Result<_, sqlx::Error>>()?,
        result: result_of(&mut *conn, user_id, fight_id).await?,
    })
}

/// The answer of a bet: the wallet and the bets of that fight, as they are now.
async fn placed(
    conn: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
    fight_id: i32,
    now_ms: i64,
) -> Result<Option<BetPlaced>, sqlx::Error> {
    let Some(facts) = facts::read(&mut *conn, user_id).await? else {
        return Ok(None);
    };
    let Some(wallet) = facts.wallet(state.bets) else {
        return Err(sqlx::Error::Protocol("wallet".to_owned()));
    };
    let mine = mine_of(conn, state, &facts, user_id, fight_id, now_ms).await?;
    Ok(Some(BetPlaced { wallet, mine }))
}

/// A checked request: the key in its form and the id inside `i32`.
pub struct BetOrder {
    pub fight_id: i32,
    pub side: Side,
    pub amount: i64,
    pub key: Uuid,
}

/// Rows 2 to 10 of RF25 and the writes, inside `tx`. The caller commits only on `Created`.
///
/// # Errors
/// A database error outside row 4, which never is one (010 RF29).
pub async fn place_in(
    tx: &mut PgConnection,
    state: &AppState,
    user_id: Uuid,
    order: &BetOrder,
    ip_hash: &[u8; 32],
) -> Result<Placed, sqlx::Error> {
    use BetErrorCode as E;
    let conflict = |code| Ok(Placed::Refused(StatusCode::CONFLICT, code));
    // Row 2.
    if !lock_user(&mut *tx, user_id).await? {
        return Ok(Placed::Gone);
    }
    // Row 3: under the user's lock, a retry cannot race its first try.
    let earlier = sqlx::query(
        "SELECT fight_id, side, stake FROM bets WHERE user_id = $1 AND client_key = $2",
    )
    .bind(user_id)
    .bind(order.key)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = earlier {
        let same = row.try_get::<i32, _>(0)? == order.fight_id
            && row.try_get::<&str, _>(1)? == side_code(order.side)
            && row.try_get::<i64, _>(2)? == order.amount;
        if !same {
            return conflict(E::KeyReused);
        }
        let now_ms = state.fights.clock().now_ms();
        return Ok(placed(tx, state, user_id, order.fight_id, now_ms)
            .await?
            .map_or(Placed::Gone, |answer| Placed::Repeated(Box::new(answer))));
    }
    // Row 4: the clock after the lock of row 2, and every failure is a closed window.
    let now_ms = state.fights.clock().now_ms();
    if state
        .fights
        .lock_open_for_bet(&mut *tx, order.fight_id, now_ms)
        .await
        .is_err()
    {
        return conflict(E::WindowClosed);
    }
    // Row 5.
    if order.amount < state.bets.min_bet.0 {
        return Ok(Placed::Refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            E::BelowMinimum,
        ));
    }
    let mine = bets_of(&mut *tx, user_id, order.fight_id).await?;
    // Row 6 (RF26).
    if mine.iter().any(|bet| bet.1 != order.side) {
        return conflict(E::SideLocked);
    }
    // Row 7.
    if i64::try_from(mine.len()).unwrap_or(i64::MAX) >= BETS_MAX_PER_USER_PER_FIGHT {
        return conflict(E::TooManyBets);
    }
    // Row 8.
    if fight_bets(&mut *tx, order.fight_id).await? >= BETS_MAX_PER_FIGHT {
        return conflict(E::PoolFull);
    }
    let Some(facts) = facts::read(&mut *tx, user_id).await? else {
        return Ok(Placed::Gone);
    };
    // Row 9 (RF27).
    let staked = mine.iter().map(|bet| bet.2).sum::<i64>();
    if staked.saturating_add(order.amount) > facts.fight_cap(state.bets).0 {
        return conflict(E::FightCapExceeded);
    }
    // Row 10.
    if facts.balance.0 < order.amount {
        return conflict(E::InsufficientBalance);
    }
    // The bet, its stake and its event, in this transaction (RF25). `ranked_stake` is the photo of
    // probation under the user's lock (RF5).
    let bet = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(bet)
    .bind(order.fight_id)
    .bind(user_id)
    .bind(side_code(order.side))
    .bind(order.amount)
    .bind(facts.probation().is_none())
    .bind(order.key)
    .execute(&mut *tx)
    .await?;
    ledger::bet_stake(&mut *tx, bet, order.fight_id, user_id, Coins(order.amount)).await?;
    audit::record(&mut *tx, user_id, Event::Bet { bet }, ip_hash).await?;
    Ok(placed(tx, state, user_id, order.fight_id, now_ms)
        .await?
        .map_or(Placed::Gone, |answer| Placed::Created(Box::new(answer))))
}

/// A bad request: 422 `bet.invalid_request`, never serde's text.
fn invalid() -> Response {
    refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        BetErrorCode::InvalidRequest,
    )
}

pub async fn place(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<BetRequest>, JsonRejection>,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.bet_ip,
        user: &state.rate.bet_user,
    };
    let info = match authorize(&state, &quota, &key, &headers, BetErrorCode::Unavailable).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let Ok(Json(request)) = body else {
        return invalid();
    };
    let Some(client_key) = canonical_key(&request.client_key) else {
        return invalid();
    };
    let Ok(fight_id) = i32::try_from(request.fight_id) else {
        return refused(StatusCode::CONFLICT, BetErrorCode::WindowClosed);
    };
    let order = BetOrder {
        fight_id,
        side: request.side,
        amount: i64::from(request.amount),
        key: client_key,
    };
    let ip_hash = key.hash(&state.auth.ip_hash_key);
    let done = async {
        let mut tx = state.db.begin().await?;
        let placed = place_in(&mut tx, &state, info.user_id, &order, &ip_hash).await?;
        if matches!(placed, Placed::Created(_)) {
            tx.commit().await?;
        }
        Ok::<_, sqlx::Error>(placed)
    }
    .await;
    match done {
        Ok(Placed::Created(answer)) => {
            private((StatusCode::CREATED, Json(*answer)).into_response())
        }
        Ok(Placed::Repeated(answer)) => private(Json(*answer).into_response()),
        Ok(Placed::Refused(status, code)) => refused(status, code),
        Ok(Placed::Gone) => session::unauthenticated(&state),
        Err(error) => {
            tracing::warn!(
                event = "bets.place_failed",
                error_kind = crate::db::error_kind(&error)
            );
            unavailable(BetErrorCode::Unavailable)
        }
    }
}

#[derive(Deserialize)]
pub struct MineQuery {
    fight_id: Option<String>,
}

/// `GET /api/bets/mine?fight_id=` (RF33): only the session's user, never a user parameter.
pub async fn mine(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    query: Result<Query<MineQuery>, QueryRejection>,
) -> Response {
    let key = ip_key(client.as_ref());
    let quota = Quota {
        ip: &state.rate.wallet_read_ip,
        user: &state.rate.wallet_read_user,
    };
    let info = match authorize(&state, &quota, &key, &headers, BetErrorCode::Unavailable).await {
        Ok(info) => info,
        Err(response) => return *response,
    };
    let not_found = || refused(StatusCode::NOT_FOUND, FightErrorCode::NotFound);
    let Some(fight_id) = query
        .ok()
        .and_then(|Query(q)| q.fight_id)
        .and_then(|text| crate::fights::canonical_id(&text))
    else {
        return not_found();
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM fights WHERE id = $1)")
            .bind(fight_id)
            .fetch_one(&mut *conn)
            .await?;
        if !exists {
            return Ok(Err(true));
        }
        let Some(facts) = facts::read(&mut conn, info.user_id).await? else {
            return Ok(Err(false));
        };
        let now_ms = state.fights.clock().now_ms();
        let mine = mine_of(&mut conn, &state, &facts, info.user_id, fight_id, now_ms).await?;
        Ok::<_, sqlx::Error>(Ok(mine))
    }
    .await;
    match read {
        Ok(Ok(mine)) => private(Json(mine).into_response()),
        Ok(Err(true)) => not_found(),
        Ok(Err(false)) => session::unauthenticated(&state),
        Err(error) => {
            tracing::warn!(
                event = "bets.mine_failed",
                error_kind = crate::db::error_kind(&error)
            );
            unavailable(BetErrorCode::Unavailable)
        }
    }
}
