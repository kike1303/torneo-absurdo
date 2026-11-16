//! 024 phase 1 (TA-92): the shop over HTTP and in the database. A purchase moves coins only by its
//! entry, unlocks the item and keeps the ledger whole; every refusal writes nothing; purchases at
//! once never spend more than the balance; and the database refuses a charge that does not match
//! its purchase, a purchase without a charge and the removal of what was bought.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use serde_json::Value;
use server::app::Settings;
use server::ledger::Coins;
use server::rate::{RateSettings, ServerClock};
use sqlx::PgPool;
use support::app::{ORIGIN, RunningApp};
use support::db::TestDb;
use support::http;
use uuid::Uuid;

struct App {
    app: RunningApp,
    pool: PgPool,
    clock: ServerClock,
}

impl App {
    /// A request a minute of the rate clock later: no quota in the way.
    async fn send(&self, method: &str, path: &str, token: &str, body: &str) -> Value {
        self.clock.advance(Duration::from_secs(60));
        let cookie = support::oauth::session_cookie(token);
        let headers = [
            ("origin", ORIGIN),
            ("content-type", "application/json"),
            ("cookie", cookie.as_str()),
        ];
        let response = http::send_body(self.app.addr, method, path, &headers, body).await;
        let text = response.text();
        let mut value: Value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap()
        };
        if let Value::Object(map) = &mut value {
            map.insert("_status".to_owned(), response.status.as_u16().into());
            map.insert(
                "_cache".to_owned(),
                response.header("cache-control").unwrap_or("").into(),
            );
        }
        value
    }

    async fn buy(&self, token: &str, kind: &str, id: &str, price: i64, key: &str) -> Value {
        let body = format!(
            r#"{{"kind":"{kind}","item_id":"{id}","expected_price":{price},"client_key":"{key}"}}"#
        );
        self.send("POST", "/api/shop/buy", token, &body).await
    }

    async fn shop(&self, token: &str) -> Value {
        self.send("GET", "/api/shop", token, "").await
    }
}

async fn start() -> (App, TestDb) {
    let db = TestDb::create().await;
    let clock = ServerClock::manual();
    let settings = Settings {
        rate: RateSettings {
            clock: clock.clone(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    let (app, pool) = support::app::start_db(&db, &[], &settings).await;
    (App { app, pool, clock }, db)
}

/// A user with a botarga and `coins` granted today.
async fn buyer(pool: &PgPool, coins: i64) -> (Uuid, String) {
    let (user, token) = support::app::user_with_session(pool).await;
    let catalog = server::botarga::BotargaCatalog::embedded().unwrap();
    let mut tx = pool.begin().await.unwrap();
    server::botarga::store::create(&mut tx, user, catalog.starter())
        .await
        .unwrap();
    if coins > 0 {
        server::ledger::grant(&mut tx, user, Coins(coins))
            .await
            .unwrap()
            .unwrap();
    }
    tx.commit().await.unwrap();
    (user, token)
}

fn key() -> String {
    Uuid::new_v4().to_string()
}

fn status(value: &Value) -> u64 {
    value["_status"].as_u64().unwrap()
}

fn assert_code(value: &Value, expected: u64, code: &str) {
    assert_eq!(status(value), expected, "{value}");
    assert_eq!(value["error"], code, "{value}");
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn balance(pool: &PgPool, user: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(p.amount), 0)::bigint FROM ledger_postings p \
         JOIN ledger_accounts a ON a.id = p.account_id WHERE a.user_id = $1",
    )
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// What nothing may break: every entry adds up to zero, and the shop sink holds exactly the prices
/// of the purchases.
async fn assert_ledger_whole(pool: &PgPool) {
    assert_eq!(
        count(
            pool,
            "SELECT COALESCE(sum(amount), 0)::bigint FROM ledger_postings"
        )
        .await,
        0
    );
    let sink = count(
        pool,
        "SELECT COALESCE(sum(amount), 0)::bigint FROM ledger_postings \
         WHERE account_id = '00000000-0000-0000-0000-000000000004'",
    )
    .await;
    let prices = count(
        pool,
        "SELECT COALESCE(sum(price), 0)::bigint FROM shop_purchases",
    )
    .await;
    assert_eq!(sink, prices);
}

#[tokio::test]
async fn a_purchase_moves_coins_by_its_entry_and_unlocks_the_item() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 1000).await;

    let shop = app.shop(&token).await;
    assert_eq!(status(&shop), 200);
    assert_eq!(shop["_cache"], "private, no-store");
    assert_eq!(shop["balance"], 1000);
    let idle = shop["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "idle_2")
        .unwrap()
        .clone();
    assert_eq!(idle["price"], 400);
    assert_eq!(idle["owned"], false);

    let k = key();
    let bought = app.buy(&token, "cosmetic", "idle_2", 400, &k).await;
    assert_eq!(status(&bought), 201, "{bought}");
    assert_eq!(bought["balance"], 600);
    assert_eq!(bought["price"], 400);
    assert!(
        bought["botarga"]["unlocks"]["cosmetics"]
            .as_array()
            .unwrap()
            .contains(&Value::from("idle_2"))
    );
    assert_eq!(balance(pool, user).await, 600);
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM botarga_unlocks WHERE source = 'shop' AND item_id = 'idle_2'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM auth_events WHERE kind = 'purchase'"
        )
        .await,
        1
    );
    assert_ledger_whole(pool).await;

    // The same key is the same purchase; with another item it is refused.
    let again = app.buy(&token, "cosmetic", "idle_2", 400, &k).await;
    assert_eq!(status(&again), 200);
    assert_eq!(again["balance"], 600);
    assert_code(
        &app.buy(&token, "cosmetic", "idle_3", 400, &k).await,
        409,
        "shop.key_reused",
    );
    // Owned now: refused, and nothing written.
    assert_code(
        &app.buy(&token, "cosmetic", "idle_2", 400, &key()).await,
        409,
        "shop.owned",
    );
    assert_eq!(count(pool, "SELECT count(*) FROM shop_purchases").await, 1);
    let shop = app.shop(&token).await;
    let idle = shop["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "idle_2")
        .unwrap()
        .clone();
    assert_eq!(idle["owned"], true);
    assert_ledger_whole(pool).await;
}

#[tokio::test]
async fn every_refusal_has_its_code_and_writes_nothing() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 500).await;

    // Not enough coins for a victory of 750.
    assert_code(
        &app.buy(&token, "cosmetic", "win_1", 750, &key()).await,
        402,
        "shop.insufficient",
    );
    // A tier 2 accessory at level 1.
    assert_code(
        &app.buy(&token, "accessory", "tinfoil_crown", 1000, &key())
            .await,
        422,
        "shop.level",
    );
    // A price the client did not see.
    assert_code(
        &app.buy(&token, "cosmetic", "idle_3", 1, &key()).await,
        409,
        "shop.price_changed",
    );
    // Not in the catalog, of a well formed id.
    assert_code(
        &app.buy(&token, "skin", "nobody", 5000, &key()).await,
        404,
        "shop.not_found",
    );
    // Out of form at the edge.
    for body in [
        r#"{"kind":"cosmetic","item_id":"idle_2","expected_price":400}"#.to_owned(),
        format!(
            r#"{{"kind":"cosmetic","item_id":"Idle","expected_price":400,"client_key":"{}"}}"#,
            key()
        ),
        r#"{"kind":"cosmetic","item_id":"idle_2","expected_price":400,"client_key":"nope"}"#
            .to_owned(),
        format!(
            r#"{{"kind":"ability","item_id":"idle_2","expected_price":400,"client_key":"{}"}}"#,
            key()
        ),
    ] {
        assert_code(
            &app.send("POST", "/api/shop/buy", &token, &body).await,
            422,
            "shop.invalid_request",
        );
    }
    // A choice of the open draw is had by choosing it there.
    sqlx::query(
        "INSERT INTO botarga_draws (user_id, level, kind, options) \
         VALUES ($1, 5, 'cosmetic', ARRAY['idle_3', 'idle_4'])",
    )
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
    assert_code(
        &app.buy(&token, "cosmetic", "idle_3", 400, &key()).await,
        409,
        "shop.in_draw",
    );
    let shop = app.shop(&token).await;
    let item = |id: &str| {
        shop["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(item("idle_3")["in_draw"], true);
    assert_eq!(item("tinfoil_crown")["level_ok"], false);
    assert_eq!(item("tinfoil_crown")["tier"], 2);
    assert_eq!(item("oven_mitts")["level_ok"], true);

    assert_eq!(count(pool, "SELECT count(*) FROM shop_purchases").await, 0);
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM ledger_entries WHERE kind = 'shop_purchase'"
        )
        .await,
        0
    );
    assert_eq!(balance(pool, user).await, 500);

    // Without a botarga there is no shop.
    let (_, bare) = support::app::user_with_session(pool).await;
    assert_code(&app.shop(&bare).await, 404, "botarga.not_created");
    assert_code(
        &app.buy(&bare, "cosmetic", "idle_2", 400, &key()).await,
        404,
        "botarga.not_created",
    );
}

#[tokio::test]
async fn purchases_at_once_never_spend_more_than_the_balance() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    // Three items of 750 (as many as the burst of the quota lets through at once) and 1000
    // coins: only one fits.
    let (user, token) = buyer(pool, 1000).await;
    let items = [
        ("cosmetic", "win_1"),
        ("accessory", "oven_mitts"),
        ("accessory", "bunny_slippers"),
    ];
    let buys = items.iter().map(|(kind, id)| {
        let app = &app;
        let token = token.clone();
        async move { app.buy(&token, kind, id, 750, &key()).await }
    });
    let answers = futures_util::future::join_all(buys).await;
    let mut created = 0;
    for answer in &answers {
        match status(answer) {
            201 => created += 1,
            402 => assert_eq!(answer["error"], "shop.insufficient"),
            other => panic!("{other}: {answer}"),
        }
    }
    assert_eq!(created, 1, "{answers:?}");
    // All three reached the lock: the other two were refused for the balance, not by a quota.
    let short = answers.iter().filter(|a| status(a) == 402).count();
    assert_eq!(short, 2, "{answers:?}");
    let left = balance(pool, user).await;
    assert_eq!(left, 1000 - 750 * created);
    assert_eq!(
        count(pool, "SELECT count(*) FROM shop_purchases").await,
        created
    );
    assert_ledger_whole(pool).await;
}

#[tokio::test]
async fn a_purchase_waits_on_the_same_lock_as_a_bet() {
    // A bet holds the row of the user while it spends; a purchase must wait for it and read the
    // balance after, not before. Hold that row, send the purchase, see it wait, then spend.
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 500).await;
    let mut hold = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(user)
        .execute(&mut *hold)
        .await
        .unwrap();
    let release = async {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(900);
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting == 1 {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "{waiting} waiting");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // While the purchase waits, the coins go elsewhere (as a stake would): a purchase of
        // another item, written straight in this transaction.
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO shop_purchases (user_id, kind, item_id, price, client_key) \
             VALUES ($1, 'cosmetic', 'idle_5', 400, $2) RETURNING id",
        )
        .bind(user)
        .bind(Uuid::new_v4())
        .fetch_one(&mut *hold)
        .await
        .unwrap();
        server::ledger::shop_purchase(&mut hold, id, user, Coins(400))
            .await
            .unwrap();
        hold.commit().await.unwrap();
    };
    let k = key();
    let (answer, ()) = tokio::join!(app.buy(&token, "cosmetic", "idle_2", 400, &k), release);
    assert_code(&answer, 402, "shop.insufficient");
    assert_eq!(balance(pool, user).await, 100);
    assert_ledger_whole(pool).await;
}

#[tokio::test]
async fn the_database_refuses_a_charge_that_does_not_match_and_a_free_purchase() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 1000).await;
    let bought = app.buy(&token, "cosmetic", "idle_2", 400, &key()).await;
    assert_eq!(status(&bought), 201);

    let purchase = |item: &'static str| async move {
        let mut tx = pool.begin().await.unwrap();
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO shop_purchases (user_id, kind, item_id, price, client_key) \
             VALUES ($1, 'cosmetic', $2, 400, $3) RETURNING id",
        )
        .bind(user)
        .bind(item)
        .bind(Uuid::new_v4())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        (tx, id)
    };
    // Inverted signs: the sink never pays.
    let (mut tx, id) = purchase("idle_3").await;
    let error = charge(&mut tx, user, id, 400, -400).await.unwrap_err();
    assert_eq!(server::db::error_kind(&error), "ledger_posting_rule");
    drop(tx);
    // An amount that is not the price.
    let (mut tx, id) = purchase("idle_3").await;
    let error = charge(&mut tx, user, id, -1, 1).await.unwrap_err();
    assert_eq!(server::db::error_kind(&error), "ledger_posting_rule");
    drop(tx);
    // A purchase without its charge fails at commit.
    let (tx, _) = purchase("idle_3").await;
    let error = tx.commit().await.unwrap_err();
    assert_eq!(server::db::error_kind(&error), "shop_uncharged");

    // What was bought is never changed nor removed.
    for sql in [
        "UPDATE shop_purchases SET price = 1",
        "DELETE FROM shop_purchases",
        "DELETE FROM botarga_unlocks WHERE source = 'shop'",
        "UPDATE botarga_unlocks SET item_id = 'idle_4' WHERE source = 'shop'",
    ] {
        let error = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .unwrap_err();
        assert_eq!(server::db::error_kind(&error), "shop_guard", "{sql}");
    }
    assert_eq!(balance(pool, user).await, 600);
    assert_ledger_whole(pool).await;
}

/// A `shop_purchase` entry of purchase `id`, with these two amounts for the user and the sink.
async fn charge(
    tx: &mut sqlx::PgConnection,
    user: Uuid,
    id: i32,
    user_amount: i64,
    sink_amount: i64,
) -> Result<(), sqlx::Error> {
    let entry = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, shop_purchase_id) \
         VALUES ($1, 'shop_purchase', $2, $3)",
    )
    .bind(entry)
    .bind(user)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT $1, '00000000-0000-0000-0000-000000000004'::uuid, $2 \
         UNION ALL SELECT $1, id, $3 FROM ledger_accounts WHERE user_id = $4",
    )
    .bind(entry)
    .bind(sink_amount)
    .bind(user_amount)
    .bind(user)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[tokio::test]
async fn retries_at_once_with_one_key_charge_once() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 1000).await;
    let k = key();
    let (one, two) = tokio::join!(
        app.buy(&token, "cosmetic", "idle_2", 400, &k),
        app.buy(&token, "cosmetic", "idle_2", 400, &k),
    );
    let mut statuses = [status(&one), status(&two)];
    statuses.sort_unstable();
    assert!(
        statuses == [200, 201] || statuses == [201, 503],
        "{one} {two}"
    );
    assert_eq!(count(pool, "SELECT count(*) FROM shop_purchases").await, 1);
    assert_eq!(balance(pool, user).await, 600);
    assert_ledger_whole(pool).await;
}

#[tokio::test]
async fn the_database_keeps_every_other_rule_of_a_purchase() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 1000).await;
    let (other, _) = buyer(pool, 1000).await;
    let bought = app.buy(&token, "cosmetic", "idle_2", 400, &key()).await;
    assert_eq!(status(&bought), 201);
    let id: i32 = sqlx::query_scalar("SELECT id FROM shop_purchases")
        .fetch_one(pool)
        .await
        .unwrap();

    // A second charge of the same purchase.
    let mut tx = pool.begin().await.unwrap();
    assert!(charge(&mut tx, user, id, -400, 400).await.is_err());
    drop(tx);
    // A charge of it in the name of another user.
    let mut tx = pool.begin().await.unwrap();
    assert!(charge(&mut tx, other, id, -400, 400).await.is_err());
    drop(tx);
    // A grant that pays the sink.
    let mut tx = pool.begin().await.unwrap();
    let entry = Uuid::new_v4();
    sqlx::query("INSERT INTO ledger_entries (id, kind, user_id) VALUES ($1, 'bailout', $2)")
        .bind(entry)
        .bind(other)
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES \
         ($1, '00000000-0000-0000-0000-000000000001', -5), \
         ($1, '00000000-0000-0000-0000-000000000004', 5)",
    )
    .bind(entry)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(server::db::error_kind(&error), "ledger_posting_rule");
    drop(tx);
    // An unlock of the shop without its purchase fails at commit.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) \
         VALUES ($1, 'cosmetic', 'win_3', 'shop')",
    )
    .bind(other)
    .execute(&mut *tx)
    .await
    .unwrap();
    let error = tx.commit().await.unwrap_err();
    assert_eq!(server::db::error_kind(&error), "shop_uncharged");
    // Nothing bought is truncated.
    for sql in [
        "TRUNCATE shop_purchases CASCADE",
        "TRUNCATE botarga_unlocks",
    ] {
        let error = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .unwrap_err();
        assert_eq!(server::db::error_kind(&error), "shop_guard", "{sql}");
    }
    assert_eq!(balance(pool, user).await, 600);
    assert_eq!(balance(pool, other).await, 1000);
    assert_ledger_whole(pool).await;
}

#[tokio::test]
async fn one_item_bought_twice_at_once_is_bought_once_and_a_won_one_is_owned() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (user, token) = buyer(pool, 1000).await;
    let (k1, k2) = (key(), key());
    let (one, two) = tokio::join!(
        app.buy(&token, "cosmetic", "idle_2", 400, &k1),
        app.buy(&token, "cosmetic", "idle_2", 400, &k2),
    );
    let mut statuses = [status(&one), status(&two)];
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 409], "{one} {two}");
    let refused = if status(&one) == 409 { &one } else { &two };
    assert_code(refused, 409, "shop.owned");
    assert_eq!(count(pool, "SELECT count(*) FROM shop_purchases").await, 1);
    assert_eq!(balance(pool, user).await, 600);

    // Won by level, not bought: owned all the same.
    sqlx::query(
        "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) \
         VALUES ($1, 'cosmetic', 'idle_4', 'level')",
    )
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
    assert_code(
        &app.buy(&token, "cosmetic", "idle_4", 400, &key()).await,
        409,
        "shop.owned",
    );
    assert_ledger_whole(pool).await;
}
