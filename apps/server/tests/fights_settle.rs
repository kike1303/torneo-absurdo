//! 010 RF9, RF18, RF19 and RF31: a final is written once, with the hook inside its transaction. A
//! hook that fails, panics or dies reverts the reveal too, and a retry leaves everything once.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use protocol::{CancelReason, FightFinal, FightState};
use server::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use server::fights::sealed::{self, SealedLog, SealedSeed};
use server::fights::snapshot::FightSnapshot;
use server::fights::{FightSettings, FightsShared, PassProbe, store};
use sqlx::{PgConnection, PgPool};
use support::db::TestDb;
use support::fights::{
    self as f, CADENCE, DRAW, KO, T0, Vector, WINDOW, row, wait_for, wait_state,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ok,
    Fail,
    Panic,
    Hang,
    /// Holds its transaction open until the test releases it.
    Hold,
}

/// Counts the passes of the runner. The result of a task is taken before the next pass, so a pass
/// after a call to the hook means the runner saw how the call ended.
#[derive(Default)]
struct Passes(AtomicU32);

impl PassProbe for Passes {
    fn before_pass(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Writes a row in the settling transaction, and then does what it was told.
struct TestHook {
    mode: std::sync::Mutex<Mode>,
    calls: AtomicU32,
    entered: AtomicBool,
    passes: Arc<Passes>,
    passes_at_call: AtomicU32,
    release: tokio::sync::Notify,
}

impl TestHook {
    fn new(mode: Mode) -> Arc<Self> {
        Arc::new(Self {
            mode: std::sync::Mutex::new(mode),
            calls: AtomicU32::new(0),
            entered: AtomicBool::new(false),
            passes: Arc::new(Passes::default()),
            passes_at_call: AtomicU32::new(0),
            release: tokio::sync::Notify::new(),
        })
    }

    fn set(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }
}

impl SettlementHook for TestHook {
    fn settle<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        Box::pin(async move {
            self.passes_at_call
                .store(self.passes.0.load(Ordering::SeqCst), Ordering::SeqCst);
            self.calls.fetch_add(1, Ordering::SeqCst);
            sqlx::query("INSERT INTO hook_log (fight_id, final) VALUES ($1, $2)")
                .bind(fight.id)
                .bind(serde_json::to_string(&fight.final_result).unwrap())
                .execute(&mut *tx)
                .await?;
            self.entered.store(true, Ordering::SeqCst);
            let mode = *self.mode.lock().unwrap();
            match mode {
                Mode::Ok => Ok(()),
                Mode::Fail => Err(SettlementError { kind: "test" }),
                Mode::Panic => panic!("the test hook"),
                Mode::Hold => {
                    self.release.notified().await;
                    Ok(())
                }
                Mode::Hang => {
                    std::future::pending::<()>().await;
                    Ok(())
                }
            }
        })
    }
}

async fn database() -> (TestDb, PgPool) {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    sqlx::query("CREATE TABLE hook_log (fight_id int NOT NULL, final text NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    (db, pool)
}

async fn hook_rows(pool: &PgPool, id: i32) -> Vec<String> {
    sqlx::query_scalar("SELECT final FROM hook_log WHERE fight_id = $1")
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A fight of `vector` in `live`, with `settle_at` in reach of the clock.
async fn live_fight(pool: &PgPool, vector: &Vector) -> (i32, i64) {
    let id = f::open_vector(pool, T0, vector).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(pool, id, closes_at).await;
    f::live_by_sql(pool, id, live_at).await;
    (id, live_at)
}

fn with_hook(settings: FightSettings, hook: &Arc<TestHook>) -> FightSettings {
    FightSettings {
        hook: hook.clone(),
        probe: hook.passes.clone(),
        ..settings
    }
}

/// Until the hook ran `at_least` times, and the runner saw how it ended: the hour of a failure is
/// the clock when the runner sees it, so a test must not move it before that.
async fn wait_calls(hook: &TestHook, at_least: u32) {
    wait_for("the hook", || async {
        hook.calls.load(Ordering::SeqCst) >= at_least
            && hook.passes.0.load(Ordering::SeqCst) > hook.passes_at_call.load(Ordering::SeqCst)
    })
    .await;
}

fn assert_still_live(fight: &server::fights::state::FightRow) {
    assert_eq!(fight.state, FightState::Live);
    assert_eq!(fight.revealed_seed, None);
    assert_eq!(fight.log, None);
    assert_eq!(fight.final_result(), None);
}

/// RF18 and RF31: a hook that fails reverts everything, a short failure does not halt, 60 s of them
/// do, and once it works there is one final and one row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_hook_reverts_the_reveal_and_halts_after_a_minute() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let (id, live_at) = live_fight(&pool, &vector).await;
    let hook = TestHook::new(Mode::Fail);
    let (settings, clock) = f::settings_at(live_at + 100_000);
    let (shared, _runner) = f::start(&pool, &with_hook(settings, &hook));

    wait_calls(&hook, 1).await;
    wait_for("the evaluation", || async {
        shared.current().halted.is_some()
    })
    .await;
    assert_still_live(&row(&pool, id).await);
    assert_eq!(f::secrets_of(&pool, id).await, 1);
    assert!(hook_rows(&pool, id).await.is_empty());
    // A failure of less than 60 s is no halt: the live goes on.
    clock.advance(59_000);
    wait_calls(&hook, 2).await;
    assert_eq!(shared.current().halted, Some(false));

    // A new fight during the failure: at 60 s no window opens.
    let next = f::announce(&pool, clock.now_ms() + 2000, &vector.a, &vector.b).await;
    clock.advance(1100);
    wait_for("the halt", || async {
        shared.current().halted == Some(true)
    })
    .await;
    clock.advance(70_000);
    wait_calls(&hook, 3).await;
    assert_eq!(row(&pool, next).await.state, FightState::Scheduled);
    assert_still_live(&row(&pool, id).await);

    // It is fixed: the final, once, and the halt ends.
    hook.set(Mode::Ok);
    clock.advance(60_000);
    wait_state(&pool, id, FightState::Settled).await;
    wait_for("the end of the halt", || async {
        shared.current().halted == Some(false)
    })
    .await;
    let fight = row(&pool, id).await;
    assert_eq!(fight.log.as_deref(), Some(vector.canonical().as_slice()));
    assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
    assert_eq!(f::secrets_of(&pool, id).await, 0);
    assert_eq!(hook_rows(&pool, id).await.len(), 1);
}

/// RF18: a panic in the hook is one more `settle_failed`, and never kills the runner in silence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hook_that_panics_is_one_more_failure() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let (id, live_at) = live_fight(&pool, &vector).await;
    let hook = TestHook::new(Mode::Panic);
    let (settings, clock) = f::settings_at(live_at + 100_000);
    let (_shared, _runner) = f::start(&pool, &with_hook(settings, &hook));
    wait_calls(&hook, 1).await;
    clock.advance(1100);
    wait_calls(&hook, 2).await;
    assert_still_live(&row(&pool, id).await);
    hook.set(Mode::Ok);
    clock.advance(10_000);
    wait_state(&pool, id, FightState::Settled).await;
    assert_eq!(hook_rows(&pool, id).await.len(), 1);
}

/// RF18: a kill before the `COMMIT` leaves `live`, and the next run settles it once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kill_before_the_commit_leaves_it_live() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let (id, live_at) = live_fight(&pool, &vector).await;
    let hook = TestHook::new(Mode::Hang);
    let (settings, _clock) = f::settings_at(live_at + 100_000);
    let settings = with_hook(settings, &hook);
    let (_shared, runner) = f::start(&pool, &settings);
    wait_for("the hook", || async { hook.entered.load(Ordering::SeqCst) }).await;
    runner.stop().await;
    // The connection of the attempt may take a moment to give back its transaction.
    wait_for("the rollback", || async {
        hook_rows(&pool, id).await.is_empty()
    })
    .await;
    assert_still_live(&row(&pool, id).await);
    assert_eq!(f::secrets_of(&pool, id).await, 1);

    hook.set(Mode::Ok);
    let (_shared, _runner) = f::start(&pool, &settings);
    wait_state(&pool, id, FightState::Settled).await;
    assert_eq!(hook_rows(&pool, id).await.len(), 1);
    assert_eq!(
        row(&pool, id).await.log.as_deref(),
        Some(vector.canonical().as_slice())
    );
}

/// RF18 and RF59: with the row already settled by another process, an attempt is no failure: it
/// closes the emission and stays quiet. The hook does not run a second time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_row_settled_by_another_process_is_no_failure() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let (id, live_at) = live_fight(&pool, &vector).await;
    let hook = TestHook::new(Mode::Ok);
    let (settings, clock) = f::settings_at(live_at + 1000);
    let (shared, _runner) = f::start(&pool, &with_hook(settings, &hook));
    wait_for("the evaluation", || async {
        shared.current().current.is_some()
    })
    .await;

    // What the CLI of TA-27 will do.
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, id, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();

    // Past its settle_at, with the row gone from under it.
    clock.set(live_at + 100_000);
    wait_for("the board", || async {
        let current = shared.current();
        current.current.is_none()
            && current
                .last
                .is_some_and(|last| matches!(last.final_result, Some(FightFinal::Voided { .. })))
    })
    .await;
    assert_eq!(hook.calls.load(Ordering::SeqCst), 0);
    assert_eq!(shared.current().halted, Some(false));
    assert!(row(&pool, id).await.log.is_none());
}

/// The final of `vector`, straight from the types of the runner.
async fn final_of(pool: &PgPool, id: i32, live_at: i64) -> server::fights::sealed::FinalLog {
    let fight = row(pool, id).await;
    let snapshot = FightSnapshot::from_row(&fight)
        .map_err(|e| e.code())
        .unwrap();
    let window = store::read_closed_window(pool, id).await.unwrap().unwrap();
    let mut connection = pool.acquire().await.unwrap();
    let seed = SealedSeed::load(&mut connection, id, fight.seed_commit.as_ref().unwrap())
        .await
        .map_err(|_| "seed")
        .unwrap();
    let mut log = SealedLog::compute(seed, &snapshot, &window)
        .map_err(|_| "compute")
        .unwrap();
    let _ = log.release_due(u32::MAX);
    log.into_final(live_at).map_err(|_| "final").unwrap()
}

/// RF9: two processes compete to close a fight: one final, and one effective call to the hook.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_processes_compete_for_the_final() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let (id, live_at) = live_fight(&pool, &vector).await;
    let hook = TestHook::new(Mode::Ok);
    let shared_final = Arc::new(final_of(&pool, id, live_at).await);

    let attempt = |pool: PgPool, hook: Arc<TestHook>, final_log: Arc<sealed::FinalLog>| async move {
        let mut tx = pool.begin().await.unwrap();
        if !sealed::settle_decided(&mut tx, &final_log, live_at + 100_000)
            .await
            .unwrap()
        {
            return false;
        }
        let fight = SettlingFight {
            id,
            final_result: final_log.final_result(),
            closes_at: live_at,
        };
        hook.settle(&mut tx, &fight).await.unwrap();
        tx.commit().await.unwrap();
        true
    };
    let (one, other) = tokio::join!(
        tokio::spawn(attempt(pool.clone(), hook.clone(), shared_final.clone())),
        tokio::spawn(attempt(pool.clone(), hook.clone(), shared_final.clone())),
    );
    let wins = [one.unwrap(), other.unwrap()];
    assert_eq!(wins.iter().filter(|won| **won).count(), 1, "{wins:?}");
    assert_eq!(hook.calls.load(Ordering::SeqCst), 1);
    assert_eq!(hook_rows(&pool, id).await.len(), 1);

    // And rewriting it fails at the database.
    let error = sqlx::query("UPDATE fights SET winner = NULL, end_reason = 'draw' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(server::db::error_kind(&error), "fight_guard");
}

/// RF31: for each final the hook writes in the same transaction, and a failure leaves nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_hook_sees_every_kind_of_final() {
    let (_db, pool) = database().await;

    // decided, with a winner and with a draw.
    for (slot, spec) in [(0_i64, KO), (1, DRAW)] {
        let vector = Vector::of(spec);
        let opens_at = T0 + slot * CADENCE;
        let id = f::open_vector(&pool, opens_at, &vector).await;
        let (closes_at, live_at) = f::hours(opens_at);
        f::close_by_sql(&pool, id, closes_at).await;
        f::live_by_sql(&pool, id, live_at).await;
        let hook = TestHook::new(Mode::Fail);
        let (settings, clock) = f::settings_at(live_at + 100_000);
        let (_shared, runner) = f::start(&pool, &with_hook(settings, &hook));
        wait_calls(&hook, 1).await;
        assert_still_live(&row(&pool, id).await);
        assert!(hook_rows(&pool, id).await.is_empty());
        hook.set(Mode::Ok);
        clock.advance(10_000);
        wait_state(&pool, id, FightState::Settled).await;
        runner.stop().await;
        let rows = hook_rows(&pool, id).await;
        let expected = row(&pool, id).await.final_result().unwrap();
        assert_eq!(rows, [serde_json::to_string(&expected).unwrap()]);
    }

    // cancelled: the hook is in the transaction too, and a failure leaves it scheduled.
    let vector = Vector::of(KO);
    let opens_at = T0 + 2 * CADENCE;
    let id = f::announce(&pool, opens_at, &vector.a, &vector.b).await;
    let hook = TestHook::new(Mode::Fail);
    let (settings, clock) = f::settings_at(opens_at + WINDOW + 1);
    let (_shared, runner) = f::start(&pool, &with_hook(settings, &hook));
    wait_calls(&hook, 1).await;
    assert_eq!(row(&pool, id).await.state, FightState::Scheduled);
    assert!(hook_rows(&pool, id).await.is_empty());
    hook.set(Mode::Ok);
    clock.advance(10_000);
    wait_state(&pool, id, FightState::Settled).await;
    runner.stop().await;
    let cancelled = FightFinal::Cancelled {
        reason: CancelReason::MissedWindow,
    };
    assert_eq!(
        hook_rows(&pool, id).await,
        [serde_json::to_string(&cancelled).unwrap()]
    );

    // voided: nothing in `src` writes it, so a test transaction plays the CLI of TA-27.
    let opens_at = T0 + 3 * CADENCE;
    let id = f::open_vector(&pool, opens_at, &vector).await;
    let (closes_at, _) = f::hours(opens_at);
    f::close_by_sql(&pool, id, closes_at).await;
    let final_result = FightFinal::Voided {
        reason: protocol::VoidReason::EngineError,
    };
    let fight = SettlingFight {
        id,
        final_result,
        closes_at,
    };
    let void = |mode: Mode| {
        let pool = pool.clone();
        let hook = TestHook::new(mode);
        let seed = vector.seed;
        async move {
            let mut tx = pool.begin().await.unwrap();
            f::void_by_sql(&mut tx, id, &seed, "engine_error").await;
            let settled = hook.settle(&mut tx, &fight).await;
            if settled.is_ok() {
                tx.commit().await.unwrap();
            }
        }
    };
    void(Mode::Fail).await;
    assert_eq!(row(&pool, id).await.state, FightState::BettingClosed);
    assert_eq!(row(&pool, id).await.revealed_seed, None);
    assert!(hook_rows(&pool, id).await.is_empty());
    assert_eq!(f::secrets_of(&pool, id).await, 1);
    void(Mode::Ok).await;
    assert_eq!(
        hook_rows(&pool, id).await,
        [serde_json::to_string(&final_result).unwrap()]
    );
    assert_eq!(row(&pool, id).await.final_result(), Some(final_result));
}

/// RF16: a hook that fails during several slots: no new window, and when it is fixed, one final per
/// fight and no marathon.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_long_failure_opens_no_windows_and_leaves_one_final_per_fight() {
    let (_db, pool) = database().await;
    let vector = Vector::of(KO);
    let (id, live_at) = live_fight(&pool, &vector).await;
    let hook = TestHook::new(Mode::Fail);
    let start = live_at + 100_000;
    let (settings, clock) = f::settings_at(start);
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let _runner = f::spawn_runner(&pool, &shared, f::config(), with_hook(settings, &hook));
    wait_calls(&hook, 1).await;
    clock.advance(61_000);
    wait_calls(&hook, 2).await;
    wait_for("the halt", || async {
        shared.current().halted == Some(true)
    })
    .await;
    let rows_during: i64 = sqlx::query_scalar("SELECT count(*) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Three slots go by, with the settlement failing all along.
    for _ in 0..3 {
        clock.advance(CADENCE);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let rows_after: i64 = sqlx::query_scalar("SELECT count(*) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows_after, rows_during, "no new rows during a halt");
    let opened: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fights WHERE seed_commit IS NOT NULL AND id <> $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(opened, 0, "no new windows during a halt");

    hook.set(Mode::Ok);
    clock.advance(CADENCE);
    wait_state(&pool, id, FightState::Settled).await;
    assert_eq!(hook_rows(&pool, id).await.len(), 1);
    wait_for("the end of the halt", || async {
        shared.current().halted == Some(false)
    })
    .await;
    // One announcement ahead, not one per slot that went by. A missed fight whose cancellation is
    // on its way may still count for a moment.
    wait_for("one fight ahead", || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fights WHERE state <> 'settled'")
            .fetch_one(&pool)
            .await
            .unwrap()
            <= 1
    })
    .await;
}

/// RF20, "the pair lost in a round": while the cancellation of a fight is not committed its pair
/// still counts as used, so nothing may be announced. With the hook holding that transaction open
/// this does not depend on who wins a race: the row of the next slot can only be right if it waits.
/// With two characters a round has two pairs: the other one is what must come, in the same round.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_fight_gives_back_its_pair_before_the_next_announcement() {
    let (_db, pool) = database().await;
    let content = f::content_with("pair", 2);
    let ruleset = engine::Ruleset::for_version(1).unwrap();
    let characters = content::load_roster(&content, &ruleset).unwrap();
    let (a, b) = (characters[0].spec().clone(), characters[1].spec().clone());
    let roster = server::fights::matchup::roster_sha256(&[a.slug.clone(), b.slug.clone()]).to_vec();
    let same_roster = |raw: &mut f::RawFight| raw.roster_sha256.clone_from(&roster);

    // X: `(a, b)` in round 1, already settled.
    let first = f::announce_with(&pool, T0 - 2 * CADENCE, &a, &b, same_roster).await;
    f::open_with(&pool, first, &[5; 32], T0 - 2 * CADENCE).await;
    f::close_by_sql(&pool, first, T0 - 2 * CADENCE + WINDOW).await;
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, first, &[5; 32], "engine_error").await;
    tx.commit().await.unwrap();
    // Y: `(b, a)` in round 1, with its window gone.
    let missed = f::announce_with(&pool, T0 - CADENCE, &b, &a, same_roster).await;

    let hook = TestHook::new(Mode::Hold);
    let (settings, _clock) = f::settings_at(T0 - CADENCE + WINDOW + 1);
    let shared = Arc::new(FightsShared::new(settings.clock.clone()));
    let _runner = f::spawn_runner(
        &pool,
        &shared,
        f::config_on(&content),
        with_hook(settings, &hook),
    );

    wait_for("the cancellation", || async {
        hook.entered.load(Ordering::SeqCst)
    })
    .await;
    let rows = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fights")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    // Several passes' worth of time: the cancellation is pending, and nothing is announced.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rows().await, 2, "announced before the pair was given back");

    hook.release.notify_one();
    wait_state(&pool, missed, FightState::Settled).await;
    wait_for("the announcement", || async { rows().await == 3 }).await;
    let id: i32 = sqlx::query_scalar("SELECT max(id) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap();
    let next = row(&pool, id).await;
    assert_eq!((next.slug_a, next.slug_b), (b.slug, a.slug));
    assert_eq!(next.matchup_round, 1);
}
