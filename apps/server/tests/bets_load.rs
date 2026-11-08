//! 012 RF15: the hook fits its pool. Two pools of `n` bets, sown by SQL in bulk on the same rules as
//! the API (each `bet_stake` for its bet's stake, with its triggers on): a refund of all of them, and
//! a split of both sides with 500 bp and its `rake`. Each is timed from the first write to the
//! `COMMIT`, where the deferred checks of the ledger run.
//!
//! `just check` runs 2000. The measurement of the criterion (≤ 10 s in local) is ignored:
//! `cargo test --release -p server --test bets_load -- --ignored --nocapture`, and in debug to
//! compare with phase A. Plan-b, "Medición de RF15".
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

mod support;

use std::time::{Duration, Instant};

use protocol::{EndReason, FightFinal, Side, VoidReason};
use server::bets::BetsHook;
use server::bets::api::BETS_MAX_PER_FIGHT;
use server::fights::hook::{SettlementHook, SettlingFight};
use sqlx::PgPool;
use support::bets;
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector};

/// `users` accounts with `per_user` bets of `stake` each on `fight`, half of the users on each side
/// when `both`. One statement per table.
async fn sow(pool: &PgPool, fight: i32, users: i64, per_user: i64, stake: i64, both: bool) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "CREATE TEMP TABLE sown ON COMMIT DROP AS \
         SELECT gen_random_uuid() AS id, i FROM generate_series(1, $1) AS i",
    )
    .bind(users)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO users (id, display_name, display_name_key) \
         SELECT id, 'Load' || replace(id::text, '-', ''), 'load' || replace(id::text, '-', '') \
         FROM sown",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ledger_accounts (id, kind, user_id) SELECT gen_random_uuid(), 'user', id FROM sown",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    // Today's grant of what they will stake.
    sqlx::query(
        "CREATE TEMP TABLE granted ON COMMIT DROP AS \
         SELECT gen_random_uuid() AS entry, s.id AS user_id, a.id AS account \
         FROM sown s JOIN ledger_accounts a ON a.user_id = s.id",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO ledger_entries (id, kind, user_id) SELECT entry, 'grant', user_id FROM granted")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT entry, $1, -$2::bigint FROM granted \
         UNION ALL SELECT entry, account, $2::bigint FROM granted",
    )
    .bind(server::ledger::ISSUANCE_ACCOUNT)
    .bind(per_user * stake)
    .execute(&mut *tx)
    .await
    .unwrap();
    // The bets, and one `bet_stake` each.
    sqlx::query(
        "CREATE TEMP TABLE placed ON COMMIT DROP AS \
         SELECT gen_random_uuid() AS bet, gen_random_uuid() AS entry, g.user_id, g.account, \
           CASE WHEN $1 AND s.i % 2 = 0 THEN 'b' ELSE 'a' END AS side \
         FROM granted g JOIN sown s ON s.id = g.user_id, generate_series(1, $2)",
    )
    .bind(both)
    .bind(per_user)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         SELECT bet, $1, user_id, side, $2, true, gen_random_uuid() FROM placed",
    )
    .bind(fight)
    .bind(stake)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, fight_id, bet_id) \
         SELECT entry, 'bet_stake', user_id, $1, bet FROM placed",
    )
    .bind(fight)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT entry, account, -$1::bigint FROM placed \
         UNION ALL SELECT entry, $2, $1::bigint FROM placed",
    )
    .bind(stake)
    .bind(server::ledger::ESCROW_ACCOUNT)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The hook and its `COMMIT`, timed.
async fn settle(pool: &PgPool, id: i32, final_result: FightFinal) -> Duration {
    let started = Instant::now();
    let mut tx = pool.begin().await.unwrap();
    BetsHook
        .settle(
            &mut tx,
            &SettlingFight {
                id,
                final_result,
                closes_at: T0,
            },
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    started.elapsed()
}

/// Two pools of `bets_per_pool` bets: a refund and a split. Returns the two times.
async fn measure(bets_per_pool: i64) -> (Duration, Duration) {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let users = bets_per_pool / 10;
    let refunded = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    let split = f::open_vector_with_rake(&pool, T0 + f::CADENCE, &Vector::of(KO), 500).await;
    sow(&pool, refunded, users, 10, 25, false).await;
    sow(&pool, split, users, 10, 25, true).await;
    let refund = settle(
        &pool,
        refunded,
        FightFinal::Voided {
            reason: VoidReason::EngineError,
        },
    )
    .await;
    let paid = settle(
        &pool,
        split,
        FightFinal::Decided {
            winner: Some(Side::A),
            reason: EndReason::Ko,
        },
    )
    .await;
    let settled: i64 = sqlx::query_scalar("SELECT count(*) FROM bet_settlements")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(settled, 2 * bets_per_pool);
    assert_eq!(bets::escrow(&pool).await, 0);
    assert_eq!(bets::total(&pool).await, 0);
    (refund, paid)
}

/// The size `just check` runs: it works, it conserves, and it is far under the attempt's 30 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_thousand_bets_settle_whole() {
    let (refund, paid) = measure(2000).await;
    assert!(refund < Duration::from_secs(30), "{refund:?}");
    assert!(paid < Duration::from_secs(30), "{paid:?}");
}

/// RF15's criterion: `BETS_MAX_PER_FIGHT` bets, each pool in ≤ 10 s in local.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "the measurement of RF15: run it by hand, in release"]
async fn the_largest_pool_settles_within_ten_seconds() {
    let (refund, paid) = measure(BETS_MAX_PER_FIGHT).await;
    println!(
        "{}",
        serde_json::json!({
            "bets_per_pool": BETS_MAX_PER_FIGHT,
            "refund_ms": refund.as_millis(),
            "paid_ms": paid.as_millis(),
        })
    );
    assert!(refund <= Duration::from_secs(10), "{refund:?}");
    assert!(paid <= Duration::from_secs(10), "{paid:?}");
}
