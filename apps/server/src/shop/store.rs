//! The queries of the shop (024). Every function takes the connection or the transaction of its
//! caller; the ones that write expect the lock of the user that [`super::api`] takes.

use protocol::ShopItemKind;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A purchase as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purchase {
    pub id: i32,
    pub kind: ShopItemKind,
    pub item_id: String,
    pub price: i64,
}

fn kind_of(code: &str) -> Result<ShopItemKind, sqlx::Error> {
    match code {
        "accessory" => Ok(ShopItemKind::Accessory),
        "cosmetic" => Ok(ShopItemKind::Cosmetic),
        "skin" => Ok(ShopItemKind::Skin),
        _ => Err(sqlx::Error::Protocol("shop_row".to_owned())),
    }
}

/// The purchase `user_id` already made with `client_key`.
///
/// # Errors
/// The database error, or a row the `CHECK`s forbid.
pub async fn by_key(
    tx: &mut PgConnection,
    user_id: Uuid,
    client_key: Uuid,
) -> Result<Option<Purchase>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, kind, item_id, price FROM shop_purchases \
         WHERE user_id = $1 AND client_key = $2",
    )
    .bind(user_id)
    .bind(client_key)
    .fetch_optional(tx)
    .await?;
    row.map(|row| {
        Ok(Purchase {
            id: row.try_get(0)?,
            kind: kind_of(row.try_get(1)?)?,
            item_id: row.try_get(2)?,
            price: row.try_get(3)?,
        })
    })
    .transpose()
}

/// What `user_id` owns, as `(kind, item_id)`: every unlock, bought or won.
///
/// # Errors
/// The database error.
pub async fn owned(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT kind, item_id FROM botarga_unlocks WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(conn)
        .await
}

/// The choices of the open draw of `user_id`, if it has one.
///
/// # Errors
/// The database error.
pub async fn in_draw(conn: &mut PgConnection, user_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT unnest(options) FROM botarga_draws \
         WHERE user_id = $1 AND chosen IS NULL AND options IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// Writes the purchase and its unlock, and returns its id. The caller writes its charge in the
/// same transaction; without it the commit fails (`TA016`).
///
/// # Errors
/// The database error (the unique `(user, kind, item)` and `(user, client_key)` included).
pub async fn insert(
    tx: &mut PgConnection,
    user_id: Uuid,
    kind: ShopItemKind,
    item_id: &str,
    price: i64,
    client_key: Uuid,
) -> Result<i32, sqlx::Error> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO shop_purchases (user_id, kind, item_id, price, client_key) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(user_id)
    .bind(kind.as_str())
    .bind(item_id)
    .bind(price)
    .bind(client_key)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) VALUES ($1, $2, $3, 'shop')",
    )
    .bind(user_id)
    .bind(kind.as_str())
    .bind(item_id)
    .execute(&mut *tx)
    .await?;
    Ok(id)
}
