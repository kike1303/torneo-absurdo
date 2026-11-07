//! 012 RF13 in production: the custody query of `apps/server/sql/custody.sql`, the file the README
//! sends operators to,
//! is clean on a healthy database after refunds and splits, and points at a refund of more and at a
//! bet without its stake, written with direct SQL.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use protocol::{EndReason, FightFinal, Side, VoidReason};
use server::bets::BetsHook;
use server::fights::hook::{SettlementHook, SettlingFight};
use server::ledger::ESCROW_ACCOUNT;
use sqlx::{PgPool, Row};
use support::bets;
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector};
use uuid::Uuid;

/// The query the README sends operators to.
fn custody_query() -> String {
    include_str!("../sql/custody.sql").to_owned()
}

/// `(escrow, open_stakes, all_movements, bets_without_stake, wrong_refunds)`.
async fn custody(pool: &PgPool) -> (i64, i64, i64, i64, i64) {
    let row = sqlx::query(sqlx::AssertSqlSafe(custody_query()))
        .fetch_one(pool)
        .await
        .unwrap();
    (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4))
}

async fn settle(pool: &PgPool, id: i32, final_result: FightFinal) {
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
}

#[tokio::test]
async fn the_query_of_the_readme_finds_what_breaks_the_custody() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    assert_eq!(custody(&pool).await, (0, 0, 0, 0, 0));
    // An open pool, a refunded one and a split one.
    let open = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    let refunded = f::open_vector(&pool, T0 + f::CADENCE, &Vector::of(KO)).await;
    let split = f::open_vector_with_rake(&pool, T0 + 2 * f::CADENCE, &Vector::of(KO), 500).await;
    let (a, b) = (
        bets::funded_user(&pool, 1000).await,
        bets::funded_user(&pool, 1000).await,
    );
    for id in [open, refunded, split] {
        bets::seed_bet(&pool, id, a, "a", 30).await;
        bets::seed_bet(&pool, id, b, "b", 20).await;
    }
    assert_eq!(custody(&pool).await, (150, 150, 0, 0, 0));
    settle(
        &pool,
        refunded,
        FightFinal::Voided {
            reason: VoidReason::EngineError,
        },
    )
    .await;
    settle(
        &pool,
        split,
        FightFinal::Decided {
            winner: Some(Side::A),
            reason: EndReason::Ko,
        },
    )
    .await;
    // The hook alone does not move a fight to `settled` (the runner does, in the same transaction),
    // so `open_stakes` still counts the three here; what the entries say is what matters: the escrow
    // holds only the 50 of `open`, and nothing else is off.
    let (escrow, open_stakes, all, without, wrong) = custody(&pool).await;
    assert_eq!((escrow, all, without, wrong), (50, 0, 0, 0));
    assert_eq!(open_stakes, 150);

    // A refund of more on the open pool, with direct SQL: the database allows it (area of concern
    // 21), and the query points at it.
    let bet: Uuid = sqlx::query_scalar("SELECT id FROM bets WHERE fight_id = $1 AND user_id = $2")
        .bind(open)
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let entry = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, fight_id, bet_id) \
         VALUES ($1, 'refund', $2, $3, $4)",
    )
    .bind(entry)
    .bind(a)
    .bind(open)
    .bind(bet)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ledger_postings (entry_id, account_id, amount) \
         SELECT $1, $2, -1000 UNION ALL SELECT $1, id, 1000 FROM ledger_accounts WHERE user_id = $3",
    )
    .bind(entry)
    .bind(ESCROW_ACCOUNT)
    .bind(a)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (escrow, _, all, without, wrong) = custody(&pool).await;
    assert_eq!((escrow, all, without, wrong), (50 - 1000, 0, 0, 1));

    // A bet without its stake.
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         VALUES ($1, $2, $3, 'b', 25, true, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(open)
    .bind(b)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(custody(&pool).await.3, 1);
}
