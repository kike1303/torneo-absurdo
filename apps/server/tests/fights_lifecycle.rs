//! 010 RF1, RF3, RF8, RF12, RF13, RF15, RF19 and RF20 in the runner: fights by themselves, on a
//! manual clock, from the announcement to the final.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use engine::canonical_spec_json;
use protocol::{CancelReason, EndReason, FightFinal, FightState};
use server::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use server::fights::sealed;
use server::fights::snapshot::snapshot_sha256;
use server::fights::state::FightRow;
use server::fights::{FightClock, FightSettings, FightsConfig, FightsShared, store};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use support::db::TestDb;
use support::fights::{
    self as f, CADENCE, DRAW, KO, PRE_LIVE, T0, Vector, WINDOW, drive_to_settled, row, wait_for,
    wait_state,
};

async fn all_rows(pool: &PgPool) -> Vec<i32> {
    sqlx::query_scalar("SELECT id FROM fights ORDER BY opens_at")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn wait_rows(pool: &PgPool, count: usize) -> Vec<i32> {
    wait_for("the rows", || async { all_rows(pool).await.len() >= count }).await;
    all_rows(pool).await
}

fn assert_snapshot_is_the_canonical_one(fight: &FightRow) {
    let decode = |bytes: &[u8]| -> engine::CharacterSpec { serde_json::from_slice(bytes).unwrap() };
    assert_eq!(
        canonical_spec_json(&decode(&fight.spec_a)).unwrap(),
        fight.spec_a
    );
    assert_eq!(
        canonical_spec_json(&decode(&fight.spec_b)).unwrap(),
        fight.spec_b
    );
    assert_eq!(
        snapshot_sha256(1, &fight.spec_a, &fight.spec_b).as_slice(),
        fight.snapshot_sha256
    );
    assert_eq!(fight.ruleset_version, 1);
}

/// The intent's "Sola": with nobody connected, several fights in a row complete their cycle at the
/// cadence, each announced before it opens, each with its own seed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fights_run_by_themselves() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let (settings, clock) = f::settings_at(T0 + 1);
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let _runner = f::spawn_runner(&pool, &shared, f::config(), settings);

    let mut commits = Vec::new();
    for slot in 1..=3 {
        let opens_at = T0 + slot * CADENCE;
        // RF12: announced a cadence ahead, with its snapshot, before any seed exists.
        wait_for("the announcement", || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM fights \
                 WHERE (extract(epoch FROM opens_at) * 1000)::bigint = $1",
            )
            .bind(opens_at)
            .fetch_one(&pool)
            .await
            .unwrap()
                == 1
        })
        .await;
        let id = *all_rows(&pool).await.last().unwrap();
        let announced = row(&pool, id).await;
        assert_eq!(announced.state, FightState::Scheduled);
        assert_eq!(announced.seed_commit, None);
        assert_eq!(f::secrets_of(&pool, id).await, 0);
        assert_snapshot_is_the_canonical_one(&announced);
        assert_ne!(announced.slug_a, announced.slug_b);
        assert_eq!(announced.closes_at, opens_at + WINDOW);
        assert_eq!(announced.live_at, opens_at + WINDOW + PRE_LIVE);

        clock.set(opens_at);
        wait_state(&pool, id, FightState::BettingOpen).await;
        let open = row(&pool, id).await;
        assert_eq!(open.opened_at, Some(opens_at));
        // RF3: the commit of the seed in its table.
        let secret: Vec<u8> =
            sqlx::query_scalar("SELECT seed FROM fight_secrets WHERE fight_id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let commit = open.seed_commit.clone().unwrap();
        assert_eq!(Sha256::digest(&secret).as_slice(), commit);
        commits.push(commit.clone());

        drive_to_settled(&pool, &clock, id).await;
        let settled = row(&pool, id).await;
        assert_eq!(settled.closed_at, Some(open.closes_at));
        assert_eq!(settled.live_started_at, Some(open.live_at));
        assert!(matches!(
            settled.final_result(),
            Some(FightFinal::Decided { .. })
        ));
        // RF18: the reveal, the log, and no secret left.
        assert_eq!(settled.revealed_seed, Some(secret));
        assert!(settled.log.is_some());
        assert_eq!(f::secrets_of(&pool, id).await, 0);

        // RF35: the board follows, from memory.
        wait_for("the board", || async {
            shared
                .current()
                .last
                .is_some_and(|last| last.id == u32::try_from(id).unwrap())
        })
        .await;
        assert_eq!(shared.current().halted, Some(false));
    }
    commits.sort();
    commits.dedup();
    assert_eq!(commits.len(), 3);
}

/// Amendment DP-1: with the smallest grid of worry 9, the fights open and finish. With the fixed 60 s
/// every one would be cancelled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_smallest_grid_runs() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let cadence = 113_000;
    let (settings, clock) = f::settings_at(cadence * 1000 + 1);
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let config = FightsConfig {
        cadence_ms: cadence,
        window_ms: 5000,
        pre_live_ms: 3000,
        ..f::config()
    };
    let _runner = f::spawn_runner(&pool, &shared, config, settings);
    let ids = wait_rows(&pool, 1).await;
    let fight = row(&pool, ids[0]).await;
    assert_eq!(fight.opens_at, cadence * 1001);
    clock.set(fight.opens_at + 2500);
    wait_state(&pool, ids[0], FightState::BettingOpen).await;
    drive_to_settled(&pool, &clock, ids[0]).await;
    assert!(matches!(
        row(&pool, ids[0]).await.final_result(),
        Some(FightFinal::Decided { .. })
    ));
}

/// RF15: with less than the minimum window left, it is cancelled instead of opening late.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_past_its_minimum_window_is_cancelled() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let opens_at = T0 + CADENCE;
    let id = f::announce(&pool, opens_at, &vector.a, &vector.b).await;
    let window = WINDOW;
    let min = 60_000;
    let (settings, _clock) = f::settings_at(opens_at + window - min + 1);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Settled).await;
    assert_eq!(
        row(&pool, id).await.final_result(),
        Some(FightFinal::Cancelled {
            reason: CancelReason::MissedWindow
        })
    );
}

/// RF3: a failure halfway leaves neither a commit nor a new state.
#[tokio::test]
async fn opening_is_one_transaction() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(sealed::open_window(&mut tx, id, T0).await.unwrap());
    tx.rollback().await.unwrap();
    let fight = row(&pool, id).await;
    assert_eq!(fight.state, FightState::Scheduled);
    assert_eq!(fight.seed_commit, None);
    assert_eq!(f::secrets_of(&pool, id).await, 0);

    let mut tx = pool.begin().await.unwrap();
    assert!(sealed::open_window(&mut tx, id, T0).await.unwrap());
    tx.commit().await.unwrap();
    let fight = row(&pool, id).await;
    assert_eq!(fight.state, FightState::BettingOpen);
    let secret: Vec<u8> = sqlx::query_scalar("SELECT seed FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        Sha256::digest(&secret).as_slice(),
        fight.seed_commit.unwrap()
    );

    // A second opening finds no scheduled row, and changes nothing.
    let mut tx = pool.begin().await.unwrap();
    assert!(!sealed::open_window(&mut tx, id, T0).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(f::secrets_of(&pool, id).await, 1);
}

/// RF8: any sequence of repeated or out-of-order transitions ends in the same state, with no visible
/// error: with no row, someone else already did it.
#[test]
fn repeated_or_disordered_transitions_are_no_errors() {
    use proptest::prelude::*;

    #[derive(Debug, Clone, Copy)]
    enum Step {
        Open,
        Close,
        GoLive,
        Cancel,
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let db = runtime.block_on(TestDb::create());
    let pool = runtime.block_on(db.migrated_pool());
    let vector = Vector::of(KO);
    let slot = std::cell::Cell::new(0_i64);
    let strategy = prop::collection::vec(
        prop_oneof![
            Just(Step::Open),
            Just(Step::Close),
            Just(Step::GoLive),
            Just(Step::Cancel),
        ],
        1..12,
    );
    let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig {
        cases: 24,
        ..ProptestConfig::default()
    });
    runner
        .run(&strategy, |steps| {
            let opens_at = T0 + slot.get() * CADENCE;
            slot.set(slot.get() + 1);
            runtime.block_on(async {
                let id = f::announce(&pool, opens_at, &vector.a, &vector.b).await;
                // The model: each transition only applies from its state.
                let mut expected = FightState::Scheduled;
                for step in &steps {
                    match step {
                        Step::Open => {
                            let mut tx = pool.begin().await.unwrap();
                            let done = sealed::open_window(&mut tx, id, opens_at).await.unwrap();
                            tx.commit().await.unwrap();
                            assert_eq!(done, expected == FightState::Scheduled);
                            if done {
                                expected = FightState::BettingOpen;
                            }
                        }
                        Step::Close => {
                            let done = store::close(&pool, id, opens_at, 500).await.unwrap();
                            assert_eq!(done.is_some(), expected == FightState::BettingOpen);
                            if done.is_some() {
                                expected = FightState::BettingClosed;
                            }
                        }
                        Step::GoLive => {
                            let done = store::go_live(&pool, id, opens_at, 500).await.unwrap();
                            assert_eq!(done, expected == FightState::BettingClosed);
                            if done {
                                expected = FightState::Live;
                            }
                        }
                        Step::Cancel => {
                            let mut tx = pool.begin().await.unwrap();
                            let done =
                                store::cancel(&mut tx, id, CancelReason::MissedWindow, opens_at)
                                    .await
                                    .unwrap();
                            tx.commit().await.unwrap();
                            assert_eq!(done, expected == FightState::Scheduled);
                            if done {
                                expected = FightState::Settled;
                            }
                        }
                    }
                    assert_eq!(row(&pool, id).await.state, expected);
                }
            });
            Ok(())
        })
        .unwrap();
    runtime.block_on(pool.close());
    drop(runtime);
}

/// RF19: a cancelled fight has no commit, no seed and no log, and the hook still runs. RF20: its pair
/// is not used up and does not count as recent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missed_fight_is_cancelled_without_a_seed_and_does_not_count() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    // The server was down past its whole window.
    let (settings, _clock) = f::settings_at(T0 + WINDOW + 1);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Settled).await;
    let fight = row(&pool, id).await;
    assert_eq!(
        fight.final_result(),
        Some(FightFinal::Cancelled {
            reason: CancelReason::MissedWindow
        })
    );
    assert_eq!(fight.seed_commit, None);
    assert_eq!(fight.revealed_seed, None);
    assert_eq!(fight.log, None);
    assert_eq!(fight.opened_at, None);
    let context = store::matchup_context(&pool).await.unwrap();
    assert!(context.used.is_empty());
    assert!(context.recent.is_empty());
}

/// RF15: a scheduled fight that cannot run is cancelled before its commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_that_cannot_run_is_cancelled_at_its_opening() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let version = f::announce_with(&pool, T0, &vector.a, &vector.b, |raw| {
        raw.ruleset_version = 999;
    })
    .await;
    let corrupt = f::announce_with(&pool, T0 + CADENCE, &vector.a, &vector.b, |raw| {
        raw.spec_a = b"{".to_vec();
    })
    .await;
    let (settings, clock) = f::settings_at(T0);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, version, FightState::Settled).await;
    clock.set(T0 + CADENCE);
    wait_state(&pool, corrupt, FightState::Settled).await;
    for id in [version, corrupt] {
        let fight = row(&pool, id).await;
        assert_eq!(
            fight.final_result(),
            Some(FightFinal::Cancelled {
                reason: CancelReason::FightUnrunnable
            })
        );
        assert_eq!(fight.seed_commit, None);
    }
}

/// RF15, amendment DP-2: a closed fight waits for `live_at`, and starts exactly then.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_fight_never_skips_its_pre_live() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (closes_at, live_at) = f::hours(T0);
    let (settings, clock) = f::settings_at(closes_at);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::BettingClosed).await;
    clock.set(live_at - 1);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(row(&pool, id).await.state, FightState::BettingClosed);
    clock.set(live_at);
    wait_state(&pool, id, FightState::Live).await;
    assert_eq!(row(&pool, id).await.live_started_at, Some(live_at));
}

/// RF15: past `live_at + 1 s` it starts 10 s from now, and `live_started_at` is written only once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_late_fight_starts_ten_seconds_from_now() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::open_vector(&pool, T0, &vector).await;
    let (_, live_at) = f::hours(T0);
    let late = live_at + 1001;
    let (settings, _clock) = f::settings_at(late);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Live).await;
    assert_eq!(row(&pool, id).await.live_started_at, Some(late + 10_000));
}

/// RF13: going live waits for another fight in `live`, and then takes the late route.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_fight_in_live_at_a_time() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let first = f::open_vector(&pool, T0, &vector).await;
    let (first_closes, first_live) = f::hours(T0);
    f::close_by_sql(&pool, first, first_closes).await;
    f::live_by_sql(&pool, first, first_live).await;
    // A second one, a second behind: opening and closing do not wait for the first.
    let second = f::open_vector(&pool, T0 + 1000, &vector).await;
    let (second_closes, second_live) = f::hours(T0 + 1000);
    let (settings, clock) = f::settings_at(second_closes);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, second, FightState::BettingClosed).await;

    clock.set(second_live);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(row(&pool, first).await.state, FightState::Live);
    assert_eq!(row(&pool, second).await.state, FightState::BettingClosed);

    let after_first = first_live + 100_000;
    clock.set(after_first);
    wait_state(&pool, first, FightState::Settled).await;
    wait_state(&pool, second, FightState::Live).await;
    assert_eq!(
        row(&pool, second).await.live_started_at,
        Some(after_first + 10_000)
    );
}

/// RF12: `UNIQUE (opens_at)`, and the slots that passed without a row are not recovered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_row_per_slot_and_no_marathon() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let (settings, clock) = f::settings_at(T0 + 1);
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let runner = f::spawn_runner(&pool, &shared, f::config(), settings.clone());
    wait_rows(&pool, 1).await;
    runner.stop().await;
    // Down for a day: the fight of that slot is lost, and only the next future slot gets a row.
    clock.set(T0 + 144 * CADENCE + 1);
    let _runner = f::spawn_runner(&pool, &shared, f::config(), settings);
    wait_rows(&pool, 2).await;
    let rows = all_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    let newest = row(&pool, rows[1]).await;
    assert_eq!(newest.opens_at, T0 + 145 * CADENCE);
    wait_state(&pool, rows[0], FightState::Settled).await;
    // Another pass adds nothing.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(all_rows(&pool).await.len(), 2);
    let count: i64 = sqlx::query("SELECT count(DISTINCT opens_at) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 2);
}

/// RF20 in the database: a new roster opens a new round, and the next row uses a pair of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_roster_opens_a_new_round() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(DRAW);
    // A row with `roster_sha256 = [1; 32]`, not the one of the real characters.
    let id = f::announce(&pool, T0 - CADENCE, &vector.a, &vector.b).await;
    assert_eq!(row(&pool, id).await.matchup_round, 1);
    let (settings, clock) = f::settings_at(T0 - CADENCE - 1);
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let _runner = f::spawn_runner(&pool, &shared, f::config(), settings);
    clock.set(T0 - CADENCE);
    wait_rows(&pool, 2).await;
    let rows = all_rows(&pool).await;
    assert_eq!(row(&pool, rows[1]).await.matchup_round, 2);
}

/// Intent, "Final único": a win and a draw are told apart in the public data, and each log is the
/// one of its golden vector, byte by byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_win_and_a_draw_are_told_apart_in_public_data() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let ko = Vector::of(KO);
    let draw = Vector::of(DRAW);
    let (settings, clock): (_, FightClock) = f::settings_at(T0);
    let (_shared, _runner) = f::start(&pool, &settings);
    let first = f::open_vector(&pool, T0 + CADENCE, &ko).await;
    drive_to_settled(&pool, &clock, first).await;
    let second = f::open_vector(&pool, T0 + 100 * CADENCE, &draw).await;
    clock.advance(1);
    drive_to_settled(&pool, &clock, second).await;

    let won = row(&pool, first).await.final_result().unwrap();
    assert!(matches!(
        won,
        FightFinal::Decided {
            winner: Some(_),
            reason: EndReason::Ko
        }
    ));
    assert_eq!(
        row(&pool, second).await.final_result(),
        Some(FightFinal::Decided {
            winner: None,
            reason: EndReason::Draw
        })
    );
    assert_eq!(row(&pool, first).await.log.unwrap(), ko.canonical());
    assert_eq!(row(&pool, second).await.log.unwrap(), draw.canonical());
}

/// The hook of a fight that cannot settle until the test says so. The others go through.
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

/// RF35 (amendment DP-B8): during a halt the next fight loses its window and is cancelled. When the
/// halted one finally settles, it is the last one settled, whatever the order of their slots.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn last_is_the_last_one_settled_not_the_last_slot() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let halted = f::open_vector(&pool, T0, &vector).await;
    let cancelled = f::announce(&pool, T0 + 2 * CADENCE, &vector.a, &vector.b).await;

    let hook = Arc::new(FailsFor {
        id: halted,
        fixed: AtomicBool::new(false),
    });
    let (settings, clock) = f::settings_at(T0);
    let settings = FightSettings {
        hook: hook.clone(),
        ..settings
    };
    let (shared, _runner) = f::start(&pool, &settings);

    // The fight ends, its settlement fails, and the time goes by until the next one is lost.
    let (closes_at, live_at) = f::hours(T0);
    clock.set(closes_at);
    wait_state(&pool, halted, FightState::BettingClosed).await;
    clock.set(live_at);
    wait_state(&pool, halted, FightState::Live).await;
    clock.set(live_at + vector.end_t() + 10_000);
    wait_for("the halt", || async {
        clock.advance(2000);
        shared.current().halted == Some(true)
    })
    .await;
    clock.set(clock.now_ms().max(T0 + 2 * CADENCE + WINDOW));
    wait_state(&pool, cancelled, FightState::Settled).await;
    wait_for("the cancelled one on the board", || async {
        shared
            .current()
            .last
            .is_some_and(|last| last.id == u32::try_from(cancelled).unwrap())
    })
    .await;

    hook.fixed.store(true, Ordering::SeqCst);
    f::wait_state_advancing(&pool, &clock, halted, FightState::Settled).await;
    wait_for("the last one settled on the board", || async {
        clock.advance(2000);
        shared
            .current()
            .last
            .is_some_and(|last| last.id == u32::try_from(halted).unwrap())
    })
    .await;
}
