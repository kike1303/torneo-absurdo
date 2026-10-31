//! 010 RF45: with `dev-fights`, a script forces the fights and their seeds, in a cycle that survives a
//! restart, with the golden vectors of the engine.
#![cfg(feature = "dev-fights")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use protocol::FightState;
use server::fights::script::FightScript;
use server::fights::state::target_slot;
use server::fights::{FightSettings, FightsShared, hex};
use sqlx::PgPool;
use support::db::TestDb;
use support::fights::{self as f, CADENCE, DRAW, KO, T0, Vector, drive_to_settled, row, wait_for};

fn script_of(vectors: &[&Vector]) -> Arc<FightScript> {
    let rows: Vec<_> = vectors
        .iter()
        .map(|vector| {
            serde_json::json!({
                "seed": hex(&vector.seed),
                "spec_a": vector.a,
                "spec_b": vector.b,
            })
        })
        .collect();
    let bytes = serde_json::to_vec(&rows).unwrap();
    Arc::new(FightScript::from_json(&bytes).unwrap())
}

async fn last_id(pool: &PgPool) -> Option<i32> {
    sqlx::query_scalar("SELECT max(id) FROM fights")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The fight of the next slot, announced and run to its end, by the runner alone.
async fn next_fight(
    pool: &PgPool,
    clock: &server::fights::FightClock,
    previous: Option<i32>,
) -> i32 {
    wait_for("the announcement", || async {
        last_id(pool).await != previous
    })
    .await;
    let id = last_id(pool).await.unwrap();
    let opens_at = row(pool, id).await.opens_at;
    clock.set(clock.now_ms().max(opens_at));
    f::wait_state(pool, id, FightState::BettingOpen).await;
    drive_to_settled(pool, clock, id).await;
    id
}

fn assert_is(fight: &server::fights::state::FightRow, vector: &Vector) {
    assert_eq!(fight.log.as_deref(), Some(vector.canonical().as_slice()));
    assert_eq!(fight.revealed_seed.as_deref(), Some(vector.seed.as_slice()));
    assert_eq!(fight.slug_a, vector.a.slug);
    assert_eq!(fight.slug_b, vector.b.slug);
    assert_eq!(fight.state, FightState::Settled);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_script_forces_its_fights_in_a_cycle_also_after_a_restart() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let (ko, draw) = (Vector::of(KO), Vector::of(DRAW));
    let (settings, clock) = f::settings_at(T0 - 1);
    let settings = FightSettings {
        script: Some(script_of(&[&ko, &draw])),
        ..settings
    };
    // No roster: a script does not need one.
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings.clone());

    let first = next_fight(&pool, &clock, None).await;
    assert_is(&row(&pool, first).await, &ko);
    assert_eq!(
        row(&pool, first).await.opens_at,
        target_slot(T0 - 1, CADENCE)
    );
    let second = next_fight(&pool, &clock, Some(first)).await;
    assert_is(&row(&pool, second).await, &draw);

    // A new process goes on from the table, not from a counter.
    runner.stop().await;
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let _runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);
    let third = next_fight(&pool, &clock, Some(second)).await;
    assert_is(&row(&pool, third).await, &ko);
    let fourth = next_fight(&pool, &clock, Some(third)).await;
    assert_is(&row(&pool, fourth).await, &draw);
}

/// A fight announced before the script was configured has other specs: `OsRng`, not a seed of the
/// script.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_with_other_specs_does_not_take_the_seed_of_the_script() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let (ko, draw) = (Vector::of(KO), Vector::of(DRAW));
    let id = f::announce(&pool, T0, &draw.a, &draw.b).await;
    let (settings, clock) = f::settings_at(T0);
    let settings = FightSettings {
        script: Some(script_of(&[&ko])),
        ..settings
    };
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let _runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);
    f::wait_state(&pool, id, FightState::BettingOpen).await;
    let secret: Vec<u8> = sqlx::query_scalar("SELECT seed FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(secret, ko.seed.to_vec());
    assert_ne!(secret, draw.seed.to_vec());
}

/// `Config` has a hand-written `Debug`, which cannot print what has none.
#[test]
fn the_script_does_not_show_in_the_debug_of_the_config() {
    let vector = Vector::of(KO);
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("script-{}.json", uuid::Uuid::new_v4().simple()));
    let rows = serde_json::json!([{
        "seed": hex(&vector.seed),
        "spec_a": vector.a,
        "spec_b": vector.b,
    }]);
    std::fs::write(&path, serde_json::to_vec(&rows).unwrap()).unwrap();
    let path = path.to_str().unwrap().to_owned();
    let config = support::config::config(&[("FIGHT_SCRIPT_PATH", path.as_str())]);
    assert!(config.fight_script.is_some());
    let shown = format!("{config:?}");
    assert!(!shown.contains("fight_script"), "{shown}");
    assert!(!shown.contains(&hex(&vector.seed)), "{shown}");
    assert!(!shown.contains(&path), "{shown}");
}
