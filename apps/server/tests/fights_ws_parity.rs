//! 010 RF41: nothing delates before `Live`. Two fights with the same specs and two seeds that give a
//! different result or duration produce, on a manual clock, the same messages and the same bodies of
//! the two `GET` until `live_started_at`, except the `id`, the `seed_commit` and the hours.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use protocol::{FightState, ServerMessage};
use serde_json::Value;
use server::app::Settings;
use server::fights::{FightSettings, Gate};
use support::app::{ORIGIN, TRUST_LOOPBACK};
use support::db::TestDb;
use support::fights::{self as f, T0, Vector, wait_for, wait_state};
use support::{http, ws};
use tokio::sync::watch;

const SEVERAL_TICKS: Duration = Duration::from_millis(300);

/// Two vectors of `turtle` against `turtle`: a draw and whatever the other seed gives.
const SEEDS: [u32; 2] = [118, 1];

/// Holds the computation of the log until the test opens it.
struct HeldGate {
    open: watch::Sender<bool>,
}

impl Gate for HeldGate {
    fn wait(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let mut open = self.open.subscribe();
        Box::pin(async move {
            let _ = open.wait_for(|open| *open).await;
        })
    }
}

/// Without what can differ. The hours are the same in both runs, and they stay.
fn normal(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("id");
            map.remove("seed_commit");
            for inner in map.values_mut() {
                normal(inner);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normal),
        _ => {}
    }
}

fn normal_text(text: &str) -> String {
    let mut value: Value = serde_json::from_str(text).unwrap();
    normal(&mut value);
    value.to_string()
}

async fn read_channel(client: &mut ws::Client, out: &mut Vec<String>) {
    while let Some(message) = ws::next_data(client, SEVERAL_TICKS).await {
        out.push(normal_text(message.into_text().unwrap().as_str()));
    }
}

async fn read_routes(addr: SocketAddr, id: i32, out: &mut Vec<String>) {
    for path in [
        "/api/fights/current".to_owned(),
        format!("/api/fights/{id}"),
    ] {
        let response = http::send(addr, "GET", &path, &[]).await;
        assert_eq!(response.status, 200);
        out.push(normal_text(&response.text()));
    }
}

/// Everything public of one fight, step by step until `live_started_at`, one channel per socket: the
/// order between two channels does not mean anything.
async fn transcript(vector: &Vector) -> Vec<String> {
    let db = TestDb::create().await;
    let gate = Arc::new(HeldGate {
        open: watch::Sender::new(false),
    });
    let (settings, clock) = f::settings_at(T0 - 1);
    let settings = FightSettings {
        compute_gate: gate.clone(),
        ..settings
    };
    let app_settings = Settings {
        fights: settings.clone(),
        ..Settings::default()
    };
    let (app, pool, shared, _) = f::start_app_ws(&db, &[TRUST_LOOPBACK], &app_settings).await;
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let _runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);
    wait_for("the board", || async {
        shared.current().halted == Some(false) && shared.hub().knows(id)
    })
    .await;

    let mut out = Vec::new();
    let mut schedule = ws::connect(app.addr, ORIGIN).await.unwrap();
    ws::subscribe(&mut schedule, "schedule").await;
    let mut fight = ws::connect(app.addr, ORIGIN).await.unwrap();
    ws::subscribe(&mut fight, &format!("fight:{id}")).await;
    let step = async |schedule: &mut ws::Client, fight: &mut ws::Client, out: &mut Vec<String>| {
        read_channel(schedule, out).await;
        out.push("--".to_owned());
        read_channel(fight, out).await;
        out.push("--".to_owned());
        read_routes(app.addr, id, out).await;
        out.push("==".to_owned());
    };
    step(&mut schedule, &mut fight, &mut out).await;

    f::open_with(&pool, id, &vector.seed, T0).await;
    clock.set(T0);
    wait_state(&pool, id, FightState::BettingOpen).await;
    step(&mut schedule, &mut fight, &mut out).await;

    // The close goes out with the computation held: its hour does not depend on it.
    let (closes_at, live_at) = f::hours(T0);
    clock.set(closes_at);
    wait_state(&pool, id, FightState::BettingClosed).await;
    let Some(ServerMessage::ScheduleUpdate { board }) =
        ws::next_message(&mut schedule, Duration::from_secs(20)).await
    else {
        panic!("the close did not go out with the computation held");
    };
    let current = board.current.as_ref().unwrap();
    assert_eq!(current.state, FightState::BettingClosed);
    out.push(normal_text(
        &serde_json::to_string(&ServerMessage::ScheduleUpdate { board }).unwrap(),
    ));
    step(&mut schedule, &mut fight, &mut out).await;

    // The computation ends in `betting_closed`: not one message comes from it.
    gate.open.send_replace(true);
    assert!(ws::drain(&mut schedule, SEVERAL_TICKS).await.is_empty());
    assert!(ws::drain(&mut fight, SEVERAL_TICKS).await.is_empty());
    step(&mut schedule, &mut fight, &mut out).await;

    clock.set(live_at - 1);
    step(&mut schedule, &mut fight, &mut out).await;
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_fights_with_different_results_look_the_same_until_they_start() {
    let of = |seed| Vector::of(("turtle", "turtle", seed));
    let (one, other) = (of(SEEDS[0]), of(SEEDS[1]));
    // Otherwise this test proves nothing.
    assert_ne!(one.seed, other.seed);
    assert!(
        one.end_t() != other.end_t() || one.log.outcome() != other.log.outcome(),
        "the two seeds give the same fight"
    );
    let first = transcript(&one).await;
    let second = transcript(&other).await;
    assert!(first.len() > 20, "{first:?}");
    assert!(first.iter().any(|line| line.contains("fight_sync")));
    assert!(first.iter().any(|line| line.contains("schedule_update")));
    assert_eq!(first, second);
}
