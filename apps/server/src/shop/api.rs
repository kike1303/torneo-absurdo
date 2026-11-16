//! The routes of the shop (024). Every answer is private (`private, no-store`). No log carries an
//! item, a key or a balance: only `event` and `error_kind` (ADR-0012 A8).

use axum::Extension;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use engine::loadout::tier;
use protocol::{
    BotargaErrorCode, CoinAmount, ErrorCode, ShopBuyRequest, ShopErrorCode, ShopItem, ShopItemKind,
    ShopPurchase, ShopView,
};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{store, well_formed};
use crate::app::AppState;
use crate::bets::audit::{self, Event};
use crate::bets::facts;
use crate::bets::http::{Quota, authorize, ip_key, lock_user, private, refused, unavailable};
use crate::botarga;
use crate::db::{ACQUIRE_TIMEOUT, error_kind};
use crate::http::session;
use crate::ledger::{self, Coins};
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

fn invalid() -> Response {
    refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        ShopErrorCode::InvalidRequest,
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
                unavailable(ShopErrorCode::Busy)
            } else {
                unavailable(ShopErrorCode::Unavailable)
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

fn coins(amount: i64) -> Result<CoinAmount, sqlx::Error> {
    CoinAmount::new(amount).ok_or_else(|| sqlx::Error::Protocol("coin_range".to_owned()))
}

/// The balance of `user_id`, read in the caller's transaction (after its lock, for a purchase).
async fn balance(conn: &mut PgConnection, user_id: Uuid) -> Result<Option<Coins>, sqlx::Error> {
    Ok(facts::read(conn, user_id).await?.map(|f| f.balance))
}

/// Whether the level reaches the tier of an accessory (the rule of the draw); other kinds always.
fn level_ok(state: &AppState, kind: ShopItemKind, id: &str, level: u8) -> (Option<u8>, bool) {
    if kind != ShopItemKind::Accessory {
        return (None, true);
    }
    let item_tier = state.botarga.accessory_tier(id);
    let ok = matches!((item_tier, tier(level)), (Some(item), Some(top)) if item <= top);
    (item_tier, ok)
}

/// `GET /api/shop`: every item with what the user has, and the balance. Never writes.
pub async fn get(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
) -> Response {
    let quota = Quota {
        ip: &state.rate.botarga_read_ip,
        user: &state.rate.botarga_read_user,
    };
    let key = ip_key(client.as_ref());
    let user_id = match authorize(&state, &quota, &key, &headers, ShopErrorCode::Unavailable).await
    {
        Ok(info) => info.user_id,
        Err(response) => return *response,
    };
    let read = async {
        let mut conn = state.db.acquire().await?;
        let Some(view) = botarga::store::view(&mut conn, user_id).await? else {
            return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
        };
        let Some(balance) = balance(&mut conn, user_id).await? else {
            return Ok(Outcome::Gone);
        };
        let owned = store::owned(&mut conn, user_id).await?;
        let draw = store::in_draw(&mut conn, user_id).await?;
        let mut items = Vec::with_capacity(state.shop.items().len());
        for item in state.shop.items() {
            let (tier, level_ok) = level_ok(&state, item.kind, &item.id, view.progress.level);
            items.push(ShopItem {
                kind: item.kind,
                id: item.id.clone(),
                price: coins(i64::from(item.price))?,
                owned: owned
                    .iter()
                    .any(|(k, i)| k == item.kind.as_str() && *i == item.id),
                in_draw: draw.contains(&item.id),
                tier,
                level_ok,
            });
        }
        Ok(Outcome::Done(
            StatusCode::OK,
            ShopView {
                items,
                balance: coins(balance.0)?,
            },
        ))
    };
    let done = tokio::time::timeout(ACQUIRE_TIMEOUT.saturating_mul(3), read)
        .await
        .unwrap_or(Err(sqlx::Error::PoolTimedOut));
    answer(done, &state, "shop.read_failed")
}

/// `POST /api/shop/buy`: one purchase in one transaction, under the lock of the user (the same one
/// the bets take, so a bet and a purchase never spend the same coins). The balance is read after
/// that lock, in the transaction.
pub async fn buy(
    State(state): State<AppState>,
    client: Option<Extension<ClientContext>>,
    headers: HeaderMap,
    body: Result<Json<ShopBuyRequest>, JsonRejection>,
) -> Response {
    let quota = Quota {
        ip: &state.rate.shop_ip,
        user: &state.rate.shop_user,
    };
    let key = ip_key(client.as_ref());
    let user_id = match authorize(&state, &quota, &key, &headers, ShopErrorCode::Unavailable).await
    {
        Ok(info) => info.user_id,
        Err(response) => return *response,
    };
    let ip_hash = key.hash(&state.auth.ip_hash_key);
    // At the edge, before any query.
    let Ok(Json(request)) = body else {
        return invalid();
    };
    let Some(client_key) = canonical_key(&request.client_key) else {
        return invalid();
    };
    if !well_formed(&request.item_id) {
        return invalid();
    }
    let done = async {
        let mut tx = state.db.begin().await?;
        if !lock_user(&mut tx, user_id).await? {
            return Ok(Outcome::Gone);
        }
        // `FOR NO KEY UPDATE`: it waits for the XP a defense gives (`grant_xp` locks the botarga
        // `FOR UPDATE`), so a level up never opens a draw nor gives a reward of what is being
        // bought, while the foreign keys of the fights that name the botarga (`FOR KEY SHARE`)
        // never wait on it.
        sqlx::query("SELECT 1 FROM botargas WHERE user_id = $1 FOR NO KEY UPDATE")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        let Some(view) = botarga::store::view(&mut tx, user_id).await? else {
            return refusal(StatusCode::NOT_FOUND, BotargaErrorCode::NotCreated);
        };
        // Under the lock, a retry cannot race its first try.
        if let Some(done) = store::by_key(&mut tx, user_id, client_key).await? {
            if done.kind != request.kind || done.item_id != request.item_id {
                return refusal(StatusCode::CONFLICT, ShopErrorCode::KeyReused);
            }
            let Some(now) = balance(&mut tx, user_id).await? else {
                return Ok(Outcome::Gone);
            };
            let purchase = ShopPurchase {
                kind: done.kind,
                item_id: done.item_id,
                price: coins(done.price)?,
                balance: coins(now.0)?,
                botarga: view,
            };
            return Ok(Outcome::Done(StatusCode::OK, purchase));
        }
        let Some(item) = state.shop.find(request.kind, &request.item_id) else {
            return refusal(StatusCode::NOT_FOUND, ShopErrorCode::NotFound);
        };
        let owned = store::owned(&mut tx, user_id).await?;
        if owned
            .iter()
            .any(|(k, i)| k == item.kind.as_str() && *i == item.id)
        {
            return refusal(StatusCode::CONFLICT, ShopErrorCode::Owned);
        }
        if store::in_draw(&mut tx, user_id).await?.contains(&item.id) {
            return refusal(StatusCode::CONFLICT, ShopErrorCode::InDraw);
        }
        if !level_ok(&state, item.kind, &item.id, view.progress.level).1 {
            return refusal(StatusCode::UNPROCESSABLE_ENTITY, ShopErrorCode::Level);
        }
        let price = i64::from(item.price);
        if request.expected_price.get() != price {
            return refusal(StatusCode::CONFLICT, ShopErrorCode::PriceChanged);
        }
        let Some(before) = balance(&mut tx, user_id).await? else {
            return Ok(Outcome::Gone);
        };
        if before.0 < price {
            return refusal(StatusCode::PAYMENT_REQUIRED, ShopErrorCode::Insufficient);
        }
        let id = store::insert(&mut tx, user_id, item.kind, &item.id, price, client_key).await?;
        let entry = ledger::shop_purchase(&mut tx, id, user_id, Coins(price)).await?;
        audit::record(&mut tx, user_id, Event::Purchase { entry }, &ip_hash).await?;
        let Some(after) = balance(&mut tx, user_id).await? else {
            return Ok(Outcome::Gone);
        };
        let Some(botarga) = botarga::store::view(&mut tx, user_id).await? else {
            return Ok(Outcome::Gone);
        };
        tx.commit().await?;
        Ok(Outcome::Done(
            StatusCode::CREATED,
            ShopPurchase {
                kind: item.kind,
                item_id: item.id.clone(),
                price: coins(price)?,
                balance: coins(after.0)?,
                botarga,
            },
        ))
    }
    .await;
    answer(done, &state, "shop.buy_failed")
}

#[cfg(test)]
mod tests {
    use super::canonical_key;

    #[test]
    fn a_key_is_a_canonical_uuid() {
        assert!(canonical_key("0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10").is_some());
        assert!(canonical_key("0B0E8E5E-8A43-4C3C-9B55-0E6E3E1B2A10").is_none());
    }
}
