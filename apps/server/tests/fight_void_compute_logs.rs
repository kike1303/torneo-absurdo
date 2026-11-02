//! 012 RF18: an `engine_error` cannot be made with the real binary, so this one runs in process at
//! `trace`, with the failure of a test engine and a panic: no line has the seed in any form.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use server::admin::{BinaryTag, VoidCommand, run_fight_void};
use server::bets::BetsHook;
use server::fights::hex;
use server::fights::sealed::{ClosedWindow, ComputeError, SealedSeed, VoidCompute};
use server::fights::snapshot::FightSnapshot;
use support::bets;
use support::db::{TestDb, privileged};
use support::fights::{self as f, KO, T0, Vector};
use support::logs;

struct EventLimit;

impl VoidCompute for EventLimit {
    fn compute(
        &self,
        seed: SealedSeed,
        _: &FightSnapshot,
        _: &ClosedWindow,
    ) -> Result<(), (SealedSeed, ComputeError)> {
        Err((seed, ComputeError::Engine("event_limit")))
    }
}

#[test]
fn an_engine_error_does_not_log_its_seed() {
    let vector = Vector::of(KO);
    let captured = logs::run("trace", |_captured| async {
        let db = TestDb::create().await;
        let pool = db.migrated_pool().await;
        let admin = bets::user(&pool).await;
        privileged(
            &pool,
            sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
        )
        .await;
        let user = bets::funded_user(&pool, 100).await;
        let id = f::open_vector(&pool, T0, &vector).await;
        bets::seed_bet(&pool, id, user, "a", 10).await;
        f::close_by_sql(&pool, id, T0 + f::WINDOW).await;
        let command = VoidCommand {
            fight: id,
            by: admin,
            previous_binary: Some(BinaryTag::parse("v1.2.3").unwrap()),
        };
        run_fight_void(&db.options(), &command, Arc::new(EventLimit), &BetsHook)
            .await
            .unwrap();
        let reason: String = sqlx::query_scalar("SELECT void_reason FROM fights WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(reason, "engine_error");
        pool.close().await;
    });
    assert_eq!(captured.events("fight.void_applied").len(), 1);
    let text = captured.text();
    let seed = vector.seed.as_slice();
    for canary in [
        hex(seed),
        hex(seed).to_uppercase(),
        STANDARD.encode(seed),
        URL_SAFE_NO_PAD.encode(seed),
        serde_json::to_string(seed).unwrap(),
        format!("{seed:?}"),
    ] {
        assert!(!text.contains(&canary), "{canary}");
    }
}
