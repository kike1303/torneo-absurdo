//! Bets for the tests of 012, phase A: there is no API yet, so a test sows them on the same route the
//! API of phase B will take, `ledger::bet_stake`, and compares what was written.

use server::ledger::{self, Coins};
use sqlx::PgPool;
use uuid::Uuid;

/// A user with its ledger account, as its first login creates them.
pub async fn user(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO users (id, display_name, display_name_key) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Test{}", id.simple()))
        .bind(format!("test{}", id.simple()))
        .execute(&mut *tx)
        .await
        .unwrap();
    ledger::create_user_account(&mut tx, id).await.unwrap();
    tx.commit().await.unwrap();
    id
}

/// A user with today's grant of `coins`.
pub async fn funded_user(pool: &PgPool, coins: i64) -> Uuid {
    let id = user(pool).await;
    let mut tx = pool.begin().await.unwrap();
    ledger::grant(&mut tx, id, Coins(coins)).await.unwrap();
    tx.commit().await.unwrap();
    id
}

/// A bet with its stake, in one transaction. The fight must be open: the database says so.
pub async fn seed_bet(pool: &PgPool, fight: i32, user: Uuid, side: &str, stake: i64) -> Uuid {
    let bet = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         VALUES ($1, $2, $3, $4, $5, true, $6)",
    )
    .bind(bet)
    .bind(fight)
    .bind(user)
    .bind(side)
    .bind(stake)
    .bind(Uuid::new_v4())
    .execute(&mut *tx)
    .await
    .unwrap();
    ledger::bet_stake(&mut tx, bet, fight, user, Coins(stake))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    bet
}

/// Everything a settlement or a void can write, to compare it before and after.
#[derive(Debug, PartialEq, Eq)]
pub struct Written {
    fights: Vec<String>,
    secrets: i64,
    entries: i64,
    postings: i64,
    bets: i64,
    bet_settlements: i64,
    pool_settlements: i64,
    audit: i64,
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

pub async fn written(pool: &PgPool) -> Written {
    Written {
        fights: sqlx::query_scalar(
            "SELECT concat_ws('|', id, state, void_reason, settled_at, encode(revealed_seed, 'hex')) \
             FROM fights ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap(),
        secrets: count(pool, "SELECT count(*) FROM fight_secrets").await,
        entries: count(pool, "SELECT count(*) FROM ledger_entries").await,
        postings: count(pool, "SELECT count(*) FROM ledger_postings").await,
        bets: count(pool, "SELECT count(*) FROM bets").await,
        bet_settlements: count(pool, "SELECT count(*) FROM bet_settlements").await,
        pool_settlements: count(pool, "SELECT count(*) FROM pool_settlements").await,
        audit: count(pool, "SELECT count(*) FROM admin_audit").await,
    }
}

/// The balance of the escrow account.
pub async fn escrow(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0)::bigint FROM ledger_postings WHERE account_id = $1",
    )
    .bind(ledger::ESCROW_ACCOUNT)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The sum of every posting: always 0.
pub async fn total(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0)::bigint FROM ledger_postings")
        .fetch_one(pool)
        .await
        .unwrap()
}
