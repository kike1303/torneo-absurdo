//! 010 RF10, and the security review R5 and R6: one instance acts, and a lost lock stops what starts
//! without losing what is on air.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::FightState;
use server::fights::runner::APPLICATION_NAME;
use server::fights::{FightSettings, RunnerTimings};
use sqlx::postgres::PgPoolOptions;
use support::db::TestDb;
use support::fights::{
    self as f, KO, RecordingSink, T0, Vector, WINDOW, quick_timings, row, wait_for, wait_state,
};
use support::pg_proxy::PgProxy;

/// Two runners on the same database: only one advances, and when it lets go the other takes over.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_one_runner_acts() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (settings, clock) = f::settings_at(T0 + 1);
    let (first, first_runner) = f::start(&pool, &settings);
    wait_for("the first", || async {
        first.current().halted == Some(false)
    })
    .await;
    let (second, second_runner) = f::start(&pool, &settings);
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Without the lock it says nothing and accepts nothing.
    assert_eq!(second.current().halted, None);
    assert!(second.current().current.is_none());
    assert!(!second.accepts_bets());
    assert!(first.accepts_bets());

    // One lock connection, by name.
    let holders: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a ON a.pid = l.pid \
         WHERE l.locktype = 'advisory' AND l.granted AND a.application_name = $1 \
         AND a.datname = current_database()",
    )
    .bind(APPLICATION_NAME)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(holders, 1);

    first_runner.stop().await;
    wait_for("the second", || async {
        second.current().halted == Some(false)
    })
    .await;
    assert_eq!(first.current().halted, None);
    clock.set(T0 + WINDOW);
    wait_state(&pool, id, FightState::BettingClosed).await;
    second_runner.stop().await;
}

/// RF10, DP-4 and R6: with the connection lost, nothing starts and nothing settles, an emission on
/// air goes on, and on recovering the lock there is still one emission.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lost_lock_stops_what_starts_and_keeps_what_is_on_air() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let on_air = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, on_air, closes_at).await;
    let waiting = f::open_vector(&pool, T0 + 1000, &vector).await;

    let proxy = PgProxy::start(db.server_addr()).await;
    let through = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy_with(db.options_via(proxy.addr()));
    let sink = Arc::new(RecordingSink::default());
    let (settings, clock) = f::settings_at(live_at);
    let settings = FightSettings {
        sink: sink.clone(),
        ..settings
    };
    let (shared, _runner) = f::start(&through, &settings);
    wait_state(&pool, on_air, FightState::Live).await;
    clock.set(live_at + 1000);
    wait_for("the first events", || async { !sink.events().is_empty() }).await;

    proxy.cut();
    wait_for("the lost lock", || async {
        shared.current().halted.is_none()
    })
    .await;
    assert!(!shared.accepts_bets());
    // Its hour comes with no lock: it stays, and the fight on air goes on to its end.
    clock.set(live_at + vector.end_t());
    wait_for("the end on air", || async {
        sink.events() == vector.log.events
    })
    .await;
    clock.set(live_at + 100_000);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(row(&pool, on_air).await.state, FightState::Live);

    proxy.restore();
    wait_state(&pool, on_air, FightState::Settled).await;
    // No second emission: the one on air was kept.
    assert_eq!(sink.events(), vector.log.events);
    assert_eq!(sink.starts()[0], 0);
    assert_eq!(sink.starts().iter().filter(|start| **start == 0).count(), 1);
    // What had to wait goes on, with the lock back.
    assert_ne!(row(&pool, waiting).await.state, FightState::BettingOpen);
    assert_eq!(shared.current().halted, Some(false));
}

/// R5: a hung connection, not a closed one. The backend stays alive behind the proxy, which answers
/// the TCP keepalives; even so the runner notices within its timeout, and another one takes the lock
/// in a bounded time, because the server closes a session with no heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hung_connection_loses_the_lock_in_a_bounded_time() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let through = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy_with(db.options_via(proxy.addr()));
    let idle_session_ms = 3000;
    let timings = RunnerTimings {
        heartbeat_ms: 100,
        heartbeat_timeout_ms: 1000,
        idle_session_ms,
        ..quick_timings()
    };
    let (settings, clock) = f::settings_at(T0 + 1);
    let settings = FightSettings {
        timings,
        ..settings
    };
    let (hung, _hung_runner) = f::start(&through, &settings);
    wait_for("the first", || async {
        hung.current().halted == Some(false)
    })
    .await;

    let (other, _other_runner) = f::start(&pool, &settings);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(other.current().halted, None);

    let hung_at = Instant::now();
    proxy.hang();
    // The hung runner stops saying what it does not know, in its timeout.
    wait_for("the hung runner", || async {
        hung.current().halted.is_none()
    })
    .await;
    assert!(
        hung_at.elapsed() < Duration::from_secs(30),
        "{:?}",
        hung_at.elapsed()
    );
    assert!(!hung.accepts_bets());

    // The backend is still there at this point, with the lock: nothing else freed it.
    let holders: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity \
         WHERE application_name = $1 AND datname = current_database()",
    )
    .bind(APPLICATION_NAME)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(holders >= 1);

    // Another runner takes over, in a bounded time.
    wait_for("the other runner", || async {
        other.current().halted == Some(false)
    })
    .await;
    let took = hung_at.elapsed();
    assert!(took < Duration::from_secs(45), "{took:?}");
    clock.set(T0 + WINDOW);
    wait_state(&pool, id, FightState::BettingClosed).await;
    proxy.cut();
}
