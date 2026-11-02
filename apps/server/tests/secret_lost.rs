//! 012 RF19 and RF20: the deadline of `secret_lost` to the millisecond, by the guard and by the command
//! on the same connection, and the rest of what the database asks of this final. All of it inside one
//! transaction, where `now()` holds still: no waits and no margins.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use protocol::{FightState, VoidReason};
use server::bets::BetsHook;
use server::fights::sealed::{self, RealCompute, VoidError, VoidRefusal, VoidRequest};
use server::fights::{SECRET_LOST_MARGIN_MS, SETTLE_DELAY_MS};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};
use support::db::TestDb;
use support::fights::{self as f, secret_lost_fixture};

const STATES: [FightState; 2] = [FightState::BettingClosed, FightState::Live];
const TO_LOST: &str = "state = 'settled', settled_at = now(), void_reason = 'secret_lost'";

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
}

/// A statement that runs in a savepoint, so a refusal does not lose the transaction with its `now()`.
async fn attempt(tx: &mut PgConnection, set: &str, id: i32) -> Result<u64, sqlx::Error> {
    let mut savepoint = tx.begin().await?;
    let sql = format!("UPDATE fights SET {set} WHERE id = $1");
    match sqlx::query(AssertSqlSafe(sql))
        .bind(id)
        .execute(&mut *savepoint)
        .await
    {
        Ok(done) => {
            savepoint.commit().await?;
            Ok(done.rows_affected())
        }
        Err(error) => {
            savepoint.rollback().await?;
            Err(error)
        }
    }
}

async fn refused(tx: &mut PgConnection, set: &str, id: i32) {
    let error = attempt(tx, set, id).await.expect_err(set);
    assert_eq!(sqlstate(&error).as_deref(), Some("TA006"), "{set}");
}

async fn database() -> (TestDb, PgPool) {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    (db, pool)
}

/// RF19 by the guard: 1 ms before the deadline it refuses, and at it, it accepts. In `betting_closed`
/// `live_at` rules; in `live`, `live_started_at`, with a `live_at` that would have let it through.
#[tokio::test]
async fn the_guard_holds_until_the_deadline() {
    let (_db, pool) = database().await;
    let mut tx = pool.begin().await.unwrap();
    for (slot, state) in (0..).zip(STATES) {
        let before = secret_lost_fixture(&mut tx, slot * 2, state, 1, true).await;
        refused(&mut tx, TO_LOST, before).await;
        let at = secret_lost_fixture(&mut tx, slot * 2 + 1, state, 0, true).await;
        assert_eq!(attempt(&mut tx, TO_LOST, at).await.unwrap(), 1, "{state:?}");
    }
    tx.rollback().await.unwrap();
}

async fn void_on(
    tx: &mut PgConnection,
    id: i32,
    backup_checked: bool,
) -> Result<VoidReason, VoidError> {
    let request = VoidRequest {
        id,
        previous_binary: false,
        backup_checked,
    };
    sealed::void_fight(tx, &request, Arc::new(RealCompute), &BetsHook).await
}

/// RF19 by the command, on the same connection and so with the same `now()`: the two cannot disagree.
#[tokio::test]
async fn the_command_holds_until_the_deadline() {
    let (_db, pool) = database().await;
    let mut tx = pool.begin().await.unwrap();
    for (slot, state) in (0..).zip(STATES) {
        let before = secret_lost_fixture(&mut tx, slot * 2, state, 1, true).await;
        assert!(matches!(
            void_on(&mut tx, before, true).await,
            Err(VoidError::Refused(VoidRefusal::EmissionPossible))
        ));
        let at = secret_lost_fixture(&mut tx, slot * 2 + 1, state, 0, true).await;
        assert!(matches!(
            void_on(&mut tx, at, false).await,
            Err(VoidError::Refused(VoidRefusal::BackupNotChecked))
        ));
        assert!(matches!(
            void_on(&mut tx, at, true).await,
            Ok(VoidReason::SecretLost)
        ));
    }
    tx.rollback().await.unwrap();
}

/// RF20: with the row of the secret there is no `secret_lost`; nor with a seed.
#[tokio::test]
async fn secret_lost_is_only_without_the_row_and_without_a_seed() {
    let (_db, pool) = database().await;
    let mut tx = pool.begin().await.unwrap();
    let present = secret_lost_fixture(&mut tx, 0, FightState::Live, -1000, false).await;
    refused(&mut tx, TO_LOST, present).await;
    let lost = secret_lost_fixture(&mut tx, 1, FightState::Live, -1000, true).await;
    let seed = hex_of_seed();
    refused(
        &mut tx,
        &format!("{TO_LOST}, revealed_seed = '\\x{seed}'"),
        lost,
    )
    .await;
    assert_eq!(attempt(&mut tx, TO_LOST, lost).await.unwrap(), 1);
    tx.rollback().await.unwrap();
}

fn hex_of_seed() -> String {
    server::fights::hex(&f::Vector::of(f::KO).seed)
}

/// RF20: only the lines of the reveal are conditioned: all the rest of this edge rules it too.
#[tokio::test]
async fn the_rest_of_the_guard_rules_secret_lost_too() {
    let (_db, pool) = database().await;
    let mut tx = pool.begin().await.unwrap();
    for (slot, state) in (0..).zip(STATES) {
        let id = secret_lost_fixture(&mut tx, slot, state, -1000, true).await;
        for extra in [
            "log = '\\x7b7d'",
            "end_reason = 'ko', winner = 'a'",
            "end_reason = 'draw'",
            "winner = 'a'",
            "cancel_reason = 'missed_window'",
            "live_started_at = now() - interval '1 hour'",
        ] {
            refused(&mut tx, &format!("{TO_LOST}, {extra}"), id).await;
        }
        refused(
            &mut tx,
            "state = 'settled', void_reason = 'secret_lost'",
            id,
        )
        .await;
    }
    tx.rollback().await.unwrap();
}

/// RF20: any other reason without a seed fails as before, and a settled row is not reopened.
#[tokio::test]
async fn nothing_else_goes_without_a_seed_and_nothing_reopens() {
    let (_db, pool) = database().await;
    let mut tx = pool.begin().await.unwrap();
    let id = secret_lost_fixture(&mut tx, 0, FightState::Live, -1000, true).await;
    for reason in ["engine_error", "snapshot_invalid"] {
        refused(
            &mut tx,
            &format!("state = 'settled', settled_at = now(), void_reason = '{reason}'"),
            id,
        )
        .await;
    }
    // A decided fight, without a seed, with its secret lost.
    refused(
        &mut tx,
        "state = 'settled', settled_at = now(), winner = 'a', end_reason = 'ko', log = '\\x7b7d'",
        id,
    )
    .await;
    assert_eq!(attempt(&mut tx, TO_LOST, id).await.unwrap(), 1);
    let seed = hex_of_seed();
    for set in [
        "void_reason = 'engine_error'".to_owned(),
        format!("revealed_seed = '\\x{seed}'"),
        "settled_at = now() + interval '1 hour'".to_owned(),
    ] {
        refused(&mut tx, &set, id).await;
    }
    tx.rollback().await.unwrap();
}

/// The `CHECK`s are the second barrier: with the guard off, the only form without a seed is this one,
/// and a final without a void reason never gets through (a `NULL` would, with `=`).
#[tokio::test]
async fn the_checks_are_the_second_barrier() {
    let (_db, pool) = database().await;
    let mut tx = pool.begin().await.unwrap();
    let id = secret_lost_fixture(&mut tx, 0, FightState::Live, 3_600_000, false).await;
    sqlx::query("ALTER TABLE fights DISABLE TRIGGER fights_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    let constraint = |error: sqlx::Error| {
        error
            .as_database_error()
            .and_then(|e| e.constraint())
            .map(str::to_owned)
    };
    let seed = hex_of_seed();
    let cases = [
        (
            "state = 'settled', settled_at = now(), winner = 'a', end_reason = 'ko', log = '\\x7b7d'"
                .to_owned(),
            "fights_reveal_ck",
        ),
        (
            "state = 'settled', settled_at = now(), void_reason = 'engine_error'".to_owned(),
            "fights_reveal_ck",
        ),
        (
            format!(
                "state = 'settled', settled_at = now(), void_reason = 'other', \
                 revealed_seed = '\\x{seed}'"
            ),
            "fights_void_reason_ck",
        ),
    ];
    for (set, name) in &cases {
        let error = attempt(&mut tx, set, id).await.expect_err(set);
        assert_eq!(constraint(error).as_deref(), Some(*name), "{set}");
    }
    tx.rollback().await.unwrap();
}

/// RF19: the deadline of the migration is the one of the code, and 215 s with the bounds by default.
#[test]
fn the_constants_of_the_migration_are_the_ones_of_rust() {
    let migration = include_str!("../migrations/20261102000100_void_secret_lost.sql");
    let sum = format!("(max_ms + {SETTLE_DELAY_MS} + {SECRET_LOST_MARGIN_MS})");
    assert!(migration.contains(&sum), "{sum}");
    let ruleset = engine::Ruleset::for_version(1).unwrap();
    let max = ruleset.time_limit_ms() + ruleset.end_ms();
    assert_eq!(max + SETTLE_DELAY_MS + SECRET_LOST_MARGIN_MS, 215_000);
}
