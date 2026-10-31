//! 010 RF15 and RF28 on the real binary: killed with SIGKILL with a fight on air, the next process
//! finishes it with the same bytes. With `RUST_LOG=trace`, neither process logs the seed.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use protocol::FightState;
use server::fights::hex;
use support::bin;
use support::db::TestDb;
use support::fights::{self as f, KO, Vector, row, wait_state};

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
    let started = now - vector.end_t() - 5000 + 6000;
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
