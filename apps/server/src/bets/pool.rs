//! The pools (012 RF34, RF35, RF47). Every [`POOL_INTERVAL`], **one** query reads the sums and counts
//! of the fights in `betting_open`, and of those that were open the turn before; a pool that changed
//! and has bets goes to the hub, and a fight that stops being open sends its last value if it changed
//! and leaves the cache. With or without spectators, one query each 2 s. A pool is sums and counts,
//! never who bet: it does not depend on the seed, and the log does not exist until the close.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use axum::Extension;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use protocol::{
    BetErrorCode, BetQuote, CoinAmount, FightErrorCode, FightPool, PoolOutcome, PoolSettlement,
    PoolSide, Side, TimestampMs,
};
use serde::Deserialize;
use sqlx::{PgPool, Row};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use super::http::{Quota, authorize, ip_key, private, refused};
use super::parimutuel;
use crate::app::AppState;
use crate::fights::FightsShared;
use crate::http::errors::error_response;
use crate::net::ClientContext;

/// How often the pools are read (RF35). A constant of the code, never the environment.
pub const POOL_INTERVAL: Duration = Duration::from_secs(2);

/// The sums of one fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolEntry {
    pub stake_a: i64,
    pub bets_a: i64,
    pub stake_b: i64,
    pub bets_b: i64,
    pub rake_bp: u16,
    pub closes_at_ms: i64,
    pub open: bool,
}

impl PoolEntry {
    const fn has_bets(&self) -> bool {
        self.bets_a + self.bets_b > 0
    }

    fn side(&self, side: Side) -> Option<PoolSide> {
        let (own, bets, other) = match side {
            Side::A => (self.stake_a, self.bets_a, self.stake_b),
            Side::B => (self.stake_b, self.bets_b, self.stake_a),
        };
        Some(PoolSide {
            stake: CoinAmount::new(own)?,
            bets: u32::try_from(bets).ok()?,
            payout_bp: parimutuel::payout_bp(own, other, self.rake_bp),
        })
    }

    fn pool(
        &self,
        fight_id: i32,
        now_ms: i64,
        settlement: Option<PoolSettlement>,
    ) -> Option<FightPool> {
        Some(FightPool {
            fight_id: u32::try_from(fight_id).ok()?,
            a: self.side(Side::A)?,
            b: self.side(Side::B)?,
            rake_bp: self.rake_bp,
            server_time: TimestampMs::from_ms(now_ms),
            settlement,
        })
    }
}

/// The pools of the open fights, and of those that just closed until their last value is sent.
#[derive(Default)]
pub struct PoolCache {
    entries: RwLock<BTreeMap<i32, PoolEntry>>,
}

impl PoolCache {
    /// The entry of an open window, if it is one now.
    pub fn open(&self, fight_id: i32, now_ms: i64) -> Option<PoolEntry> {
        let entries = self.entries.read().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(&fight_id)
            .filter(|entry| entry.open && entry.closes_at_ms > now_ms)
            .copied()
    }
}

/// The sums of `fight_id`, or of the open ones and `previous`. One statement.
const POOLS_SQL: &str = "\
SELECT f.id, f.rake_bp, (EXTRACT(EPOCH FROM f.closes_at) * 1000)::bigint, f.state = 'betting_open', \
  COALESCE(SUM(b.stake) FILTER (WHERE b.side = 'a'), 0)::bigint, count(b.id) FILTER (WHERE b.side = 'a'), \
  COALESCE(SUM(b.stake) FILTER (WHERE b.side = 'b'), 0)::bigint, count(b.id) FILTER (WHERE b.side = 'b') \
FROM fights f LEFT JOIN bets b ON b.fight_id = f.id \
WHERE f.state = 'betting_open' OR f.id = ANY($1) \
GROUP BY f.id";

fn entry_of(row: &sqlx::postgres::PgRow) -> Result<(i32, PoolEntry), sqlx::Error> {
    let rake: i16 = row.try_get(1)?;
    Ok((
        row.try_get(0)?,
        PoolEntry {
            rake_bp: u16::try_from(rake).map_err(|_| sqlx::Error::Protocol("rake".to_owned()))?,
            closes_at_ms: row.try_get(2)?,
            open: row.try_get(3)?,
            stake_a: row.try_get(4)?,
            bets_a: row.try_get(5)?,
            stake_b: row.try_get(6)?,
            bets_b: row.try_get(7)?,
        },
    ))
}

/// The sums of one fight, open or not.
const POOL_OF_SQL: &str = "\
SELECT f.id, f.rake_bp, (EXTRACT(EPOCH FROM f.closes_at) * 1000)::bigint, f.state = 'betting_open', \
  COALESCE(SUM(b.stake) FILTER (WHERE b.side = 'a'), 0)::bigint, count(b.id) FILTER (WHERE b.side = 'a'), \
  COALESCE(SUM(b.stake) FILTER (WHERE b.side = 'b'), 0)::bigint, count(b.id) FILTER (WHERE b.side = 'b') \
FROM fights f LEFT JOIN bets b ON b.fight_id = f.id WHERE f.id = $1 GROUP BY f.id";

/// One turn of the task: read, compare, publish.
pub async fn refresh(db: &PgPool, cache: &PoolCache, fights: &FightsShared) {
    let previous: Vec<i32> = {
        let entries = cache.entries.read().unwrap_or_else(PoisonError::into_inner);
        entries.keys().copied().collect()
    };
    let rows = match sqlx::query(POOLS_SQL).bind(&previous).fetch_all(db).await {
        Ok(rows) => rows,
        Err(error) => {
            // The last values stay; `quote` never answers past `closes_at` anyway.
            tracing::warn!(
                event = "bets.pool_read_failed",
                error_kind = crate::db::error_kind(&error)
            );
            return;
        }
    };
    let now_ms = fights.clock().now_ms();
    let mut fresh = BTreeMap::new();
    let mut changed = Vec::new();
    {
        let entries = cache.entries.read().unwrap_or_else(PoisonError::into_inner);
        for row in &rows {
            let Ok((id, entry)) = entry_of(row) else {
                continue;
            };
            let before = entries.get(&id);
            let moved = before.is_none_or(|old| {
                (old.stake_a, old.bets_a, old.stake_b, old.bets_b)
                    != (entry.stake_a, entry.bets_a, entry.stake_b, entry.bets_b)
            });
            if moved && entry.has_bets() {
                changed.push((id, entry));
            }
            if entry.open {
                fresh.insert(id, entry);
            }
        }
    }
    *cache
        .entries
        .write()
        .unwrap_or_else(PoisonError::into_inner) = fresh;
    for (id, entry) in changed {
        if let Some(pool) = entry.pool(id, now_ms, None) {
            fights.hub().publish_pool(&pool);
        }
    }
}

/// The task of `app::serve`, until `stop`.
pub async fn run(
    db: PgPool,
    cache: Arc<PoolCache>,
    fights: Arc<FightsShared>,
    stop: CancellationToken,
) {
    let mut interval = tokio::time::interval(POOL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            _ = interval.tick() => refresh(&db, &cache, &fights).await,
        }
    }
}

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

/// `GET /api/fights/{id}/pool` (RF34): from the cache while the window is open, from the database
/// otherwise, with its settlement. No session.
pub async fn pool(State(state): State<AppState>, Path(segment): Path<String>) -> Response {
    let unavailable = || {
        no_store(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            FightErrorCode::Unavailable,
        ))
    };
    let not_found = || {
        no_store(error_response(
            StatusCode::NOT_FOUND,
            FightErrorCode::NotFound,
        ))
    };
    let Some(id) = crate::fights::canonical_id(&segment) else {
        return not_found();
    };
    let now_ms = state.fights.clock().now_ms();
    if let Some(entry) = state.pools.open(id, now_ms) {
        return entry
            .pool(id, now_ms, None)
            .map_or_else(unavailable, |pool| no_store(Json(pool).into_response()));
    }
    let read = async {
        let Some(row) = sqlx::query(POOL_OF_SQL)
            .bind(id)
            .fetch_optional(&state.db)
            .await?
        else {
            return Ok(None);
        };
        let (_, entry) = entry_of(&row)?;
        let settled =
            sqlx::query("SELECT outcome, rake, residue FROM pool_settlements WHERE fight_id = $1")
                .bind(id)
                .fetch_optional(&state.db)
                .await?;
        let settlement = match settled {
            Some(row) => Some(PoolSettlement {
                outcome: PoolOutcome::parse(row.try_get(0)?)
                    .ok_or_else(|| sqlx::Error::Protocol("outcome".to_owned()))?,
                rake: CoinAmount::new(row.try_get(1)?)
                    .ok_or_else(|| sqlx::Error::Protocol("rake".to_owned()))?,
                residue: CoinAmount::new(row.try_get(2)?)
                    .ok_or_else(|| sqlx::Error::Protocol("residue".to_owned()))?,
            }),
            None => None,
        };
        Ok::<_, sqlx::Error>(Some(entry.pool(id, now_ms, settlement)))
    }
    .await;
    match read {
        Ok(Some(Some(pool))) => no_store(Json(pool).into_response()),
        Ok(None) => not_found(),
        Ok(Some(None)) | Err(_) => unavailable(),
    }
}

#[derive(Deserialize)]
pub struct QuoteQuery {
    side: String,
    amount: String,
}

/// `GET /api/fights/{id}/quote?side=&amount=` (RF47): only the session and the cache, never more of
/// the database. No caps and no balance: those are of the `POST`.
pub async fn quote(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    Path(segment): Path<String>,
    query: Result<Query<QuoteQuery>, QueryRejection>,
) -> Response {
    let key = ip_key(client.as_ref());
    let limits = Quota {
        ip: &state.rate.quote_ip,
        user: &state.rate.quote_user,
    };
    if let Err(response) =
        authorize(&state, &limits, &key, &headers, BetErrorCode::Unavailable).await
    {
        return *response;
    }
    let invalid = || {
        refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            BetErrorCode::InvalidRequest,
        )
    };
    let Ok(Query(query)) = query else {
        return invalid();
    };
    let side = match query.side.as_str() {
        "a" => Side::A,
        "b" => Side::B,
        _ => return invalid(),
    };
    let Ok(amount) = query.amount.parse::<u32>() else {
        return invalid();
    };
    if query.amount.starts_with('+') || (query.amount.len() > 1 && query.amount.starts_with('0')) {
        return invalid();
    }
    let amount = i64::from(amount);
    if amount < state.bets.min_bet.0 {
        return refused(StatusCode::UNPROCESSABLE_ENTITY, BetErrorCode::BelowMinimum);
    }
    let closed = || refused(StatusCode::CONFLICT, BetErrorCode::WindowClosed);
    let Some(id) = crate::fights::canonical_id(&segment) else {
        return closed();
    };
    let now_ms = state.fights.clock().now_ms();
    let Some(entry) = state.pools.open(id, now_ms) else {
        return closed();
    };
    let (own, other) = match side {
        Side::A => (entry.stake_a, entry.stake_b),
        Side::B => (entry.stake_b, entry.stake_a),
    };
    let quote = parimutuel::quote(amount, own, other, entry.rake_bp)
        .ok()
        .and_then(|(payout, one_sided)| {
            Some(BetQuote {
                fight_id: u32::try_from(id).ok()?,
                side,
                amount: CoinAmount::new(amount)?,
                payout: CoinAmount::new(payout)?,
                net: CoinAmount::new(payout.checked_sub(amount)?)?,
                one_sided,
                rake_bp: entry.rake_bp,
            })
        });
    match quote {
        Some(quote) => private(Json(quote).into_response()),
        None => invalid(),
    }
}
