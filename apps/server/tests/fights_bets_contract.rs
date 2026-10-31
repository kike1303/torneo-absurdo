//! 010 RF17, RF29, RF30, RF32 and RF33: what TA-27 can assume, proved without a bet. Every bet that
//! saw the window open confirms before the close, no transaction sees it open afterwards, and the
//! log is computed only after the `COMMIT` of the close.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use protocol::FightState;
use server::fights::bets_contract::{WindowClosed, unsettled_in};
use server::fights::{FightSettings, FightsShared, Gate, store};
use sqlx::PgPool;
use support::db::TestDb;
use support::fights::{self as f, CADENCE, KO, T0, Vector, WINDOW, row, wait_for, wait_state};

/// Counts how many times the point before the computation of the log was reached.
#[derive(Default)]
struct CountingGate(AtomicU32);

impl Gate for CountingGate {
    fn wait(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            self.0.fetch_add(1, Ordering::SeqCst);
        })
    }
}

async fn ready(shared: &Arc<FightsShared>) {
    wait_for("the runner", || async { shared.accepts_bets() }).await;
}

async fn setup() -> (TestDb, PgPool, i32, Vector) {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    (db, pool, id, vector)
}

/// RF29, RF17: T1 holds the row; the close waits and the point before the log is not reached; T1
/// confirms; a T2 gets `WindowClosed`; and only then the log is computed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_close_waits_for_every_bet_that_saw_it_open() {
    let (_db, pool, id, _) = setup().await;
    let gate = Arc::new(CountingGate::default());
    let (settings, clock) = f::settings_at(T0 + 1);
    let settings = FightSettings {
        compute_gate: gate.clone(),
        ..settings
    };
    let (shared, _runner) = f::start(&pool, &settings);
    ready(&shared).await;

    let mut t1 = pool.begin().await.unwrap();
    let window = shared.lock_open_for_bet(&mut t1, id, T0 + 1).await.unwrap();
    assert_eq!(window.closes_at_ms, T0 + WINDOW);

    // Concurrent bets do not block each other.
    let mut other = pool.begin().await.unwrap();
    shared
        .lock_open_for_bet(&mut other, id, T0 + 1)
        .await
        .unwrap();
    other.commit().await.unwrap();

    clock.set(T0 + WINDOW);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(row(&pool, id).await.state, FightState::BettingOpen);
    assert_eq!(
        gate.0.load(Ordering::SeqCst),
        0,
        "computed before the close"
    );

    // A bet at the hour does not wait for T1, nor enter.
    let mut t2 = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut t2, id, T0 + WINDOW).await,
        Err(WindowClosed)
    );
    // It got here with T1 still open: it did not wait for it.
    t2.rollback().await.unwrap();

    t1.commit().await.unwrap();
    wait_state(&pool, id, FightState::BettingClosed).await;
    wait_for("the computation", || async {
        gate.0.load(Ordering::SeqCst) == 1
    })
    .await;

    // And after the close, nobody sees it open, whatever hour they think it is (a clock going back).
    let mut t3 = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut t3, id, T0 + 1).await,
        Err(WindowClosed)
    );
    t3.rollback().await.unwrap();
}

/// RF29: with readers chained after `closes_at`, the close confirms in a bounded time. The hour cuts
/// the chain: `lock_timeout` lets the close retry, and new readers no longer take the lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chained_readers_do_not_starve_the_close() {
    let (_db, pool, id, _) = setup().await;
    let (settings, clock) = f::settings_at(T0 + 1);
    let (shared, _runner) = f::start(&pool, &settings);
    ready(&shared).await;

    let mut holding = pool.begin().await.unwrap();
    shared
        .lock_open_for_bet(&mut holding, id, T0 + 1)
        .await
        .unwrap();
    clock.set(T0 + WINDOW);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Readers keep coming at the hour: none takes a lock.
    let chain = {
        let (pool, shared, stop) = (pool.clone(), shared.clone(), stop.clone());
        tokio::spawn(async move {
            while !stop.load(Ordering::SeqCst) {
                let mut tx = pool.begin().await.unwrap();
                assert_eq!(
                    shared.lock_open_for_bet(&mut tx, id, T0 + WINDOW).await,
                    Err(WindowClosed)
                );
                tx.rollback().await.unwrap();
            }
        })
    };
    // A wait longer than `lock_timeout`: the close retries.
    tokio::time::sleep(Duration::from_millis(700)).await;
    holding.commit().await.unwrap();
    let started = Instant::now();
    wait_state(&pool, id, FightState::BettingClosed).await;
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
    stop.store(true, Ordering::SeqCst);
    chain.await.unwrap();
}

/// RF29: during a halt, without the lock and before the first evaluation, in time and all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_bets_without_the_runner_or_during_a_halt() {
    let (_db, pool, id, vector) = setup().await;
    let (settings, _clock) = f::settings_at(T0 + 1);

    // No runner: no lock and no evaluation. It does not even touch the database.
    let shared = Arc::new(FightsShared::new(settings.clock.clone()));
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut tx, id, T0 + 1).await,
        Err(WindowClosed)
    );
    tx.rollback().await.unwrap();

    // A halt: a fight stopped for good, while another one is in its window.
    let stopped = f::open_vector(&pool, T0 - CADENCE, &vector).await;
    f::close_by_sql(&pool, stopped, T0 - CADENCE + WINDOW).await;
    f::lose_secret(&pool, stopped).await;
    let (shared, _runner) = f::start(&pool, &settings);
    wait_for("the halt", || async {
        shared.current().halted == Some(true)
    })
    .await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut tx, id, T0 + 1).await,
        Err(WindowClosed)
    );
    tx.rollback().await.unwrap();
}

/// RF33: a scheduled and a settled fight never accept a bet, so a cancelled one never has bets.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_an_open_window_accepts() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let (settings, clock) = f::settings_at(T0 - 1);
    let (shared, _runner) = f::start(&pool, &settings);
    ready(&shared).await;

    let scheduled = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut tx, scheduled, T0 - 1).await,
        Err(WindowClosed)
    );
    assert_eq!(
        shared.lock_open_for_bet(&mut tx, 999_999, T0 - 1).await,
        Err(WindowClosed)
    );
    tx.rollback().await.unwrap();

    // Cancelled: its hour never came for it. The clock past its window and back.
    clock.set(T0 + WINDOW);
    wait_state(&pool, scheduled, FightState::Settled).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut tx, scheduled, T0).await,
        Err(WindowClosed)
    );
    tx.rollback().await.unwrap();
}

/// RF29: a call on a fight that is not open takes no lock. With overlapping bets of TA-27 on a
/// scheduled fight, its opening would wait for them, and it could end up missing its window.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bet_on_a_scheduled_fight_does_not_hold_back_its_opening() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let (settings, clock) = f::settings_at(T0 - 1);
    let (shared, _runner) = f::start(&pool, &settings);
    ready(&shared).await;

    let mut early = pool.begin().await.unwrap();
    assert_eq!(
        shared.lock_open_for_bet(&mut early, id, T0 - 1).await,
        Err(WindowClosed)
    );
    // Its transaction stays open, as a bet that goes on with other work would.
    clock.set(T0);
    wait_state(&pool, id, FightState::BettingOpen).await;
    early.rollback().await.unwrap();
}

/// RF17: the proof the log needs does not exist before the close.
#[tokio::test]
async fn there_is_no_closed_window_before_the_close() {
    let (_db, pool, id, _) = setup().await;
    assert!(
        store::read_closed_window(&pool, id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(store::close(&pool, id, T0, 500).await.unwrap().is_some());
    let proof = store::read_closed_window(&pool, id).await.unwrap().unwrap();
    assert_eq!(proof.id(), id);
    // A second close finds nothing to close.
    assert!(store::close(&pool, id, T0, 500).await.unwrap().is_none());
}

/// RF32: `farm-report`'s query. RF30: `closes_at` is what places a fight in its season.
#[tokio::test]
async fn unsettled_in_lists_the_pending_fights_of_a_range() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let open = f::open_vector(&pool, T0, &vector).await;
    let scheduled = f::announce(&pool, T0 + CADENCE, &vector.a, &vector.b).await;
    let voided = f::open_vector(&pool, T0 + 2 * CADENCE, &vector).await;
    let closes = |opens: i64| opens + WINDOW;
    f::close_by_sql(&pool, voided, closes(T0 + 2 * CADENCE)).await;
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, voided, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();

    let all = (closes(T0), closes(T0 + 3 * CADENCE));
    assert_eq!(
        unsettled_in(&pool, all.0, all.1).await.unwrap(),
        [open, scheduled]
    );
    // A half-open range: `from` is in it, `to` is not.
    assert_eq!(
        unsettled_in(&pool, closes(T0), closes(T0) + 1)
            .await
            .unwrap(),
        [open]
    );
    assert!(
        unsettled_in(&pool, closes(T0) - 1, closes(T0))
            .await
            .unwrap()
            .is_empty()
    );
    // A voided one counts as settled.
    assert!(
        unsettled_in(&pool, closes(T0 + 2 * CADENCE), closes(T0 + 3 * CADENCE))
            .await
            .unwrap()
            .is_empty()
    );
}
