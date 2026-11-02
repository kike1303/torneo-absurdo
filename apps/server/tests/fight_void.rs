//! 012 RF16 to RF18 and RF24, and the base of 010 RF59: `server fight void` in process, with the real
//! database. Every refusal of RF17 writes nothing, in the order of the table, and every reason refunds.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use protocol::{FightFinal, FightState, VoidReason};
use server::admin::{BinaryTag, VoidCommand, VoidFailure, run_fight_void};
use server::bets::BetsHook;
use server::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use server::fights::sealed::{ClosedWindow, ComputeError, RealCompute, SealedSeed, VoidCompute};
use server::fights::snapshot::FightSnapshot;
use server::ledger::{self, Coins};
use sqlx::{PgConnection, PgPool, Row};
use support::bets::{self, written};
use support::db::{TestDb, privileged};
use support::fights::{self as f, DRAW, KO, T0, Vector, row, wait_for};
use uuid::Uuid;

struct World {
    db: TestDb,
    pool: PgPool,
    admin: Uuid,
}

async fn world() -> World {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = bets::user(&pool).await;
    privileged(
        &pool,
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
    )
    .await;
    World { db, pool, admin }
}

/// The engine, failing as it hardly ever does.
struct Fails(ComputeError);

impl VoidCompute for Fails {
    fn compute(
        &self,
        seed: SealedSeed,
        _: &FightSnapshot,
        _: &ClosedWindow,
    ) -> Result<(), (SealedSeed, ComputeError)> {
        Err((seed, self.0))
    }
}

struct Panics;

impl VoidCompute for Panics {
    fn compute(
        &self,
        _: SealedSeed,
        _: &FightSnapshot,
        _: &ClosedWindow,
    ) -> Result<(), (SealedSeed, ComputeError)> {
        panic!("the test engine");
    }
}

type Change = Box<dyn Fn(&mut f::RawFight)>;
type Case = (fn(&mut f::RawFight), Arc<dyn VoidCompute>, bool, VoidReason);

const EVENT_LIMIT: ComputeError = ComputeError::Engine("event_limit");

fn command(id: i32, by: Uuid, previous: bool) -> VoidCommand {
    VoidCommand {
        fight: id,
        by,
        previous_binary: previous.then(|| BinaryTag::parse("v1.2.3").unwrap()),
    }
}

impl World {
    async fn void_with(
        &self,
        command: &VoidCommand,
        compute: Arc<dyn VoidCompute>,
        hook: &dyn SettlementHook,
    ) -> Result<(), VoidFailure> {
        run_fight_void(&self.db.options(), command, compute, hook).await
    }

    async fn void(
        &self,
        id: i32,
        compute: Arc<dyn VoidCompute>,
        previous: bool,
    ) -> Result<(), VoidFailure> {
        self.void_with(&command(id, self.admin, previous), compute, &BetsHook)
            .await
    }

    /// It refuses with `code`, and nothing at all was written.
    async fn refused(
        &self,
        id: i32,
        compute: Arc<dyn VoidCompute>,
        previous: bool,
        code: &'static str,
    ) {
        let before = written(&self.pool).await;
        assert_eq!(
            self.void(id, compute, previous).await,
            Err(VoidFailure::Refused(code)),
            "{code}"
        );
        assert_eq!(written(&self.pool).await, before, "{code}");
    }

    /// A fight with these specs and these changes, in `betting_closed`.
    async fn closed_with(
        &self,
        slot: i64,
        vector: &Vector,
        change: impl FnOnce(&mut f::RawFight),
    ) -> i32 {
        let opens = T0 + slot * f::CADENCE;
        let id = f::announce_with(&self.pool, opens, &vector.a, &vector.b, change).await;
        f::open_with(&self.pool, id, &vector.seed, opens).await;
        f::close_by_sql(&self.pool, id, opens + f::WINDOW).await;
        id
    }

    async fn closed(&self, slot: i64, vector: &Vector) -> i32 {
        self.closed_with(slot, vector, |_| {}).await
    }
}

fn real() -> Arc<dyn VoidCompute> {
    Arc::new(RealCompute)
}

/// Row 1: `--by` that does not exist, or is not an admin. A banned or an unknown id never gets a say.
#[tokio::test]
async fn only_an_admin_can_be_named() {
    let w = world().await;
    let id = w.closed(0, &Vector::of(KO)).await;
    let user = bets::user(&w.pool).await;
    for by in [Uuid::new_v4(), user] {
        let before = written(&w.pool).await;
        assert_eq!(
            w.void_with(&command(id, by, false), real(), &BetsHook)
                .await,
            Err(VoidFailure::Refused("void.not_admin"))
        );
        assert_eq!(written(&w.pool).await, before);
    }
    // It comes before the existence of the fight.
    assert_eq!(
        w.void_with(&command(i32::MAX, user, false), real(), &BetsHook)
            .await,
        Err(VoidFailure::Refused("void.not_admin"))
    );
}

/// Rows 2 and 3: a fight that does not exist, and every state but `betting_closed` and `live`.
#[tokio::test]
async fn a_fight_must_exist_and_be_past_its_window() {
    let w = world().await;
    w.refused(i32::MAX, real(), false, "void.not_found").await;
    let vector = Vector::of(KO);
    let scheduled = f::announce(&w.pool, T0, &vector.a, &vector.b).await;
    w.refused(scheduled, real(), false, "void.bad_state").await;
    let open = f::open_vector(&w.pool, T0 + f::CADENCE, &vector).await;
    w.refused(open, real(), false, "void.bad_state").await;
    // A settled one, voided by the command itself: a second run writes nothing.
    let bad = w
        .closed_with(2, &vector, |raw| raw.snapshot_sha256 = vec![0; 32])
        .await;
    w.void(bad, real(), false).await.unwrap();
    w.refused(bad, real(), false, "void.bad_state").await;
}

/// Row 4, and it comes first: with a secret lost or different too, the version wins.
#[tokio::test]
async fn a_missing_version_comes_before_the_secret() {
    let w = world().await;
    let vector = Vector::of(KO);
    let version = |raw: &mut f::RawFight| raw.ruleset_version = 999;
    let alone = w.closed_with(0, &vector, version).await;
    w.refused(alone, real(), true, "void.version_missing").await;
    let lost = w.closed_with(1, &vector, version).await;
    f::lose_secret(&w.pool, lost).await;
    w.refused(lost, real(), true, "void.version_missing").await;
    let other = w.closed_with(2, &vector, version).await;
    f::replace_secret(&w.pool, other, &[9; 32]).await;
    w.refused(other, real(), true, "void.version_missing").await;
}

/// Row 5: a secret that is not the one of the commit never gets past, with a bad snapshot either.
#[tokio::test]
async fn a_secret_that_differs_is_refused() {
    let w = world().await;
    let vector = Vector::of(KO);
    let id = w.closed(0, &vector).await;
    f::replace_secret(&w.pool, id, &[9; 32]).await;
    w.refused(id, real(), true, "void.secret_mismatch").await;
    let bad = w
        .closed_with(1, &vector, |raw| raw.snapshot_sha256 = vec![0; 32])
        .await;
    f::replace_secret(&w.pool, bad, &[9; 32]).await;
    w.refused(bad, real(), true, "void.secret_mismatch").await;
}

/// Until `secret_lost`, a secret that is gone is one more refusal.
#[tokio::test]
async fn a_secret_that_is_gone_is_refused() {
    let w = world().await;
    let id = w.closed(0, &Vector::of(KO)).await;
    f::lose_secret(&w.pool, id).await;
    w.refused(id, real(), true, "void.secret_missing").await;
}

/// Row 9: a hash or a slug that differ, on the bytes and without the engine. Bytes that do not even
/// decode with a hash that differs are this row, not the problem of a binary.
#[tokio::test]
async fn a_snapshot_that_differs_from_its_bytes_is_voided() {
    let w = world().await;
    let vector = Vector::of(KO);
    let changes: [fn(&mut f::RawFight); 4] = [
        |raw| raw.snapshot_sha256 = vec![0; 32],
        |raw| raw.spec_a = b"{".to_vec(),
        |raw| raw.slug_a = "other".to_owned(),
        |raw| raw.slug_b = "other".to_owned(),
    ];
    for (slot, change) in (0_i64..).zip(changes) {
        let id = w.closed_with(slot, &vector, change).await;
        w.void(id, real(), false).await.unwrap();
        let fight = row(&w.pool, id).await;
        assert_eq!(fight.void_reason.as_deref(), Some("snapshot_invalid"));
        assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
    }
}

/// Row 10: the bytes are intact, and it still does not run. The exit is the previous version.
#[tokio::test]
async fn a_problem_of_the_binary_is_refused() {
    let w = world().await;
    let vector = Vector::of(KO);
    let rehash = |raw: &mut f::RawFight| {
        raw.snapshot_sha256 =
            server::fights::snapshot::snapshot_sha256(1, &raw.spec_a, &raw.spec_b).to_vec();
    };
    let changes: [Change; 5] = [
        Box::new(move |raw| {
            raw.spec_a = b"{".to_vec();
            rehash(raw);
        }),
        Box::new(move |raw| {
            raw.spec_a = [b" ".as_slice(), &raw.spec_a].concat();
            rehash(raw);
        }),
        Box::new(|raw| raw.max_hp_a += 1),
        Box::new(|raw| raw.time_limit_ms += 1),
        Box::new(|raw| raw.max_duration_ms += 1),
    ];
    for (slot, change) in (0_i64..).zip(changes) {
        let id = w.closed_with(slot, &vector, |raw| change(raw)).await;
        w.refused(id, real(), true, "void.binary_problem").await;
    }
    // A spec that decodes and does not pass `validate`: it did before the commit.
    let mut invalid = Vector::of(KO);
    invalid.a.stats.hp = 0;
    let id = w.closed(5, &invalid).await;
    w.refused(id, real(), true, "void.binary_problem").await;
}

/// Row 11: a healthy fight, in both states, with stakes on both sides too: it can rebuild its log, so
/// a fight that cannot settle in phase A is not a case for this exit (RF7).
#[tokio::test]
async fn a_healthy_fight_is_refused() {
    let w = world().await;
    let vector = Vector::of(KO);
    let closed = w.closed(0, &vector).await;
    w.refused(closed, real(), true, "void.fight_healthy").await;
    let opens = T0 + f::CADENCE;
    let live = f::open_vector(&w.pool, opens, &vector).await;
    let user = bets::funded_user(&w.pool, 100).await;
    let other = bets::funded_user(&w.pool, 100).await;
    bets::seed_bet(&w.pool, live, user, "a", 10).await;
    bets::seed_bet(&w.pool, live, other, "b", 10).await;
    f::close_by_sql(&w.pool, live, opens + f::WINDOW).await;
    f::live_by_sql(&w.pool, live, opens + f::WINDOW + f::PRE_LIVE).await;
    w.refused(live, real(), true, "void.fight_healthy").await;
}

/// Rows 12 to 14: the engine fails. `log_without_end` is refused by default, the others ask for the
/// previous binary first, and any other code of the engine is not an exit (deviation D5).
#[tokio::test]
async fn a_failure_of_the_engine_needs_the_previous_binary() {
    let w = world().await;
    let vector = Vector::of(KO);
    let id = w.closed(0, &vector).await;
    let fake = |error| -> Arc<dyn VoidCompute> { Arc::new(Fails(error)) };
    w.refused(id, fake(ComputeError::NoEnd), false, "void.no_end")
        .await;
    w.refused(id, fake(ComputeError::NoEnd), true, "void.no_end")
        .await;
    w.refused(
        id,
        fake(ComputeError::Engine("stat_out_of_range")),
        true,
        "void.binary_problem",
    )
    .await;
    w.refused(
        id,
        fake(EVENT_LIMIT),
        false,
        "void.previous_binary_not_tried",
    )
    .await;
    w.refused(
        id,
        fake(ComputeError::Encode),
        false,
        "void.previous_binary_not_tried",
    )
    .await;
    w.refused(
        id,
        Arc::new(Panics),
        false,
        "void.previous_binary_not_tried",
    )
    .await;

    let failures: [Arc<dyn VoidCompute>; 3] = [
        fake(EVENT_LIMIT),
        fake(ComputeError::Encode),
        Arc::new(Panics),
    ];
    for (slot, compute) in (1_i64..).zip(failures) {
        let id = w.closed(slot, &vector).await;
        w.void(id, compute, true).await.unwrap();
        let fight = row(&w.pool, id).await;
        assert_eq!(fight.void_reason.as_deref(), Some("engine_error"));
        // A panic loses the seed with its task, and it is read again to publish it.
        assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
        assert_eq!(fight.log, None);
    }
}

async fn audit(pool: &PgPool, id: i32) -> (String, Option<Uuid>, String, serde_json::Value) {
    let rows = sqlx::query(
        "SELECT actor, admin_id, action, payload::text FROM admin_audit WHERE target = $1",
    )
    .bind(id.to_string())
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    (
        rows[0].get(0),
        rows[0].get(1),
        rows[0].get(2),
        serde_json::from_str(rows[0].get::<&str, _>(3)).unwrap(),
    )
}

/// RF18: each reason with bets of several users: the exact refund, one row of audit, the seed
/// published and its row gone, and the row of the final as the protocol reads it.
#[tokio::test]
async fn a_void_refunds_audits_and_publishes() {
    let w = world().await;
    let vector = Vector::of(KO);
    let (a, b) = (
        bets::funded_user(&w.pool, 1000).await,
        bets::funded_user(&w.pool, 1000).await,
    );
    let cases: [Case; 2] = [
        (
            |raw| raw.snapshot_sha256 = vec![0; 32],
            real(),
            false,
            VoidReason::SnapshotInvalid,
        ),
        (
            |_| {},
            Arc::new(Fails(EVENT_LIMIT)),
            true,
            VoidReason::EngineError,
        ),
    ];
    for (slot, (change, compute, previous, reason)) in (0_i64..).zip(cases) {
        let opens = T0 + slot * f::CADENCE;
        let id = f::announce_with(&w.pool, opens, &vector.a, &vector.b, change).await;
        f::open_with(&w.pool, id, &vector.seed, opens).await;
        for (user, side, stake) in [(a, "a", 7), (a, "a", 5), (b, "b", 9)] {
            bets::seed_bet(&w.pool, id, user, side, stake).await;
        }
        f::close_by_sql(&w.pool, id, opens + f::WINDOW).await;
        assert_eq!(bets::escrow(&w.pool).await, 21);
        w.void(id, compute, previous).await.unwrap();

        assert_eq!(ledger::balance(&w.pool, a).await.unwrap(), Coins(1000));
        assert_eq!(ledger::balance(&w.pool, b).await.unwrap(), Coins(1000));
        assert_eq!(bets::escrow(&w.pool).await, 0);
        assert_eq!(bets::total(&w.pool).await, 0);
        let fight = row(&w.pool, id).await;
        assert_eq!(fight.state, FightState::Settled);
        assert_eq!(fight.final_result(), Some(FightFinal::Voided { reason }));
        assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
        assert_eq!(fight.log, None);
        assert_eq!(f::secrets_of(&w.pool, id).await, 0);
        let outcome: String =
            sqlx::query_scalar("SELECT outcome FROM pool_settlements WHERE fight_id = $1")
                .bind(id)
                .fetch_one(&w.pool)
                .await
                .unwrap();
        assert_eq!(outcome, "refund_voided");

        let (actor, admin, action, payload) = audit(&w.pool, id).await;
        assert_eq!(actor, "cli_attributed");
        assert_eq!(admin, Some(w.admin));
        assert_eq!(action, "fight_void");
        assert_eq!(payload["reason"], reason.as_str());
        assert_eq!(payload["bets_refunded"], 3);
        assert_eq!(payload["coins_refunded"], 21);
        if previous {
            assert_eq!(payload["previous_binary"], "v1.2.3");
        } else {
            assert!(payload.get("previous_binary").is_none());
        }
    }
}

/// A fight without bets is voided too, with nothing to refund.
#[tokio::test]
async fn a_void_without_bets_refunds_nothing() {
    let w = world().await;
    let id = w
        .closed_with(0, &Vector::of(DRAW), |raw| {
            raw.snapshot_sha256 = vec![0; 32]
        })
        .await;
    w.void(id, real(), false).await.unwrap();
    let (_, _, _, payload) = audit(&w.pool, id).await;
    assert_eq!(payload["bets_refunded"], 0);
    assert_eq!(payload["coins_refunded"], 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pool_settlements")
            .fetch_one(&w.pool)
            .await
            .unwrap(),
        0
    );
}

/// The real refund, and then a failure: it is not a refusal, and nothing of it stays.
struct WritesAndFails;

impl SettlementHook for WritesAndFails {
    fn settle<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        Box::pin(async move {
            BetsHook.settle(&mut *tx, fight).await?;
            Err(SettlementError { kind: "test_kind" })
        })
    }
}

/// RF18: a hook that fails reverts everything, the final and the secret too.
#[tokio::test]
async fn a_hook_that_fails_reverts_everything() {
    let w = world().await;
    let vector = Vector::of(KO);
    let opens = T0;
    let id = f::announce_with(&w.pool, opens, &vector.a, &vector.b, |raw| {
        raw.snapshot_sha256 = vec![0; 32];
    })
    .await;
    f::open_with(&w.pool, id, &vector.seed, opens).await;
    let user = bets::funded_user(&w.pool, 100).await;
    bets::seed_bet(&w.pool, id, user, "a", 10).await;
    f::close_by_sql(&w.pool, id, opens + f::WINDOW).await;
    let before = written(&w.pool).await;
    assert_eq!(
        w.void_with(&command(id, w.admin, false), real(), &WritesAndFails)
            .await,
        Err(VoidFailure::Failed("test_kind"))
    );
    assert_eq!(written(&w.pool).await, before);
    assert_eq!(f::secrets_of(&w.pool, id).await, 1);
}

/// 010 RF59, "Después": the runner sees it in its next read: the halt goes, the final is on the board
/// and the next fight goes on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_runner_sees_a_void_and_goes_on() {
    let w = world().await;
    let vector = Vector::of(KO);
    let id = w
        .closed_with(0, &vector, |raw| raw.snapshot_sha256 = vec![0; 32])
        .await;
    let (settings, clock) = f::settings_at(T0 + f::WINDOW + f::PRE_LIVE);
    let (shared, _runner) = f::start(&w.pool, &settings);
    wait_for("the halt", || async {
        shared.current().halted == Some(true)
    })
    .await;
    w.void(id, real(), false).await.unwrap();
    wait_for("the end of the halt", || async {
        clock.advance(2000);
        shared.current().halted == Some(false)
    })
    .await;
    wait_for("the final on the board", || async {
        clock.advance(2000);
        shared.current().last.is_some_and(|last| {
            last.id == u32::try_from(id).unwrap()
                && last.final_result
                    == Some(FightFinal::Voided {
                        reason: VoidReason::SnapshotInvalid,
                    })
        })
    })
    .await;
}
