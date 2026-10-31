//! 010 RF40, and the security review R4 and R7: each event at `live_started_at + t`, with no lead,
//! never waiting for the database.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use engine::FightEvent;
use protocol::FightState;
use server::fights::FightSettings;
use server::fights::release::EventSink;
use sqlx::postgres::PgPoolOptions;
use support::db::TestDb;
use support::fights::{self as f, KO, RecordingSink, T0, Vector, row, wait_for, wait_state};
use support::pg_proxy::PgProxy;

/// Enough for several ticks of 50 ms: what did not arrive by now, was not sent.
const SEVERAL_TICKS: Duration = Duration::from_millis(250);

/// In `t - 1` it did not arrive; in `t` it does, in the next tick. `ko` and `end` included: at their
/// hour, not a tick before, and each exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_event_comes_at_its_time_and_not_before() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    let sink = Arc::new(RecordingSink::default());
    let (settings, clock) = f::settings_at(live_at);
    let settings = FightSettings {
        sink: sink.clone(),
        ..settings
    };
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Live).await;

    let ko_t = vector
        .log
        .events
        .iter()
        .find(|event| matches!(event, FightEvent::Ko { .. }))
        .expect("a KO vector")
        .t();
    let end_t = vector.log.events.last().unwrap().t();
    let mut checked = 0;
    for t in [ko_t, end_t] {
        clock.set(live_at + i64::from(t) - 1);
        tokio::time::sleep(SEVERAL_TICKS).await;
        let released = sink.events();
        assert!(released.iter().all(|event| event.t() < t), "{t}");
        // Everything before it did arrive, in order.
        let due: Vec<_> = vector
            .log
            .events
            .iter()
            .filter(|e| e.t() < t)
            .cloned()
            .collect();
        assert_eq!(released, due, "{t}");

        clock.set(live_at + i64::from(t));
        wait_for("the event", || async {
            sink.events().iter().any(|event| event.t() == t)
        })
        .await;
        checked += 1;
    }
    assert_eq!(checked, 2);
    assert_eq!(sink.events(), vector.log.events);
    // The index of each batch is the place of its first event.
    let batches = sink.batches.lock().unwrap().clone();
    let mut next = 0;
    for (fight, from, events) in batches {
        assert_eq!(fight, id);
        assert_eq!(from, next);
        next += events.len();
    }
    // Settled only at its `settle_at`, 5 s after `end`.
    clock.set(live_at + i64::from(end_t) + 4999);
    tokio::time::sleep(SEVERAL_TICKS).await;
    assert_eq!(row(&pool, id).await.state, FightState::Live);
    clock.set(live_at + i64::from(end_t) + 5000);
    wait_state(&pool, id, FightState::Settled).await;
}

/// R4: with `live_started_at` in the future nothing is released, not even `intro`, whose `t` is 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_that_has_not_started_releases_nothing() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    // The late route: state `live`, and its first event 10 s from now.
    let late = live_at + 5000;
    let sink = Arc::new(RecordingSink::default());
    let (settings, clock) = f::settings_at(late);
    let settings = FightSettings {
        sink: sink.clone(),
        ..settings
    };
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Live).await;
    let started = row(&pool, id).await.live_started_at.unwrap();
    assert_eq!(started, late + 10_000);
    tokio::time::sleep(SEVERAL_TICKS).await;
    clock.set(started - 1);
    tokio::time::sleep(SEVERAL_TICKS).await;
    assert!(sink.events().is_empty(), "{:?}", sink.events());
    clock.set(started);
    wait_for("intro", || async { !sink.events().is_empty() }).await;
    assert!(matches!(sink.events()[0], FightEvent::Intro { t: 0 }));
}

/// RF40: a database that is down stops the settlement, never the events.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_database_down_events_go_on_and_nothing_settles() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;

    // The runner only sees the database through the proxy.
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
    wait_state(&pool, id, FightState::Live).await;
    clock.set(live_at + 1000);
    wait_for("the first events", || async { !sink.events().is_empty() }).await;

    proxy.cut();
    wait_for("the lost lock", || async {
        shared.current().halted.is_none()
    })
    .await;
    clock.set(live_at + vector.end_t());
    wait_for("the end", || async { sink.events() == vector.log.events }).await;
    clock.set(live_at + 100_000);
    tokio::time::sleep(SEVERAL_TICKS).await;
    assert_eq!(row(&pool, id).await.state, FightState::Live);
    assert_eq!(f::secrets_of(&pool, id).await, 1);

    // It comes back: one final, and no second emission.
    proxy.restore();
    wait_state(&pool, id, FightState::Settled).await;
    assert_eq!(sink.events(), vector.log.events);
    assert_eq!(
        row(&pool, id).await.log.as_deref(),
        Some(vector.canonical().as_slice())
    );
}

/// A sink that dies on its first batch, taking its task with it.
#[derive(Default)]
struct DyingSink {
    died: AtomicBool,
    inner: RecordingSink,
}

impl EventSink for DyingSink {
    fn publish(&self, fight_id: i32, from_index: usize, events: &[FightEvent]) {
        assert!(self.died.swap(true, Ordering::SeqCst), "the test sink");
        self.inner.publish(fight_id, from_index, events);
    }
}

/// R7: a release task that dies without giving its final goes to `Recompute`, and the fight ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_task_that_dies_is_rebuilt() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    let sink = Arc::new(DyingSink::default());
    let (settings, clock) = f::settings_at(live_at);
    let settings = FightSettings {
        sink: sink.clone(),
        ..settings
    };
    let (shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Live).await;
    wait_for("the death", || async { sink.died.load(Ordering::SeqCst) }).await;
    wait_for("the new emission", || async {
        !sink.inner.events().is_empty()
    })
    .await;
    assert_eq!(shared.current().halted, Some(false));
    clock.set(live_at + 100_000);
    wait_state(&pool, id, FightState::Settled).await;
    // The second emission gave every event, from the first one, and the log is the same.
    assert_eq!(sink.inner.events(), vector.log.events);
    assert_eq!(sink.inner.starts()[0], 0);
    assert_eq!(
        row(&pool, id).await.log.as_deref(),
        Some(vector.canonical().as_slice())
    );
}
