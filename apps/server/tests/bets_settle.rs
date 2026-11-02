//! 012 RF7, RF8, RF9, RF13 and RF14 in their refund part: the hook with sown bets, called directly
//! and from the runner.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use protocol::{CancelReason, EndReason, FightFinal, FightState, Side, VoidReason};
use server::bets::BetsHook;
use server::bets::settle::PAYOUT_UNAVAILABLE;
use server::fights::FightSettings;
use server::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use server::ledger::{self, Coins};
use sqlx::{PgConnection, PgPool, Row};
use support::bets::{self, written};
use support::db::{TestDb, privileged};
use support::fights::{self as f, DRAW, KO, T0, Vector, row, wait_for, wait_state};
use uuid::Uuid;

const KO_A: FightFinal = FightFinal::Decided {
    winner: Some(Side::A),
    reason: EndReason::Ko,
};
const A_DRAW: FightFinal = FightFinal::Decided {
    winner: None,
    reason: EndReason::Draw,
};
const VOIDED: FightFinal = FightFinal::Voided {
    reason: VoidReason::EngineError,
};
const CANCELLED: FightFinal = FightFinal::Cancelled {
    reason: CancelReason::MissedWindow,
};

async fn database() -> (TestDb, PgPool) {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    (db, pool)
}

async fn call(pool: &PgPool, id: i32, final_result: FightFinal) -> Result<(), SettlementError> {
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
        .await?;
    tx.commit().await.unwrap();
    Ok(())
}

/// Every bet of `bets` came back to its owner, with one entry, its result and no profit, and the
/// pool says why. A refund is not a settled bet: there is nothing but `refunded`.
async fn assert_refunded(pool: &PgPool, id: i32, placed: &[(Uuid, Uuid, i64)], code: &str) {
    for (bet, user, stake) in placed {
        let entries: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ledger_entries \
             WHERE bet_id = $1 AND kind = 'refund' AND user_id = $2 AND fight_id = $3",
        )
        .bind(bet)
        .bind(user)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(entries, 1);
        let row = sqlx::query(
            "SELECT outcome, payout, net, ranked_profit, user_id FROM bet_settlements \
             WHERE bet_id = $1",
        )
        .bind(bet)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>(0), "refunded");
        assert_eq!(row.get::<i64, _>(1), *stake);
        assert_eq!(row.get::<i64, _>(2), 0);
        assert_eq!(row.get::<i64, _>(3), 0);
        assert_eq!(row.get::<Uuid, _>(4), *user);
    }
    let row = sqlx::query(
        "SELECT outcome, rake_bp, rake, residue FROM pool_settlements WHERE fight_id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), code);
    assert_eq!(row.get::<i32, _>(1), 0);
    assert_eq!(row.get::<i64, _>(2), 0);
    assert_eq!(row.get::<i64, _>(3), 0);
    let won_or_lost: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM bet_settlements WHERE fight_id = $1 AND outcome <> 'refunded'",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(won_or_lost, 0);
    assert_eq!(bets::total(pool).await, 0);
}

/// Two users, three bets, on the sides given.
async fn sow(pool: &PgPool, id: i32, sides: [&str; 3]) -> (Uuid, Uuid, Vec<(Uuid, Uuid, i64)>) {
    let (a, b) = (
        bets::funded_user(pool, 1000).await,
        bets::funded_user(pool, 1000).await,
    );
    let mut placed = Vec::new();
    for (user, side, stake) in [(a, sides[0], 7), (a, sides[1], 5), (b, sides[2], 9)] {
        placed.push((
            bets::seed_bet(pool, id, user, side, stake).await,
            user,
            stake,
        ));
    }
    (a, b, placed)
}

/// RF9, the rows of a refund, by a direct call: 25(f) for a cancelled one, which has no real route.
#[tokio::test]
async fn each_final_that_refunds_gives_everything_back() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    for (slot, final_result, sides, code) in [
        (0_i64, KO_A, ["a", "a", "a"], "refund_one_sided"),
        (1, KO_A, ["b", "b", "b"], "refund_one_sided"),
        (2, A_DRAW, ["a", "a", "b"], "refund_draw"),
        (3, VOIDED, ["a", "a", "b"], "refund_voided"),
        (4, CANCELLED, ["a", "a", "b"], "refund_cancelled"),
    ] {
        let id = f::open_vector(&pool, T0 + slot * f::CADENCE, &vector).await;
        let (a, b, placed) = sow(&pool, id, sides).await;
        assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(988));
        assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(991));
        assert_eq!(bets::escrow(&pool).await, 21);
        call(&pool, id, final_result).await.unwrap();
        assert_refunded(&pool, id, &placed, code).await;
        assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(1000));
        assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(1000));
        assert_eq!(bets::escrow(&pool).await, 0);
    }
}

/// RF7: a decided fight with both sides is an error in phase A, and it writes nothing.
#[tokio::test]
async fn a_decided_fight_with_both_sides_has_no_split_yet() {
    let (_db, pool) = database().await;
    let id = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    sow(&pool, id, ["a", "a", "b"]).await;
    let before = written(&pool).await;
    let error = call(&pool, id, KO_A).await.unwrap_err();
    assert_eq!(error.kind, PAYOUT_UNAVAILABLE);
    assert_eq!(written(&pool).await, before);
}

/// RF8: twice, in the same transaction and after a commit, leaves the same. A fight without bets
/// writes nothing at all.
#[tokio::test]
async fn settling_twice_changes_nothing() {
    let (_db, pool) = database().await;
    let id = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    let (_, _, placed) = sow(&pool, id, ["a", "a", "b"]).await;

    let mut tx = pool.begin().await.unwrap();
    let fight = SettlingFight {
        id,
        final_result: VOIDED,
        closes_at: T0,
    };
    BetsHook.settle(&mut tx, &fight).await.unwrap();
    BetsHook.settle(&mut tx, &fight).await.unwrap();
    tx.commit().await.unwrap();
    assert_refunded(&pool, id, &placed, "refund_voided").await;
    let after = written(&pool).await;

    call(&pool, id, VOIDED).await.unwrap();
    // A later call with another final does not rewrite the first one either.
    call(&pool, id, A_DRAW).await.unwrap();
    assert_eq!(written(&pool).await, after);
    assert_refunded(&pool, id, &placed, "refund_voided").await;

    let empty = f::open_vector(&pool, T0 + f::CADENCE, &Vector::of(KO)).await;
    let before = written(&pool).await;
    call(&pool, empty, VOIDED).await.unwrap();
    assert_eq!(written(&pool).await, before);
}

/// RF14, refund part: a ban after betting changes nothing of what is written.
#[tokio::test]
async fn a_ban_after_betting_does_not_change_the_refund() {
    let (_db, pool) = database().await;
    let id = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    let (a, b, placed) = sow(&pool, id, ["a", "a", "b"]).await;
    privileged(
        &pool,
        sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1").bind(a),
    )
    .await;
    call(&pool, id, VOIDED).await.unwrap();
    assert_refunded(&pool, id, &placed, "refund_voided").await;
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(1000));
    assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(1000));
}

fn hook_settings(
    now: i64,
    hook: Arc<dyn SettlementHook>,
) -> (FightSettings, server::fights::FightClock) {
    let (settings, clock) = f::settings_at(now);
    (FightSettings { hook, ..settings }, clock)
}

/// 25(f) and the real refunds: a draw and a one-sided pool, by the runner, with the real hook.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_runner_refunds_a_real_draw_and_a_real_one_sided_pool() {
    let (_db, pool) = database().await;
    let (draw, ko) = (Vector::of(DRAW), Vector::of(KO));
    let first = f::open_vector(&pool, T0, &draw).await;
    let second = f::open_vector(&pool, T0 + 2 * f::CADENCE, &ko).await;
    let (_, _, tied) = sow(&pool, first, ["a", "a", "b"]).await;
    let (settings, clock) = hook_settings(T0, Arc::new(BetsHook));
    let (_shared, _runner) = f::start(&pool, &settings);
    f::drive_to_settled(&pool, &clock, first).await;
    assert_refunded(&pool, first, &tied, "refund_draw").await;
    assert_eq!(row(&pool, first).await.end_reason.as_deref(), Some("draw"));

    let (_, _, lonely) = sow(&pool, second, ["b", "b", "b"]).await;
    clock.set(clock.now_ms().max(T0 + 2 * f::CADENCE));
    f::drive_to_settled(&pool, &clock, second).await;
    assert_refunded(&pool, second, &lonely, "refund_one_sided").await;
    assert_eq!(bets::escrow(&pool).await, 0);
}

/// RF7: by the runner, a decided fight with both sides never settles without its entries: it retries
/// and ends in a halt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_runner_halts_on_a_decided_fight_with_both_sides() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    sow(&pool, id, ["a", "a", "b"]).await;
    let entries_before: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (settings, clock) = hook_settings(T0, Arc::new(BetsHook));
    let (shared, _runner) = f::start(&pool, &settings);
    let (closes_at, live_at) = f::hours(T0);
    clock.set(closes_at);
    wait_state(&pool, id, FightState::BettingClosed).await;
    clock.set(live_at);
    wait_state(&pool, id, FightState::Live).await;
    clock.set(live_at + vector.end_t() + 10_000);
    wait_for("the halt", || async {
        clock.advance(2000);
        shared.current().halted == Some(true)
    })
    .await;
    assert_eq!(row(&pool, id).await.state, FightState::Live);
    assert_eq!(row(&pool, id).await.revealed_seed, None);
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entries, entries_before);
    let pools: i64 = sqlx::query_scalar("SELECT count(*) FROM pool_settlements")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pools, 0);
}

/// RF7: `app::serve` builds the real hook, with no hook of a test, whatever its settings say: a
/// one-sided pool ends refunded, which `NoBets` would not do.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_settles_with_the_real_hook() {
    let (db, pool) = database().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (_, _, placed) = sow(&pool, id, ["a", "a", "a"]).await;
    let (fights, clock) = f::settings_at(T0);
    let url = db.url();
    let config = support::config::config(&[
        ("DATABASE_URL", url.as_str()),
        ("PORT", "0"),
        ("CONTENT_DIR", "/nonexistent/content/characters"),
    ]);
    let settings = server::app::Settings {
        fights,
        ..server::app::Settings::default()
    };
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        server::app::serve(&config, settings, async move {
            let _ = stopped.await;
        })
        .await
    });
    f::drive_to_settled(&pool, &clock, id).await;
    assert_refunded(&pool, id, &placed, "refund_one_sided").await;
    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();
}

/// The real hook, and then a failure before the `COMMIT`, until the test says so.
struct DiesAfterWriting {
    dying: AtomicBool,
}

impl SettlementHook for DiesAfterWriting {
    fn settle<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        Box::pin(async move {
            BetsHook.settle(&mut *tx, fight).await?;
            if self.dying.load(Ordering::SeqCst) {
                return Err(SettlementError { kind: "test" });
            }
            Ok(())
        })
    }
}

/// RF8 and RF31 of 010: a settlement that dies before its `COMMIT` leaves nothing, and the retry
/// leaves everything once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settlement_that_dies_leaves_nothing_and_the_retry_leaves_it_once() {
    let (_db, pool) = database().await;
    let vector = Vector::of(DRAW);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (a, b, placed) = sow(&pool, id, ["a", "a", "b"]).await;
    let hook = Arc::new(DiesAfterWriting {
        dying: AtomicBool::new(true),
    });
    let (settings, clock) = hook_settings(T0, hook.clone());
    let (shared, _runner) = f::start(&pool, &settings);
    let (closes_at, live_at) = f::hours(T0);
    clock.set(closes_at);
    wait_state(&pool, id, FightState::BettingClosed).await;
    clock.set(live_at);
    wait_state(&pool, id, FightState::Live).await;
    clock.set(live_at + vector.end_t() + 10_000);
    wait_for("the halt", || async {
        clock.advance(2000);
        shared.current().halted == Some(true)
    })
    .await;
    assert_eq!(bets::escrow(&pool).await, 21);
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(988));
    hook.dying.store(false, Ordering::SeqCst);
    f::wait_state_advancing(&pool, &clock, id, FightState::Settled).await;
    assert_refunded(&pool, id, &placed, "refund_draw").await;
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(1000));
    assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(1000));
}

/// One fight of the property: who bets what, and how it ends, or that it stays open.
#[derive(Debug, Clone)]
struct Round {
    bets: Vec<(usize, bool, i64)>,
    ending: u8,
}

fn rounds() -> impl Strategy<Value = Vec<Round>> {
    let bet = (0..4_usize, any::<bool>(), 1..200_i64);
    let round = (prop::collection::vec(bet, 0..12), 0..5_u8)
        .prop_map(|(bets, ending)| Round { bets, ending });
    prop::collection::vec(round, 1..8)
}

/// RF13, in its refund part (phase B adds the split): at every step the movements add up to 0 and the
/// escrow holds exactly the stakes of the fights without a settlement. After settling, what came in
/// through a fight went out through it.
#[tokio::test]
async fn the_coins_are_conserved_through_any_sequence_of_refunds() {
    let (_db, pool) = database().await;
    let users = [
        bets::funded_user(&pool, 1_000_000).await,
        bets::funded_user(&pool, 1_000_000).await,
        bets::funded_user(&pool, 1_000_000).await,
        bets::funded_user(&pool, 1_000_000).await,
    ];
    let vector = Vector::of(KO);
    let mut runner = TestRunner::deterministic();
    let mut slot = 0_i64;
    let mut open_stakes = 0_i64;
    for _ in 0..16 {
        let sequence = rounds().new_tree(&mut runner).unwrap().current();
        for round in sequence {
            slot += 1;
            let id = f::open_vector(&pool, T0 + slot * f::CADENCE, &vector).await;
            let mut staked = 0_i64;
            let sides: Vec<bool> = round.bets.iter().map(|bet| bet.1).collect();
            for (user, side, stake) in &round.bets {
                let side = if *side { "a" } else { "b" };
                bets::seed_bet(&pool, id, users[*user], side, *stake).await;
                staked += stake;
            }
            // A decided fight only with one side: both are the split of phase B.
            let final_result = match round.ending {
                0 => None,
                1 => Some(VOIDED),
                2 => Some(A_DRAW),
                3 => Some(CANCELLED),
                _ if sides.iter().all(|side| *side) => Some(KO_A),
                _ => Some(VOIDED),
            };
            open_stakes += staked;
            if let Some(final_result) = final_result {
                call(&pool, id, final_result).await.unwrap();
                open_stakes -= staked;
                let through: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(p.amount), 0)::bigint FROM ledger_postings p \
                     JOIN ledger_entries e ON e.id = p.entry_id \
                     WHERE e.fight_id = $1 AND p.account_id = $2",
                )
                .bind(id)
                .bind(ledger::ESCROW_ACCOUNT)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert_eq!(through, 0);
            }
            assert_eq!(bets::total(&pool).await, 0);
            assert_eq!(bets::escrow(&pool).await, open_stakes);
        }
    }
    assert!(slot > 16, "{slot}");
}
