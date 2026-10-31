//! 010 RF38 to RF44, in the socket: what a viewer receives of a fight, at its hour, from a manual
//! clock. Events never come before their time, whatever the route to `live` (R4), and a slow client
//! skips ahead instead of piling up.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use engine::FightEvent;
use protocol::{FightFinal, FightState, FightSummary, ServerMessage, TimestampMs};
use server::app::Settings;
use server::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use server::fights::release::EventSink;
use server::fights::{FightClock, FightSettings, FightsShared, PassProbe};
use server::ws::{WsConfig, WsState};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use support::app::{ORIGIN, RunningApp, TRUST_LOOPBACK, user_with_session};
use support::db::TestDb;
use support::fights::{self as f, CADENCE, KO, T0, Vector, row, wait_for, wait_state};
use support::pg_proxy::PgProxy;
use support::ws::{self, Client, Ending};
use tokio_tungstenite::tungstenite::Message;

/// Enough for several ticks of 50 ms: what did not arrive by now, was not sent.
const SEVERAL_TICKS: Duration = Duration::from_millis(300);
const IN_TIME: Duration = Duration::from_secs(20);

struct Harness {
    app: RunningApp,
    pool: PgPool,
    shared: Arc<FightsShared>,
    sockets: Arc<WsState>,
    clock: FightClock,
    runner: Option<f::Running>,
}

async fn harness(db: &TestDb, now: i64, change: impl FnOnce(&mut FightSettings)) -> Harness {
    harness_with(db, now, WsConfig::default(), change).await
}

async fn harness_with(
    db: &TestDb,
    now: i64,
    ws: WsConfig,
    change: impl FnOnce(&mut FightSettings),
) -> Harness {
    let (mut settings, clock) = f::settings_at(now);
    change(&mut settings);
    let app_settings = Settings {
        fights: settings.clone(),
        ws,
        ..Settings::default()
    };
    let (app, pool, shared, sockets) = f::start_app_ws(db, &[TRUST_LOOPBACK], &app_settings).await;
    let runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);
    Harness {
        app,
        pool,
        shared,
        sockets,
        clock,
        runner: Some(runner),
    }
}

impl Harness {
    async fn stop_runner(&mut self) {
        if let Some(runner) = self.runner.take() {
            runner.stop().await;
        }
    }

    async fn connect(&self) -> Client {
        ws::connect(self.app.addr, ORIGIN).await.unwrap()
    }

    /// Waits for the hub to have this fight in this state, so the socket has something to read.
    async fn wait_hub(&self, id: i32) {
        wait_for("the hub", || async { self.shared.hub().knows(id) }).await;
    }

    async fn watch(&self, id: i32) -> Client {
        self.wait_hub(id).await;
        let mut client = self.connect().await;
        ws::subscribe(&mut client, &format!("fight:{id}")).await;
        client
    }
}

/// A fight whose window is closed, whose runner starts at `live_at`.
async fn on_air(db: &TestDb, vector: &Vector) -> (Harness, i32, i64) {
    let pool = db.migrated_pool().await;
    let id = f::open_vector(&pool, T0, vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    let harness = harness(db, live_at, |_| {}).await;
    wait_state(&pool, id, FightState::Live).await;
    (harness, id, live_at)
}

fn sync_of(message: Option<ServerMessage>) -> (Box<FightSummary>, TimestampMs, Vec<FightEvent>) {
    match message {
        Some(ServerMessage::FightSync {
            fight,
            server_time,
            events,
        }) => (fight, server_time, events),
        other => panic!("{other:?}"),
    }
}

/// Everything a socket holds of a fight, as a client would: the sync replaces, and the batches are
/// checked to be in their place, with nothing repeated and no gap.
#[derive(Default)]
struct Viewer {
    events: Vec<FightEvent>,
    state: Option<FightState>,
    final_result: Option<FightFinal>,
    syncs: usize,
}

impl Viewer {
    fn apply(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::FightSync { fight, events, .. } => {
                self.state = Some(fight.state);
                self.final_result = fight.final_result;
                self.events = events;
                self.syncs += 1;
            }
            ServerMessage::FightEvents {
                from_index, events, ..
            } => {
                assert_eq!(
                    usize::try_from(from_index).unwrap(),
                    self.events.len(),
                    "a batch out of its place"
                );
                self.events.extend(events);
            }
            other => panic!("{other:?}"),
        }
    }

    /// Reads until `done`, up to a limit.
    async fn read_until(&mut self, client: &mut Client, done: impl Fn(&Self) -> bool) {
        let deadline = tokio::time::Instant::now() + IN_TIME;
        while !done(self) {
            assert!(tokio::time::Instant::now() < deadline, "never reached");
            if let Some(message) = ws::next_message(client, Duration::from_secs(1)).await {
                self.apply(message);
            }
        }
    }
}

fn due(vector: &Vector, elapsed: i64) -> Vec<FightEvent> {
    vector
        .log
        .events
        .iter()
        .filter(|event| i64::from(event.t()) <= elapsed)
        .cloned()
        .collect()
}

/// RF38 and RF39: `subscribed`, then the state, and a message per transition with nothing repeated.
/// RF44: all with the hour of the server.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_viewer_follows_every_transition_in_order() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let h = harness(&db, T0 - 1, |_| {}).await;
    h.wait_hub(id).await;
    wait_for("the board", || async {
        h.shared.current().halted == Some(false)
    })
    .await;

    let mut fight = h.watch(id).await;
    let (summary, time, events) =
        sync_of(ws::next_message(&mut fight, Duration::from_secs(5)).await);
    assert_eq!(summary.state, FightState::Scheduled);
    assert_eq!(summary.id, u32::try_from(id).unwrap());
    assert_eq!(time.as_ms(), Some(T0 - 1));
    assert!(events.is_empty());

    let mut schedule = h.connect().await;
    ws::subscribe(&mut schedule, "schedule").await;
    let Some(ServerMessage::ScheduleUpdate { board }) =
        ws::next_message(&mut schedule, Duration::from_secs(5)).await
    else {
        panic!("no board");
    };
    assert_eq!(board.server_time.as_ms(), Some(T0 - 1));
    assert_eq!(board.halted, Some(false));
    assert_eq!(board.current.unwrap().state, FightState::Scheduled);

    let (closes_at, live_at) = f::hours(T0);
    f::open_with(&pool, id, &vector.seed, T0).await;
    for (hour, state) in [
        (T0, FightState::BettingOpen),
        (closes_at, FightState::BettingClosed),
    ] {
        h.clock.set(hour);
        wait_state(&pool, id, state).await;
        let (summary, time, events) = sync_of(ws::next_message(&mut fight, IN_TIME).await);
        assert_eq!(summary.state, state);
        assert_eq!(time.as_ms(), Some(hour));
        assert!(events.is_empty());
        let Some(ServerMessage::ScheduleUpdate { board }) =
            ws::next_message(&mut schedule, IN_TIME).await
        else {
            panic!("no board");
        };
        assert_eq!(board.current.unwrap().state, state);
        assert_eq!(board.server_time.as_ms(), Some(hour));
        // Once each.
        assert!(
            ws::drain(&mut fight, SEVERAL_TICKS).await.is_empty(),
            "{state:?}"
        );
        assert!(
            ws::drain(&mut schedule, SEVERAL_TICKS).await.is_empty(),
            "{state:?}"
        );
    }

    // On air, and to the end.
    h.clock.set(live_at);
    let mut viewer = Viewer::default();
    viewer
        .read_until(&mut fight, |v| {
            v.state == Some(FightState::Live) && !v.events.is_empty()
        })
        .await;
    assert_eq!(viewer.events, due(&vector, 0));

    h.clock.set(live_at + 100_000);
    viewer
        .read_until(&mut fight, |v| v.state == Some(FightState::Settled))
        .await;
    assert_eq!(viewer.events, vector.log.events);
    assert!(matches!(
        viewer.final_result,
        Some(FightFinal::Decided { .. })
    ));
    assert_eq!(row(&pool, id).await.state, FightState::Settled);
}

/// RF40 in the socket: in `t - 1` it did not arrive, and in `t` it does, `ko` and `end` included. RF44:
/// a batch has the hour of the server when it was released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_event_reaches_the_socket_at_its_time_and_not_before() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (h, id, live_at) = on_air(&db, &vector).await;
    let mut client = h.watch(id).await;
    let mut viewer = Viewer::default();

    let ko_t = vector
        .log
        .events
        .iter()
        .find(|event| matches!(event, FightEvent::Ko { .. }))
        .unwrap()
        .t();
    let end_t = vector.log.events.last().unwrap().t();
    for t in [ko_t, end_t] {
        let t = i64::from(t);
        h.clock.set(live_at + t - 1);
        let before = due(&vector, t - 1);
        viewer
            .read_until(&mut client, |v| v.events.len() >= before.len())
            .await;
        for message in ws::drain(&mut client, SEVERAL_TICKS).await {
            viewer.apply(message);
        }
        assert_eq!(viewer.events, before, "before {t}");

        h.clock.set(live_at + t);
        let Some(message) = ws::next_message(&mut client, IN_TIME).await else {
            panic!("the event of {t} did not come");
        };
        if let ServerMessage::FightEvents { server_time, .. } = &message {
            assert_eq!(server_time.as_ms(), Some(live_at + t));
        }
        viewer.apply(message);
        assert_eq!(viewer.events, due(&vector, t), "at {t}");
    }
}

/// R4: a fight that goes `live` before its hour releases nothing, not even its first event. A socket
/// sees the state with no events, whatever brought it there.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn before_its_hour_a_live_fight_has_no_events_on_the_late_route() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    // More than the margin after its hour: it starts in 10 s (RF15).
    let now = live_at + 5000;
    let h = harness(&db, now, |_| {}).await;
    wait_state(&pool, id, FightState::Live).await;
    let started = row(&pool, id).await.live_started_at.unwrap();
    assert_eq!(started, now + 10_000);
    assert_late_start(&h, id, started, &vector).await;
}

/// R4 again: resuming with a clock behind the hour of the fight.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn before_its_hour_a_live_fight_has_no_events_with_the_clock_behind() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    f::live_by_sql(&pool, id, live_at).await;
    let h = harness(&db, live_at - 7000, |_| {}).await;
    assert_late_start(&h, id, live_at, &vector).await;
}

async fn assert_late_start(h: &Harness, id: i32, started: i64, vector: &Vector) {
    let mut client = h.watch(id).await;
    let (summary, _, events) = sync_of(ws::next_message(&mut client, IN_TIME).await);
    assert_eq!(summary.state, FightState::Live);
    assert_eq!(
        summary.live_started_at.and_then(|hour| hour.as_ms()),
        Some(started)
    );
    assert!(events.is_empty());
    assert!(ws::drain(&mut client, SEVERAL_TICKS).await.is_empty());
    h.clock.set(started - 1);
    assert!(ws::drain(&mut client, SEVERAL_TICKS).await.is_empty());

    h.clock.set(started);
    let Some(ServerMessage::FightEvents {
        from_index, events, ..
    }) = ws::next_message(&mut client, IN_TIME).await
    else {
        panic!("no events at its hour");
    };
    assert_eq!(from_index, 0);
    assert_eq!(events, due(vector, 0));
}

/// RF38: who can subscribe to what, and what it costs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_a_fight_of_the_hub_can_be_subscribed_and_never_twice() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (mut h, id, _) = on_air(&db, &vector).await;

    let mut client = h.connect().await;
    let other = format!(r#"{{"type":"subscribe","channel":"fight:{}"}}"#, id + 1);
    ws::send_text(&mut client, &other).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(5)).await,
        Some(Message::Text(
            r#"{"type":"error","code":"ws.forbidden_channel"}"#.into()
        ))
    );

    // Without a session, and a channel already open answers and sends nothing else.
    h.wait_hub(id).await;
    ws::subscribe(&mut client, &format!("fight:{id}")).await;
    let _ = ws::next_message(&mut client, Duration::from_secs(5)).await;
    let _ = ws::drain(&mut client, SEVERAL_TICKS).await;
    ws::subscribe(&mut client, &format!("fight:{id}")).await;
    assert!(ws::drain(&mut client, SEVERAL_TICKS).await.is_empty());
    ws::subscribe(&mut client, "schedule").await;
    let _ = ws::next_message(&mut client, Duration::from_secs(5)).await;
    ws::subscribe(&mut client, "schedule").await;
    assert!(ws::drain(&mut client, SEVERAL_TICKS).await.is_empty());

    // RF38: subscribing never asks the database.
    h.stop_runner().await;
    h.pool.close().await;
    let mut late = h.connect().await;
    ws::subscribe(&mut late, &format!("fight:{id}")).await;
    let (summary, _, _) = sync_of(ws::next_message(&mut late, Duration::from_secs(5)).await);
    assert_eq!(summary.state, FightState::Live);
}

/// RF38: with one channel, `session.rs` took any subscription for `self`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_public_channels_neither_open_nor_close_self() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (h, id, _) = on_air(&db, &vector).await;
    let (user, token) = user_with_session(&h.pool).await;
    let cookie_name = support::config::config(&[])
        .cookies
        .session_name()
        .to_owned();
    let cookie = format!("{cookie_name}={token}");
    let mut client = ws::connect_with(h.app.addr, ORIGIN, &[("cookie", cookie.as_str())])
        .await
        .unwrap();
    let event = protocol::SelfEvent::ProfileChanged {
        display_name: "TigreVeloz42".to_owned(),
    };

    // `schedule` does not open `self`.
    ws::subscribe(&mut client, "schedule").await;
    let _ = ws::next_message(&mut client, Duration::from_secs(5)).await;
    h.sockets.self_hub().publish(user, event.clone());
    assert!(ws::drain(&mut client, SEVERAL_TICKS).await.is_empty());

    ws::subscribe(&mut client, "self").await;
    h.wait_hub(id).await;
    ws::subscribe(&mut client, &format!("fight:{id}")).await;
    let _ = ws::next_message(&mut client, Duration::from_secs(5)).await;
    let _ = ws::drain(&mut client, SEVERAL_TICKS).await;

    // Closing another one does not close `self`.
    ws::send_text(
        &mut client,
        &format!(r#"{{"type":"unsubscribe","channel":"fight:{id}"}}"#),
    )
    .await;
    let _ = ws::next_message(&mut client, Duration::from_secs(5)).await;
    h.sockets.self_hub().publish(user, event);
    assert!(matches!(
        ws::next_message(&mut client, Duration::from_secs(5)).await,
        Some(ServerMessage::SelfEvent { .. })
    ));
}

/// DP-B7: a socket asks for 3 `fight_sync` a minute, by subscribing. The fourth closes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_socket_cannot_keep_asking_for_the_whole_fight() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (h, id, live_at) = on_air(&db, &vector).await;
    let channel = format!("fight:{id}");
    let unsubscribe = format!(r#"{{"type":"unsubscribe","channel":"{channel}"}}"#);
    let mut client = h.watch(id).await;
    assert!(ws::next_message(&mut client, IN_TIME).await.is_some());

    // What the server sends by itself does not count, and neither does what stays open.
    h.clock.set(live_at + 100_000);
    wait_state(&h.pool, id, FightState::Settled).await;
    let _ = ws::drain(&mut client, SEVERAL_TICKS).await;
    ws::subscribe(&mut client, &channel).await;
    assert!(ws::drain(&mut client, SEVERAL_TICKS).await.is_empty());
    // Neither does the board.
    ws::subscribe(&mut client, "schedule").await;
    let _ = ws::next_message(&mut client, Duration::from_secs(5)).await;

    for _ in 0..2 {
        ws::send_text(&mut client, &unsubscribe).await;
        assert!(matches!(
            ws::next_message(&mut client, Duration::from_secs(5)).await,
            Some(ServerMessage::Unsubscribed { .. })
        ));
        ws::subscribe(&mut client, &channel).await;
        assert!(matches!(
            ws::next_message(&mut client, Duration::from_secs(5)).await,
            Some(ServerMessage::FightSync { .. })
        ));
    }
    ws::send_text(&mut client, &unsubscribe).await;
    assert!(
        ws::next_message(&mut client, Duration::from_secs(5))
            .await
            .is_some()
    );
    ws::send_text(
        &mut client,
        &format!(r#"{{"type":"subscribe","channel":"{channel}"}}"#),
    )
    .await;
    assert_eq!(
        ws::ending(&mut client, Duration::from_secs(5)).await,
        Ending::Close(1008, "ws.rate_limited".into())
    );
}

/// RNF: two sockets get the same bytes, and whoever comes late gets only what was released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_sockets_get_the_same_bytes_and_a_late_one_what_was_released() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (h, id, live_at) = on_air(&db, &vector).await;
    let mut a = h.watch(id).await;
    let mut b = h.watch(id).await;
    // After the first sync they are the same from here on.
    wait_for("the intro", || async {
        h.shared
            .hub()
            .held(id)
            .is_some_and(|(events, _)| events > 0)
    })
    .await;
    let _ = ws::next_data(&mut a, IN_TIME).await;
    let _ = ws::next_data(&mut b, IN_TIME).await;
    let t = i64::from(vector.log.events[10].t());
    h.clock.set(live_at + t);
    let (mine, theirs) = (
        ws::next_data(&mut a, IN_TIME).await,
        ws::next_data(&mut b, IN_TIME).await,
    );
    assert!(matches!(mine, Some(Message::Text(_))));
    assert_eq!(mine, theirs);

    let released = due(&vector, t);
    wait_for("the batch", || async {
        h.shared
            .hub()
            .held(id)
            .is_some_and(|(events, _)| events == released.len())
    })
    .await;
    let mut late = h.watch(id).await;
    let (_, _, events) = sync_of(ws::next_message(&mut late, IN_TIME).await);
    assert_eq!(events, released);

    // Reconnecting is the same: the state, and from there on.
    drop(late);
    let mut again = h.watch(id).await;
    let (_, _, events) = sync_of(ws::next_message(&mut again, IN_TIME).await);
    assert_eq!(events, released);
    h.clock.set(live_at + vector.end_t());
    let mut viewer = Viewer {
        events,
        ..Viewer::default()
    };
    viewer
        .read_until(&mut again, |v| v.events.len() == vector.log.events.len())
        .await;
    assert_eq!(viewer.events, vector.log.events);
}

/// RF40: a database that is down stops the settlement, never the events on the socket.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_the_database_down_the_socket_keeps_receiving() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (app, pool, shared, clock, id, live_at, proxy, _runner) = {
        let pool = db.migrated_pool().await;
        let id = f::open_vector(&pool, T0, &vector).await;
        let (closes_at, live_at) = f::hours(T0);
        f::close_by_sql(&pool, id, closes_at).await;
        let proxy = PgProxy::start(db.server_addr()).await;
        let through = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy_with(db.options_via(proxy.addr()));
        let (settings, clock) = f::settings_at(live_at);
        let app_settings = Settings {
            fights: settings.clone(),
            ..Settings::default()
        };
        let (app, _, shared, _) = f::start_app_ws(&db, &[TRUST_LOOPBACK], &app_settings).await;
        let runner = f::spawn_runner(&through, &shared, f::config_without_roster(), settings);
        (app, pool, shared, clock, id, live_at, proxy, runner)
    };
    wait_state(&pool, id, FightState::Live).await;
    wait_for("the hub", || async { shared.hub().knows(id) }).await;
    let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
    ws::subscribe(&mut client, &format!("fight:{id}")).await;
    let mut viewer = Viewer::default();
    viewer
        .read_until(&mut client, |v| !v.events.is_empty())
        .await;

    // DP-B3: the sockets on `schedule` hear that the lock went.
    let mut schedule = ws::connect(app.addr, ORIGIN).await.unwrap();
    ws::subscribe(&mut schedule, "schedule").await;
    let _ = ws::next_message(&mut schedule, IN_TIME).await;

    proxy.cut();
    wait_for("the lost lock", || async {
        shared.current().halted.is_none()
    })
    .await;
    let Some(ServerMessage::ScheduleUpdate { board }) =
        ws::next_message(&mut schedule, IN_TIME).await
    else {
        panic!("no word of the lost lock");
    };
    assert_eq!(board.halted, None);

    clock.set(live_at + vector.end_t());
    viewer
        .read_until(&mut client, |v| v.events.len() == vector.log.events.len())
        .await;
    assert_eq!(viewer.events, vector.log.events);
    assert_eq!(row(&pool, id).await.state, FightState::Live);
}

#[derive(Default)]
struct DyingSink {
    died: AtomicBool,
}

impl EventSink for DyingSink {
    fn publish(&self, _fight_id: i32, _from_index: usize, _events: &[FightEvent]) {
        assert!(self.died.swap(true, Ordering::SeqCst), "the test sink");
    }
}

/// R7: a release task that dies starts over from the first event. The socket does not see it twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_task_that_dies_repeats_nothing_on_the_socket() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    let sink = Arc::new(DyingSink::default());
    let dying = sink.clone();
    let h = harness(&db, live_at, |settings| settings.sink = dying).await;
    wait_state(&pool, id, FightState::Live).await;
    let mut client = h.watch(id).await;
    wait_for("the death", || async { sink.died.load(Ordering::SeqCst) }).await;
    let mut viewer = Viewer::default();
    viewer
        .read_until(&mut client, |v| !v.events.is_empty())
        .await;
    h.clock.set(live_at + vector.end_t());
    // `Viewer` fails on a batch out of its place.
    viewer
        .read_until(&mut client, |v| v.events.len() == vector.log.events.len())
        .await;
    assert_eq!(viewer.events, vector.log.events);
}

struct FailsFor {
    id: i32,
    fixed: AtomicBool,
}

impl SettlementHook for FailsFor {
    fn settle<'a>(
        &'a self,
        _tx: &'a mut sqlx::PgConnection,
        fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        let fails = fight.id == self.id && !self.fixed.load(Ordering::SeqCst);
        Box::pin(async move {
            if fails {
                Err(SettlementError { kind: "test" })
            } else {
                Ok(())
            }
        })
    }
}

/// Finding 8, amendment DP-B8: during a halt the next fight is cancelled. When the halted one finally
/// settles, whoever watched it gets its final. Before that amendment it left the hub without a word.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_final_of_a_halted_fight_reaches_its_socket() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let halted = f::open_vector(&pool, T0, &vector).await;
    let cancelled = f::announce(&pool, T0 + 2 * CADENCE, &vector.a, &vector.b).await;
    let hook = Arc::new(FailsFor {
        id: halted,
        fixed: AtomicBool::new(false),
    });
    let the_hook = hook.clone();
    let h = harness(&db, T0, |settings| settings.hook = the_hook).await;
    let (closes_at, live_at) = f::hours(T0);

    h.wait_hub(halted).await;
    let mut client = h.watch(halted).await;
    h.clock.set(closes_at);
    wait_state(&pool, halted, FightState::BettingClosed).await;
    h.clock.set(live_at);
    wait_state(&pool, halted, FightState::Live).await;
    h.clock.set(live_at + vector.end_t() + 10_000);
    wait_for("the halt", || async {
        h.clock.advance(2000);
        h.shared.current().halted == Some(true)
    })
    .await;
    let mut viewer = Viewer::default();
    viewer
        .read_until(&mut client, |v| v.events.len() == vector.log.events.len())
        .await;

    h.clock
        .set(h.clock.now_ms().max(T0 + 2 * CADENCE + f::WINDOW));
    wait_state(&pool, cancelled, FightState::Settled).await;
    wait_for("the cancelled one", || async {
        h.shared
            .current()
            .last
            .is_some_and(|last| last.id == u32::try_from(cancelled).unwrap())
    })
    .await;
    assert_ne!(viewer.state, Some(FightState::Settled));

    hook.fixed.store(true, Ordering::SeqCst);
    f::wait_state_advancing(&pool, &h.clock, halted, FightState::Settled).await;
    viewer
        .read_until(&mut client, |v| v.state == Some(FightState::Settled))
        .await;
    assert_eq!(viewer.events, vector.log.events);
    assert!(matches!(
        viewer.final_result,
        Some(FightFinal::Decided { .. })
    ));
}

/// RF59: the CLI is another process, and the runner sees its final in its reread. With a panic of the
/// loop in between: the hub survives it, so it still knows what to look for.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_final_from_outside_reaches_the_sockets_even_after_a_panic() {
    #[derive(Default)]
    struct PanicOnce {
        armed: AtomicBool,
    }
    impl PassProbe for PanicOnce {
        fn before_pass(&self) {
            assert!(!self.armed.swap(false, Ordering::SeqCst), "the test probe");
        }
    }

    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    let probe = Arc::new(PanicOnce::default());
    let the_probe = probe.clone();
    let h = harness(&db, live_at, |settings| settings.probe = the_probe).await;
    wait_state(&pool, id, FightState::Live).await;
    wait_for("the board", || async {
        h.shared.current().halted == Some(false)
    })
    .await;
    let mut client = h.watch(id).await;
    let mut viewer = Viewer::default();
    viewer
        .read_until(&mut client, |v| !v.events.is_empty())
        .await;
    let mut schedule = h.connect().await;
    ws::subscribe(&mut schedule, "schedule").await;
    let _ = ws::next_message(&mut schedule, IN_TIME).await;

    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, id, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();
    probe.armed.store(true, Ordering::SeqCst);
    wait_for("the panic", || async {
        h.clock.advance(1000);
        !probe.armed.load(Ordering::SeqCst)
    })
    .await;

    // DP-B3: `lock_lost` after the panic, also with no pass in between.
    let updates = ws::drain(&mut schedule, Duration::from_secs(1)).await;
    let halted: Vec<_> = updates
        .iter()
        .filter_map(|update| match update {
            ServerMessage::ScheduleUpdate { board } => Some(board.halted),
            _ => None,
        })
        .collect();
    assert!(halted.contains(&None), "{updates:?}");
    assert_eq!(halted.last(), Some(&Some(false)), "{updates:?}");

    viewer
        .read_until(&mut client, |v| v.state == Some(FightState::Settled))
        .await;
    assert!(matches!(
        viewer.final_result,
        Some(FightFinal::Voided { .. })
    ));
    assert_eq!(
        h.shared.current().last.map(|last| last.id),
        Some(u32::try_from(id).unwrap())
    );
}

/// RF38: after a restart, the last fight is in the hub, with its events from its log. A log that does
/// not decode leaves it without events, and nothing else happens.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn after_a_restart_the_last_fight_has_the_events_of_its_log() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    let mut h = harness(&db, live_at, |_| {}).await;
    wait_state(&pool, id, FightState::Live).await;
    h.clock.set(live_at + 100_000);
    wait_state(&pool, id, FightState::Settled).await;
    h.stop_runner().await;

    let mut h = harness(&db, live_at + 200_000, |_| {}).await;
    let mut client = h.watch(id).await;
    let (summary, _, events) = sync_of(ws::next_message(&mut client, IN_TIME).await);
    assert_eq!(summary.state, FightState::Settled);
    assert_eq!(events, vector.log.events);
    h.stop_runner().await;

    // With the guard off, as only someone with the credential can.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE fights DISABLE TRIGGER fights_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE fights SET log = '{' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE fights ENABLE TRIGGER fights_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let h = harness(&db, live_at + 300_000, |_| {}).await;
    let mut client = h.watch(id).await;
    let (summary, _, events) = sync_of(ws::next_message(&mut client, IN_TIME).await);
    assert_eq!(summary.state, FightState::Settled);
    assert!(events.is_empty());
    assert_eq!(h.shared.current().halted, Some(false));
}

/// The longest event of a vector, as many times as the bound of the ruleset says, on a fight on air.
fn events_at_the_bound(vector: &Vector, count: usize) -> Vec<FightEvent> {
    let longest = vector
        .log
        .events
        .iter()
        .max_by_key(|event| serde_json::to_string(event).unwrap().len())
        .unwrap();
    vec![longest.clone(); count]
}

/// RF43: a `fight_sync` at the bound of events fits the write buffer, with the default settings.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sync_at_the_bound_of_events_is_sent() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let (h, id, _) = on_air(&db, &vector).await;
    wait_for("the intro", || async {
        h.shared
            .hub()
            .held(id)
            .is_some_and(|(events, _)| events == 1)
    })
    .await;
    let bound = events_at_the_bound(&vector, 417);
    h.shared.hub().publish(id, 1, &bound);
    let mut client = h.watch(id).await;
    let Some(Message::Text(text)) = ws::next_data(&mut client, IN_TIME).await else {
        panic!("the sync did not arrive");
    };
    assert!(text.len() > 40_000, "{}", text.len());
    assert!(text.len() < WsConfig::default().max_write_buffer);
    let parsed: ServerMessage = serde_json::from_str(text.as_str()).unwrap();
    let (_, _, events) = sync_of(Some(parsed));
    assert_eq!(events.len(), 418);
}

/// A socket that never reads, with the smallest buffer the system gives.
async fn stuck_socket(addr: SocketAddr, channel: &str) -> Client {
    use futures_util::SinkExt;
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_recv_buffer_size(2048).unwrap();
    let stream = socket.connect(addr).await.unwrap();
    let mut request =
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(format!(
            "ws://{addr}/ws"
        ))
        .unwrap();
    request
        .headers_mut()
        .insert("origin", ORIGIN.parse().unwrap());
    let (mut client, _) =
        tokio_tungstenite::client_async(request, tokio_tungstenite::MaybeTlsStream::Plain(stream))
            .await
            .unwrap();
    client
        .send(Message::Text(
            serde_json::json!({"type": "subscribe", "channel": channel})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    client
}

/// RF43: a socket that does not read holds nothing in the hub, the others stay on time, it closes in
/// `close_grace`, and when it comes back it catches up.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_socket_that_does_not_read_is_closed_and_the_others_go_on() {
    let db = TestDb::create().await;
    let vector = Vector::of(KO);
    let pool = db.migrated_pool().await;
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&pool, id, closes_at).await;
    // Room for the sync of a healthy socket that happens to fall behind: this test sends far more
    // than a fight has.
    let config = WsConfig {
        close_grace: Duration::from_millis(300),
        max_write_buffer: 16 * 1024 * 1024,
        ..WsConfig::default()
    };
    let h = harness_with(&db, live_at, config, |_| {}).await;
    wait_state(&pool, id, FightState::Live).await;
    wait_for("the intro", || async {
        h.shared
            .hub()
            .held(id)
            .is_some_and(|(events, _)| events == 1)
    })
    .await;
    let free = h.sockets.available_permits();

    let stuck = stuck_socket(h.app.addr, &format!("fight:{id}")).await;
    wait_for("the stuck socket", || async {
        h.sockets.available_permits() == free - 1
    })
    .await;
    let mut healthy = h.watch(id).await;
    let reading = tokio::spawn(async move {
        let mut seen = 0_usize;
        while let Some(message) = ws::next_message(&mut healthy, Duration::from_secs(10)).await {
            match message {
                ServerMessage::FightSync { events, .. } => seen = events.len(),
                ServerMessage::FightEvents {
                    from_index, events, ..
                } => {
                    assert_eq!(usize::try_from(from_index).unwrap(), seen);
                    seen += events.len();
                }
                other => panic!("{other:?}"),
            }
        }
        seen
    });

    // Batches until the buffers of the system are full and the send of the stuck one times out.
    let batch = events_at_the_bound(&vector, 400);
    let mut released = 1;
    let mut rounds = 0;
    // The healthy one keeps its permit.
    while h.sockets.available_permits() != free - 1 {
        assert!(rounds < 2000, "the stuck socket stayed open");
        h.shared.hub().publish(id, released, &batch);
        released += batch.len();
        rounds += 1;
        // The hub holds the events and one batch, and nothing for a socket.
        let (events, _) = h.shared.hub().held(id).unwrap();
        assert_eq!(events, released);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(stuck);

    // The one that was reading got everything, in its place.
    h.shared.hub().publish(id, released, &batch[..1]);
    released += 1;
    let total = released;
    wait_for("the healthy socket", || async {
        h.shared
            .hub()
            .held(id)
            .is_some_and(|(events, _)| events == total)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!reading.is_finished());
    reading.abort();

    // It comes back and it is up to date, with a sync.
    let mut back = h.watch(id).await;
    let (_, _, events) = sync_of(ws::next_message(&mut back, IN_TIME).await);
    assert_eq!(events.len(), total);
}
