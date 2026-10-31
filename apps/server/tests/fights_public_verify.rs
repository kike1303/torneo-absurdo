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
use support::public_verifier::verify;

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
        verify(&body).unwrap();
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
    verify(&body).unwrap();
    assert_eq!(body.seed, Some(server::fights::hex(&vector.seed)));

    // A cancelled one has no seed and no log.
    let cancelled = f::announce(&pool, T0 + 4 * CADENCE, &vector.a, &vector.b).await;
    clock.set(T0 + 4 * CADENCE + f::WINDOW);
    wait_state(&pool, cancelled, FightState::Settled).await;
    let body = detail(app.addr, cancelled).await;
    verify(&body).unwrap();
    assert_eq!(body.seed, None);
    assert_eq!(body.log, None);
    assert_eq!(row(&pool, cancelled).await.revealed_seed, None);
}
