//! 010 RF7: with only `GET /api/fights/{id}`, anyone reproduces the fight, also after the content
//! changed. The verifier itself (`support/public_verifier.rs`) imports nothing of the server.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;

use protocol::{FightDetail, FightFinal, FightState};
use server::app::Settings;
use server::fights::FightSettings;
use support::db::TestDb;
use support::fights::{
    self as f, CADENCE, DRAW, KO, T0, Vector, drive_to_settled, row, wait_state,
};
use support::http;
use support::public_verifier::{Verification, verify};

async fn detail(addr: SocketAddr, id: i32) -> FightDetail {
    let response = http::send(addr, "GET", &format!("/api/fights/{id}"), &[]).await;
    assert_eq!(response.status, 200, "{}", response.text());
    serde_json::from_slice(&response.body).unwrap()
}

/// The source of the verifier names nothing of the server.
#[test]
fn the_verifier_imports_nothing_of_the_server() {
    let source = include_str!("support/public_verifier.rs");
    let code: Vec<&str> = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect();
    for line in code {
        assert!(!line.contains("server"), "{line}");
        assert!(!line.contains("super::"), "{line}");
        assert!(!line.contains("crate::"), "{line}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_kind_of_final_verifies_with_public_data() {
    let db = TestDb::create().await;
    let (fight_settings, clock): (FightSettings, _) = f::settings_at(T0);
    let settings = Settings {
        fights: fight_settings.clone(),
        ..Settings::default()
    };
    let (app, pool, _shared) = f::start_app(&db, &[], &settings).await;
    let (_runner_shared, _runner) = f::start(&pool, &fight_settings);

    // A KO and a draw, by the runner.
    let mut decided = Vec::new();
    for (slot, spec) in [(1_i64, KO), (2, DRAW)] {
        let vector = Vector::of(spec);
        let id = f::open_vector(&pool, T0 + slot * CADENCE, &vector).await;
        drive_to_settled(&pool, &clock, id).await;
        decided.push((id, vector));
    }
    for (id, vector) in &decided {
        let body = detail(app.addr, *id).await;
        assert_eq!(verify(&body), Ok(Verification::Verified));
        assert_eq!(body.log.as_deref().unwrap().as_bytes(), vector.canonical());
    }

    // R3: in a decided fight, a field that is missing is a failed verification, not a skipped step.
    let good = detail(app.addr, decided[0].0).await;
    let missing: [fn(&mut FightDetail); 5] = [
        |d| d.seed = None,
        |d| d.spec_a = None,
        |d| d.spec_b = None,
        |d| d.log = None,
        |d| d.fight.seed_commit = None,
    ];
    for take in missing {
        let mut bad = good.clone();
        take(&mut bad);
        assert!(verify(&bad).is_err());
    }
    // And one that lies does not verify either.
    let mut wrong_seed = good.clone();
    wrong_seed.seed = Some("00".repeat(32));
    assert!(verify(&wrong_seed).is_err());
    let mut wrong_log = good.clone();
    wrong_log.log = Some(format!("{} ", good.log.as_deref().unwrap()));
    assert!(verify(&wrong_log).is_err());
    let mut wrong_spec = good.clone();
    wrong_spec.spec_a = wrong_spec.spec_b.clone();
    assert!(verify(&wrong_spec).is_err());
    let mut wrong_final = good.clone();
    wrong_final.fight.final_result = Some(FightFinal::Decided {
        winner: None,
        reason: protocol::EndReason::Draw,
    });
    assert!(verify(&wrong_final).is_err());

    // A voided one always proves its commit, and its snapshot unless that is what failed.
    let vector = Vector::of(KO);
    let voided = f::open_vector(&pool, T0 + 3 * CADENCE, &vector).await;
    f::close_by_sql(&pool, voided, T0 + 3 * CADENCE + f::WINDOW).await;
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, voided, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();
    let body = detail(app.addr, voided).await;
    assert_eq!(verify(&body), Ok(Verification::Verified));
    assert_eq!(body.seed, Some(server::fights::hex(&vector.seed)));

    // A cancelled one has no seed and no log.
    let cancelled = f::announce(&pool, T0 + 4 * CADENCE, &vector.a, &vector.b).await;
    clock.set(T0 + 4 * CADENCE + f::WINDOW);
    wait_state(&pool, cancelled, FightState::Settled).await;
    let body = detail(app.addr, cancelled).await;
    assert_eq!(verify(&body), Ok(Verification::CancelledUnverifiable));
    assert_eq!(body.seed, None);
    assert_eq!(body.log, None);
    assert_eq!(row(&pool, cancelled).await.revealed_seed, None);
}

/// RF7: the verification is repeated after `content/` changes: a fight is what it was announced with.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_verifies_after_its_content_changed() {
    let db = TestDb::create().await;
    let (fight_settings, clock): (FightSettings, _) = f::settings_at(T0 + 1);
    let settings = Settings {
        fights: fight_settings.clone(),
        ..Settings::default()
    };
    let (app, pool, shared) = f::start_app(&db, &[], &settings).await;
    let content = f::copy_of_content("verify");
    let runner = f::spawn_runner(
        &pool,
        &shared,
        f::config_on(&content),
        fight_settings.clone(),
    );
    f::wait_for("the announcement", || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fights")
            .fetch_one(&pool)
            .await
            .unwrap()
            == 1
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

    // The content of one of the two changes, and then it is gone: the fight does not notice.
    let character = content.join(&announced.slug_a).join("character.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&character).unwrap()).unwrap();
    json["stats"]["hp"] = serde_json::json!(1);
    std::fs::write(&character, serde_json::to_vec(&json).unwrap()).unwrap();
    let runner = f::spawn_runner(&pool, &shared, f::config_on(&content), fight_settings);
    drive_to_settled(&pool, &clock, id).await;
    runner.stop().await;
    assert_eq!(
        verify(&detail(app.addr, id).await),
        Ok(Verification::Verified)
    );

    std::fs::remove_dir_all(&content).unwrap();
    let body = detail(app.addr, id).await;
    assert_eq!(verify(&body), Ok(Verification::Verified));
    assert_eq!(body.spec_a.unwrap().as_bytes(), announced.spec_a);
}

/// RF7: a voided fight skips RF2 only with `snapshot_invalid`, and always proves its commit.
#[tokio::test]
async fn a_voided_fight_with_a_bad_snapshot_still_proves_its_commit() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let vector = Vector::of(KO);
    let id = f::announce_with(&pool, T0, &vector.a, &vector.b, |raw| {
        raw.snapshot_sha256 = vec![0; 32];
    })
    .await;
    f::open_with(&pool, id, &vector.seed, T0).await;
    f::close_by_sql(&pool, id, T0 + f::WINDOW).await;
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, id, &vector.seed, "snapshot_invalid").await;
    tx.commit().await.unwrap();

    let body = detail(app.addr, id).await;
    assert_eq!(verify(&body), Ok(Verification::Verified));

    // The same bad hash with the other reason does not verify, and neither does another seed.
    let mut other_reason = body.clone();
    other_reason.fight.final_result = Some(FightFinal::Voided {
        reason: protocol::VoidReason::EngineError,
    });
    assert!(verify(&other_reason).is_err());
    let mut other_seed = body.clone();
    other_seed.seed = Some("00".repeat(32));
    assert!(verify(&other_seed).is_err());
    let mut no_seed = body;
    no_seed.seed = None;
    assert!(verify(&no_seed).is_err());
}

/// 012 RF21: a fight voided with its secret lost, through the real route, has its own result, only
/// with no seed and no log, and a null seed never passes as a step skipped in any other final.
#[tokio::test]
async fn a_lost_secret_is_its_own_result_and_a_null_seed_is_nothing_else() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let vector = Vector::of(KO);
    let now: i64 = sqlx::query_scalar("SELECT (extract(epoch FROM now()) * 1000)::bigint")
        .fetch_one(&pool)
        .await
        .unwrap();
    let opens = now - 100 * CADENCE;
    let id = f::announce(&pool, opens, &vector.a, &vector.b).await;
    f::open_with(&pool, id, &vector.seed, opens).await;
    f::close_by_sql(&pool, id, opens + f::WINDOW).await;
    f::lose_secret(&pool, id).await;
    sqlx::query(
        "UPDATE fights SET state = 'settled', settled_at = now(), void_reason = 'secret_lost' \
         WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let body = detail(app.addr, id).await;
    assert_eq!(body.seed, None);
    assert_eq!(body.log, None);
    assert_eq!(verify(&body), Ok(Verification::UnverifiableSecretLost));

    // A lost secret with a seed, or a log, or without a commit, or whose snapshot lies.
    let mut with_seed = body.clone();
    with_seed.seed = Some(server::fights::hex(&vector.seed));
    assert!(verify(&with_seed).is_err());
    let mut with_log = body.clone();
    with_log.log = Some("{}".to_owned());
    assert!(verify(&with_log).is_err());
    let mut no_commit = body.clone();
    no_commit.fight.seed_commit = None;
    assert!(verify(&no_commit).is_err());
    let mut other_spec = body.clone();
    other_spec.spec_a = other_spec.spec_b.clone();
    assert!(verify(&other_spec).is_err());

    // The same body under any other final is not verified: a null seed is not a step skipped.
    for reason in [
        protocol::VoidReason::SnapshotInvalid,
        protocol::VoidReason::EngineError,
    ] {
        let mut other = body.clone();
        other.fight.final_result = Some(FightFinal::Voided { reason });
        assert!(verify(&other).is_err(), "{reason:?}");
    }
    let mut decided = body.clone();
    decided.fight.final_result = Some(FightFinal::Decided {
        winner: Some(protocol::Side::A),
        reason: protocol::EndReason::Ko,
    });
    assert!(verify(&decided).is_err());
}
