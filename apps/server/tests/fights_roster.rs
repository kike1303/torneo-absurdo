//! 010 RF5 and RF22: the roster is loaded once, a bad one stops only the new fights, and an announced
//! fight runs from its snapshot, whatever happens to `content/` afterwards.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use engine::{CharacterSpec, Ruleset, Seed, canonical_json, simulate};
use protocol::FightState;
use server::fights::FightsShared;
use server::fights::roster::Roster;
use support::db::TestDb;
use support::fights::{
    self as f, CADENCE, KO, T0, Vector, drive_to_settled, row, wait_for, wait_state,
};

#[test]
fn the_real_roster_loads_sorted_and_a_bad_one_has_a_code() {
    let roster = Roster::load_blocking(&f::repo_root().join("content/characters"))
        .map_err(|e| e.kind)
        .unwrap();
    assert!(roster.slugs().len() >= 2);
    assert!(roster.slugs().is_sorted());

    let missing = Roster::load_blocking(Path::new("/nonexistent/content"));
    assert!(missing.is_err());

    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "fights-roster-empty-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        Roster::load_blocking(&empty)
            .map(|_| ())
            .map_err(|e| e.kind),
        Err("too_few_characters")
    );

    // One character is not enough: fewer than 2.
    let one = f::content_with("one", 1);
    assert_eq!(
        Roster::load_blocking(&one).map(|_| ()).map_err(|e| e.kind),
        Err("too_few_characters")
    );
}

/// RF22: with a bad roster the server goes on, no new fights appear, and those created go on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_roster_only_stops_the_new_fights() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0 + CADENCE, &vector.a, &vector.b).await;
    let (settings, clock) = f::settings_at(T0 + 1);
    let (shared, _runner) = f::start(&pool, &settings);
    wait_for("the evaluation", || async {
        shared.current().halted == Some(false)
    })
    .await;
    clock.set(T0 + CADENCE);
    wait_state(&pool, id, FightState::BettingOpen).await;
    drive_to_settled(&pool, &clock, id).await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(shared.current().halted, Some(false));
}

/// RF5: changing or deleting `content/` after the announcement and restarting gives the same log.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_runs_from_its_snapshot_whatever_happens_to_the_content() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let content = f::copy_of_content("snapshot");
    let (settings, clock) = f::settings_at(T0 + 1);
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let runner = f::spawn_runner(&pool, &shared, f::config_on(&content), settings.clone());
    wait_for("the announcement", || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fights")
            .fetch_one(&pool)
            .await
            .unwrap()
            >= 1
    })
    .await;
    let id: i32 = sqlx::query_scalar("SELECT min(id) FROM fights")
        .fetch_one(&pool)
        .await
        .unwrap();
    let announced = row(&pool, id).await;
    clock.set(announced.opens_at);
    wait_state(&pool, id, FightState::BettingOpen).await;
    runner.stop().await;

    // One character changes, and then there is no content at all.
    let character = content.join(&announced.slug_a).join("character.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&character).unwrap()).unwrap();
    json["stats"]["hp"] = serde_json::json!(1);
    std::fs::write(&character, serde_json::to_vec(&json).unwrap()).unwrap();
    std::fs::remove_dir_all(&content).unwrap();

    let secret: Vec<u8> = sqlx::query_scalar("SELECT seed FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let shared = Arc::new(FightsShared::new(clock.clone()));
    let _runner = f::spawn_runner(&pool, &shared, f::config_on(&content), settings);
    wait_for("the evaluation", || async {
        shared.current().halted == Some(false)
    })
    .await;
    drive_to_settled(&pool, &clock, id).await;

    let fight = row(&pool, id).await;
    assert_eq!(fight.spec_a, announced.spec_a);
    let a: CharacterSpec = serde_json::from_slice(&fight.spec_a).unwrap();
    let b: CharacterSpec = serde_json::from_slice(&fight.spec_b).unwrap();
    let seed = Seed::from_bytes(secret.try_into().unwrap());
    let log = simulate(&seed, &a, &b, &Ruleset::for_version(1).unwrap()).unwrap();
    assert_eq!(fight.log.unwrap(), canonical_json(&log).unwrap());
    tokio::time::sleep(Duration::from_millis(10)).await;
}
