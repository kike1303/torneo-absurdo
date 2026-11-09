//! `GET /api/bets/history?before=` (019): the fights the session's user bet on, the most recent id
//! first, one row per fight. Read-only: nothing here writes, and the user comes from the session
//! alone.

use axum::Extension;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use protocol::{
    BetErrorCode, BetHistoryItem, BetHistoryPage, BetOutcome, CoinAmount, FightErrorCode,
    MyFightResult, PoolOutcome, Side, TimestampMs,
};
use serde::Deserialize;
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::http::{Quota, authorize, ip_key, private, refused, unavailable};
use crate::app::AppState;
use crate::net::ClientContext;

/// Fights per page. Fixed: there is no `limit`, as in `GET /api/fights` (011 RF7).
pub const HISTORY_PAGE_SIZE: usize = 20;

/// One fight per row, `PAGE + 1` of them, so the page knows whether another one follows. First the
/// fight ids, walking `bets_user_fight_idx` backwards, and only then the sums of those few: a page
/// never reads the whole history (019, security review 3). `result` needs the settled fight: the
/// hook writes `bet_settlements` in the transaction that settles it (012 RF18), and the state is the
/// second barrier. Its rows are joined by `bet_id`, the key that ties them to their owner.
const PAGE_SQL: &str = "\
WITH ids AS (\
    SELECT DISTINCT fight_id FROM bets WHERE user_id = $1 AND fight_id < $2::bigint \
    ORDER BY fight_id DESC LIMIT $3) \
SELECT ids.fight_id, f.slug_a, f.slug_b, f.state = 'settled', m.side, m.n, m.staked, \
       m.placed_at, r.kind, r.payout, r.net, p.pool_kind \
FROM ids JOIN fights f ON f.id = ids.fight_id \
CROSS JOIN LATERAL (\
    SELECT min(side) AS side, count(*)::bigint AS n, sum(stake)::bigint AS staked, \
           (extract(epoch FROM max(created_at)) * 1000)::bigint AS placed_at \
    FROM bets WHERE user_id = $1 AND fight_id = ids.fight_id) m \
LEFT JOIN LATERAL (\
    SELECT min(s.kind) AS kind, sum(s.payout)::bigint AS payout, \
           sum(s.net)::bigint AS net \
    FROM bets b JOIN (SELECT bet_id, outcome AS kind, payout, net FROM bet_settlements) s \
        ON s.bet_id = b.id \
    WHERE b.user_id = $1 AND b.fight_id = ids.fight_id HAVING count(*) > 0) r \
    ON f.state = 'settled' \
LEFT JOIN (SELECT fight_id, outcome AS pool_kind FROM pool_settlements) p \
    ON p.fight_id = ids.fight_id AND f.state = 'settled' \
ORDER BY ids.fight_id DESC";

/// How long a page may take: the same bound as the reads of the fights.
const STATEMENT_TIMEOUT: &str = "SET LOCAL statement_timeout = '2s'";

fn protocol_error(what: &str) -> sqlx::Error {
    sqlx::Error::Protocol(what.to_owned())
}

fn coins(value: i64) -> Result<CoinAmount, sqlx::Error> {
    CoinAmount::new(value).ok_or_else(|| protocol_error("coin amount"))
}

fn result_of(row: &PgRow) -> Result<Option<MyFightResult>, sqlx::Error> {
    let Some(outcome) = row.try_get::<Option<&str>, _>(8)? else {
        return Ok(None);
    };
    let outcome = match outcome {
        "won" => BetOutcome::Won,
        "lost" => BetOutcome::Lost,
        "refunded" => BetOutcome::Refunded,
        _ => return Err(protocol_error("outcome")),
    };
    // A refund without its pool is the same broken invariant as a settled fight without results.
    let refund_reason = if outcome == BetOutcome::Refunded {
        let text = row
            .try_get::<Option<&str>, _>(11)?
            .ok_or_else(|| protocol_error("pool outcome"))?;
        PoolOutcome::parse(text)
            .ok_or_else(|| protocol_error("pool outcome"))?
            .refund_reason()
    } else {
        None
    };
    Ok(Some(MyFightResult {
        outcome,
        payout: coins(row.try_get(9)?)?,
        net: coins(row.try_get(10)?)?,
        refund_reason,
    }))
}

fn item_of(row: &PgRow) -> Result<BetHistoryItem, sqlx::Error> {
    let side = match row.try_get::<&str, _>(4)? {
        "a" => Side::A,
        "b" => Side::B,
        _ => return Err(protocol_error("side")),
    };
    let settled: bool = row.try_get(3)?;
    Ok(BetHistoryItem {
        fight_id: u32::try_from(row.try_get::<i32, _>(0)?).map_err(|_| protocol_error("id"))?,
        a: row.try_get(1)?,
        b: row.try_get(2)?,
        side,
        staked: coins(row.try_get(6)?)?,
        bets: u32::try_from(row.try_get::<i64, _>(5)?).map_err(|_| protocol_error("count"))?,
        placed_at: TimestampMs::from_ms(row.try_get(7)?),
        settled,
        result: if settled {
            // A settled fight with bets and no result is an invariant broken, never `null` in
            // silence (019, security review 6).
            Some(result_of(row)?.ok_or_else(|| protocol_error("settled without result"))?)
        } else {
            None
        },
    })
}

/// One page of the history of `user_id`, below `before` (from the top without one). A `before` is
/// only a cursor: whether that fight exists, or who bet on it, is never looked at.
///
/// # Errors
/// A database error, or a row the `CHECK`s of the tables forbid.
pub async fn page(
    conn: &mut PgConnection,
    user_id: Uuid,
    before: Option<i32>,
) -> Result<BetHistoryPage, sqlx::Error> {
    let below = before.map_or(i64::from(i32::MAX) + 1, i64::from);
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;
    sqlx::query(STATEMENT_TIMEOUT).execute(&mut *tx).await?;
    let rows = sqlx::query(PAGE_SQL)
        .bind(user_id)
        .bind(below)
        .bind(i64::try_from(HISTORY_PAGE_SIZE + 1).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut items = rows.iter().map(item_of).collect::<Result<Vec<_>, _>>()?;
    let next_before = (items.len() > HISTORY_PAGE_SIZE)
        .then(|| items.get(HISTORY_PAGE_SIZE - 1).map(|item| item.fight_id))
        .flatten();
    items.truncate(HISTORY_PAGE_SIZE);
    Ok(BetHistoryPage { items, next_before })
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    before: Option<String>,
}

/// `GET /api/bets/history?before=`: only the session's user, never a user parameter. The cache of
/// `bets/mine` (012 RF33) and, on purpose, its bucket: the history is one more read of the wallet.
pub async fn handler(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    query: Result<Query<HistoryQuery>, QueryRejection>,
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
    // A `before` that is not an id is the 404 of `bets/mine`, and so is a repeated one.
    let before = match query {
        Ok(Query(HistoryQuery { before: None })) => None,
        Ok(Query(HistoryQuery { before: Some(text) })) => {
            match crate::fights::canonical_id(&text) {
                Some(id) => Some(id),
                None => return refused(StatusCode::NOT_FOUND, FightErrorCode::NotFound),
            }
        }
        Err(_) => return refused(StatusCode::NOT_FOUND, FightErrorCode::NotFound),
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        page(&mut conn, info.user_id, before).await
    }
    .await;
    match read {
        Ok(page) => private(Json(page).into_response()),
        Err(error) => {
            tracing::warn!(
                event = "bets.history_failed",
                error_kind = crate::db::error_kind(&error)
            );
            unavailable(BetErrorCode::Unavailable)
        }
    }
}
