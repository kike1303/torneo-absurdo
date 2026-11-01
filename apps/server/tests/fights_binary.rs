//! 010 RF15 and RF28 on the real binary: killed with SIGKILL with a fight on air, the next process
//! finishes it with the same bytes. With `RUST_LOG=trace`, neither process logs the seed. And with a
//! socket connected to the real binary: `serve` feeds the hub, and no line has a fragment of what the
//! socket received.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use protocol::{FightState, ServerMessage};
use server::fights::hex;
use support::bin;
use support::db::TestDb;
use support::fights::{self as f, KO, Vector, row, wait_state};
use support::ws;

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kill_with_a_fight_on_air_changes_nothing() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);

    // A fight on air whose `end` already passed, with its `settle_at` a few seconds ahead: the
    // whole cycle does not have to be waited, and `live_started_at` is a real hour in the past.
    let now = now_ms();
    let started = now - vector.end_t() - 5000 + 15_000;
    let opens_at = started - f::WINDOW - f::PRE_LIVE;
    let id = f::open_vector(&pool, opens_at, &vector).await;
    let (closes_at, _) = f::hours(opens_at);
    f::close_by_sql(&pool, id, closes_at).await;
    f::live_by_sql(&pool, id, started).await;

    let url = db.url();
    let envs = [
        ("DATABASE_URL", url.as_str()),
        ("PUBLIC_ORIGIN", "http://localhost:5173"),
        ("PORT", "0"),
        ("BIND_ADDR", "127.0.0.1"),
        ("RUST_LOG", "trace"),
    ];
    let first = bin::spawn(&[], &envs);
    first.wait_listening(Duration::from_secs(15)).await;
    assert_eq!(row(&pool, id).await.state, FightState::Live);
    let killed = first.kill().await;
    assert_eq!(row(&pool, id).await.state, FightState::Live);
    assert_eq!(row(&pool, id).await.revealed_seed, None);

    let second = bin::spawn(&[], &envs);
    second.wait_listening(Duration::from_secs(15)).await;
    wait_state(&pool, id, FightState::Settled).await;
    let finished = second.kill().await;

    let fight = row(&pool, id).await;
    assert_eq!(fight.log, Some(vector.canonical()));
    assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
    assert_eq!(fight.live_started_at, Some(started));

    let seed = hex(&vector.seed);
    let log = String::from_utf8(vector.canonical()).unwrap();
    for line in killed.stdout.iter().chain(&finished.stdout) {
        assert!(!line.contains(&seed), "{line}");
        assert!(!line.contains(&log), "{line}");
        assert!(!line.contains(r#""winner":"#), "{line}");
    }
    assert!(
        finished
            .stdout
            .iter()
            .any(|line| line.contains("fights.settled")),
        "{:?}",
        finished.stdout
    );
}

/// RF28 with a socket connected, in the real binary: what `serve` builds, with no hook of a test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_socket_on_the_real_binary_sees_the_fight_and_the_logs_see_nothing() {
    const ORIGIN: &str = "http://localhost:5173";
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);

    // On air, with its `end` a few seconds ahead: the socket has time to see it come.
    let started = now_ms() - vector.end_t() + 5000;
    let opens_at = started - f::WINDOW - f::PRE_LIVE;
    let id = f::open_vector(&pool, opens_at, &vector).await;
    let (closes_at, _) = f::hours(opens_at);
    f::close_by_sql(&pool, id, closes_at).await;
    f::live_by_sql(&pool, id, started).await;

    let url = db.url();
    let server = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", url.as_str()),
            ("PUBLIC_ORIGIN", ORIGIN),
            ("PORT", "0"),
            ("BIND_ADDR", "127.0.0.1"),
            ("RUST_LOG", "trace"),
        ],
    );
    let port = server.wait_listening(Duration::from_secs(15)).await;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut client = ws::connect(addr, ORIGIN).await.unwrap();
    ws::send_text(&mut client, r#"{"type":"subscribe","channel":"schedule"}"#).await;

    // The fight is in the hub once the runner went through it, and the board says so.
    let channel = format!(r#"{{"type":"subscribe","channel":"fight:{id}"}}"#);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut subscribed = false;
    let mut sent_at = None;
    let mut released: Vec<protocol::FightEvent> = Vec::new();
    let mut settled = false;
    while !settled || released.len() < vector.log.events.len() {
        assert!(tokio::time::Instant::now() < deadline, "{released:?}");
        if !subscribed && sent_at.is_none_or(|at: tokio::time::Instant| at.elapsed().as_secs() >= 2)
        {
            ws::send_text(&mut client, &channel).await;
            sent_at = Some(tokio::time::Instant::now());
        }
        match ws::next_message(&mut client, Duration::from_millis(500)).await {
            Some(ServerMessage::Subscribed { channel }) if channel.to_string() != "schedule" => {
                subscribed = true;
            }
            Some(ServerMessage::FightSync { fight, .. }) => {
                settled = fight.state == FightState::Settled;
            }
            Some(ServerMessage::FightEvents {
                from_index, events, ..
            }) => {
                let from = usize::try_from(from_index).unwrap();
                released.truncate(from);
                released.extend(events);
            }
            _ => {}
        }
    }
    assert_eq!(released, vector.log.events);
    assert_eq!(row(&pool, id).await.state, FightState::Settled);
    let finished = server.kill().await;

    let seed = hex(&vector.seed);
    let log = String::from_utf8(vector.canonical()).unwrap();
    let mut canaries = vec![
        seed.clone(),
        seed.to_uppercase(),
        log,
        r#""winner":"#.to_owned(),
        r#""attacker":"#.to_owned(),
        r#""type":"#.to_owned(),
        "fight_sync".to_owned(),
        "fight_events".to_owned(),
        "schedule_update".to_owned(),
    ];
    for event in &vector.log.events {
        canaries.push(serde_json::to_string(event).unwrap());
    }
    for line in finished.stdout.iter().chain(&finished.stderr) {
        for canary in &canaries {
            assert!(!line.contains(canary.as_str()), "{canary} in {line}");
        }
    }
    // The run did log, at `trace`, and the socket closed by what the test did.
    assert!(
        finished
            .stdout
            .iter()
            .any(|line| line.contains("fights.settled")),
        "{:?}",
        finished.stdout
    );
}
