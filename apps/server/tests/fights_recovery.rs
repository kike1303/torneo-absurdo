//! 010 RF5, RF15, RF16 and the security review R8: a restart in each state changes nothing, and what
//! cannot go on stops, with no final made up.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use protocol::FightState;
use server::fights::{FightSettings, FightsShared, PassProbe};
use sqlx::PgPool;
use support::db::TestDb;
use support::fights::{
    self as f, CADENCE, KO, RecordingSink, T0, Vector, WINDOW, row, wait_for, wait_state,
};

fn assert_is_the_vector(fight: &server::fights::state::FightRow, vector: &Vector) {
    assert_eq!(fight.state, FightState::Settled);
    assert_eq!(fight.log.as_deref(), Some(vector.canonical().as_slice()));
    assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
}

/// The table of RF15: the runner dies in each state with a commit, and the next one finishes the
/// fight with the same seed, snapshot and bytes, and one final.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_in_each_state_changes_nothing() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let (closes_at, live_at) = f::hours(T0);

    // In `betting_open`, in time.
    let id = f::open_vector(&pool, T0, &vector).await;
    let (settings, clock) = f::settings_at(T0 + 1);
    let (shared, runner) = f::start(&pool, &settings);
    wait_for("the evaluation", || async {
        shared.current().halted == Some(false)
    })
    .await;
    assert_eq!(row(&pool, id).await.state, FightState::BettingOpen);
    runner.stop().await;

    // In `betting_closed`, killed before `live_at`.
    clock.set(closes_at);
    let (_shared, runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::BettingClosed).await;
    runner.stop().await;

    // In `live`, with the fight half way through.
    clock.set(live_at);
    let (_shared, runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Live).await;
    clock.set(live_at + vector.end_t() / 2);
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.stop().await;
    let started = row(&pool, id).await.live_started_at;
    assert_eq!(started, Some(live_at));

    // With `live` past its `end`: released at once as already past, and settled.
    clock.set(live_at + 100_000);
    let (_shared, runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Settled).await;
    runner.stop().await;

    let fight = row(&pool, id).await;
    assert_is_the_vector(&fight, &vector);
    // `live_started_at` is written once: it does not come back on air after a restart.
    assert_eq!(fight.live_started_at, started);
    assert_eq!(f::secrets_of(&pool, id).await, 0);
}

/// RF15: a scheduled fight that was missed while it was down opens late, with the same `closes_at`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_opens_late_with_its_same_close() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let (settings, _clock) = f::settings_at(T0 + WINDOW / 2);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::BettingOpen).await;
    let fight = row(&pool, id).await;
    assert_eq!(fight.opened_at, Some(T0 + WINDOW / 2));
    assert_eq!(fight.closes_at, T0 + WINDOW);
}

async fn wait_halted(shared: &Arc<FightsShared>, halted: bool) {
    wait_for("the halt", || async {
        shared.current().halted == Some(halted)
    })
    .await;
}

/// What a safe stop must hold to: nothing advances, nothing new appears, no final is made up.
async fn assert_stopped(pool: &PgPool, id: i32, state: FightState) {
    tokio::time::sleep(Duration::from_millis(200)).await;
    let fight = row(pool, id).await;
    assert_eq!(fight.state, state);
    assert_eq!(fight.final_result(), None);
    assert_eq!(fight.revealed_seed, None);
}

async fn closed_fight(pool: &PgPool, vector: &Vector) -> i32 {
    let id = f::open_vector(pool, T0, vector).await;
    f::close_by_sql(pool, id, T0 + WINDOW).await;
    id
}

/// RF16 and RF23: the secret missing or not matching stops the fight and halts the rest, without a
/// final. A secret restored from a backup lets it go on (worry 13), and it ends with its log.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_secret_stops_the_fight_and_a_restored_one_resumes_it() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = closed_fight(&pool, &vector).await;
    f::lose_secret(&pool, id).await;
    let (_, live_at) = f::hours(T0);
    let (settings, clock) = f::settings_at(live_at);
    let (shared, _runner) = f::start(&pool, &settings);
    wait_halted(&shared, true).await;
    assert_stopped(&pool, id, FightState::BettingClosed).await;

    // RF16: with a halt, no new window opens. This one keeps waiting while it has its window.
    let next = f::announce(&pool, T0 + CADENCE, &vector.a, &vector.b).await;
    clock.set(T0 + CADENCE);
    wait_for("the reread", || async { shared.current().next.is_some() }).await;
    assert_eq!(row(&pool, next).await.state, FightState::Scheduled);
    assert_eq!(f::secrets_of(&pool, next).await, 0);

    // A restore that does not match is refused by the database; the right one goes in.
    sqlx::query("INSERT INTO fight_secrets (fight_id, seed) VALUES ($1, $2)")
        .bind(id)
        .bind(vector.seed.as_slice())
        .execute(&pool)
        .await
        .unwrap();
    // RF15: the runner sees it in its reread, on its 60 s.
    clock.advance(60_000);
    wait_state(&pool, id, FightState::Live).await;
    wait_halted(&shared, false).await;
    // It left with the late route, since its hour had passed.
    assert!(row(&pool, id).await.live_started_at.unwrap() > live_at);
    clock.advance(200_000);
    wait_state(&pool, id, FightState::Settled).await;
    assert_is_the_vector(&row(&pool, id).await, &vector);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_secret_that_does_not_match_stops_the_fight() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = closed_fight(&pool, &vector).await;
    f::replace_secret(&pool, id, &[9; 32]).await;
    let (_, live_at) = f::hours(T0);
    let (settings, _clock) = f::settings_at(live_at);
    let (shared, _runner) = f::start(&pool, &settings);
    wait_halted(&shared, true).await;
    assert_stopped(&pool, id, FightState::BettingClosed).await;
}

/// RF5 and RF16: every discrepancy of a snapshot with a commit published is a safe stop. Its `from_row`
/// also halts the fights (R8), and a corrupt row never gets a final.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_snapshot_after_the_commit_is_a_safe_stop() {
    type Change = fn(&mut f::RawFight);
    let cases: [(&str, Change); 7] = [
        ("version", |raw| raw.ruleset_version = 999),
        ("decode", |raw| raw.spec_a = b"{".to_vec()),
        ("reserialize", |raw| {
            raw.spec_a = [b" ".as_slice(), &raw.spec_a].concat();
        }),
        ("hash", |raw| raw.snapshot_sha256 = vec![0; 32]),
        ("slug", |raw| raw.slug_a = "other".to_owned()),
        ("max_hp", |raw| raw.max_hp_b += 1),
        // M6: it decodes, its hash and its bounds are right, and it does not pass `validate`.
        ("validate", |raw| {
            let mut spec: engine::CharacterSpec = serde_json::from_slice(&raw.spec_a).unwrap();
            spec.stats.hp = 0;
            raw.spec_a = engine::canonical_spec_json(&spec).unwrap();
            raw.snapshot_sha256 =
                server::fights::snapshot::snapshot_sha256(1, &raw.spec_a, &raw.spec_b).to_vec();
            let ruleset = engine::Ruleset::for_version(1).unwrap();
            raw.max_hp_a = i32::from(server::fights::snapshot::max_hp(&ruleset, &spec).unwrap());
        }),
    ];
    let vector = Vector::of(KO);
    for (name, change) in cases {
        let db = TestDb::create().await;
        let pool = db.migrated_pool().await;
        let id = f::announce_with(&pool, T0, &vector.a, &vector.b, change).await;
        f::open_with(&pool, id, &vector.seed, T0).await;

        // R8: already in `betting_open`, and it comes only from public data. It still closes.
        let (settings, clock) = f::settings_at(T0 + 1);
        let (shared, runner) = f::start(&pool, &settings);
        wait_halted(&shared, true).await;
        assert_eq!(
            row(&pool, id).await.state,
            FightState::BettingOpen,
            "{name}"
        );
        clock.set(T0 + WINDOW);
        wait_state(&pool, id, FightState::BettingClosed).await;
        let (_, live_at) = f::hours(T0);
        clock.set(live_at);
        assert_stopped(&pool, id, FightState::BettingClosed).await;
        assert_eq!(shared.current().halted, Some(true), "{name}");
        runner.stop().await;

        // In `live`, after a restart: the same.
        f::live_by_sql(&pool, id, live_at).await;
        let (shared, _runner) = f::start(&pool, &settings);
        wait_halted(&shared, true).await;
        assert_stopped(&pool, id, FightState::Live).await;
        pool.close().await;
    }
}

/// RF16: a halted fight in `betting_open` does not accept bets (fights_bets_contract.rs), closes at
/// its hour and does not go live; the fight already in `live` goes on to its end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_halt_stops_what_comes_next_and_not_what_is_on_air() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    // The cause: a fight stopped for good.
    let stopped = closed_fight(&pool, &vector).await;
    f::lose_secret(&pool, stopped).await;
    // On air: it started on time, with its secret.
    let on_air = f::open_vector(&pool, T0 + 1000, &vector).await;
    f::close_by_sql(&pool, on_air, T0 + 1000 + WINDOW).await;
    let (_, on_air_live) = f::hours(T0 + 1000);
    f::live_by_sql(&pool, on_air, on_air_live).await;
    // Open: it closes at its hour, and stays there.
    let open = f::open_vector(&pool, T0 + 2000, &vector).await;
    let (open_closes, open_live) = f::hours(T0 + 2000);

    let (settings, clock) = f::settings_at(open_closes);
    let (shared, _runner) = f::start(&pool, &settings);
    wait_halted(&shared, true).await;
    wait_state(&pool, open, FightState::BettingClosed).await;
    clock.set(open_live + 100_000);
    wait_state(&pool, on_air, FightState::Settled).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(row(&pool, open).await.state, FightState::BettingClosed);
    assert_eq!(row(&pool, stopped).await.state, FightState::BettingClosed);
    assert_eq!(shared.current().halted, Some(true));
    // A scheduled fight that loses its window during the halt is cancelled, and nothing new appears.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 3);
}

/// A probe that panics once, when armed.
#[derive(Default)]
struct PanicOnce {
    armed: AtomicBool,
}

impl PassProbe for PanicOnce {
    fn before_pass(&self) {
        assert!(!self.armed.swap(false, Ordering::SeqCst), "the test probe");
    }
}

/// Architect's point 9: a panic of the loop aborts the tasks of that round, so a fight never has two
/// emissions, and it starts over from the database.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_panic_of_the_loop_leaves_one_emission() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = closed_fight(&pool, &vector).await;
    let (_, live_at) = f::hours(T0);
    let sink = Arc::new(RecordingSink::default());
    let probe = Arc::new(PanicOnce::default());
    let (settings, clock) = f::settings_at(live_at);
    let settings = FightSettings {
        sink: sink.clone(),
        probe: probe.clone(),
        ..settings
    };
    let (shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Live).await;
    clock.set(live_at + 1000);
    wait_for("the first events", || async { !sink.events().is_empty() }).await;

    probe.armed.store(true, Ordering::SeqCst);
    // The next pass is at the reread; the fight is still on air then or just finished.
    clock.advance(60_000);
    wait_for("the probe", || async {
        !probe.armed.load(Ordering::SeqCst)
    })
    .await;
    wait_for("the new loop", || async {
        shared.current().halted == Some(false)
    })
    .await;
    clock.set(live_at + 100_000);
    wait_state(&pool, id, FightState::Settled).await;

    // After the last batch that starts over, one sequence with every event: the only one left.
    let batches = sink.batches.lock().unwrap().clone();
    let restart = batches.iter().rposition(|(_, from, _)| *from == 0).unwrap();
    assert!(restart > 0, "the emission did not start over");
    let second: Vec<_> = batches[restart..]
        .iter()
        .flat_map(|(_, _, events)| events.clone())
        .collect();
    assert_eq!(second, vector.log.events, "two emissions at once");
    assert_is_the_vector(&row(&pool, id).await, &vector);
}
