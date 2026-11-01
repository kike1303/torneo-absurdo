//! 010 RF28 with the seed of a script as a canary, at `trace`, with a socket connected.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![cfg(feature = "dev-fights")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use protocol::{FightState, ServerMessage};
use server::app::Settings;
use server::fights::script::FightScript;
use server::fights::{FightSettings, hex};
use support::app::{ORIGIN, TRUST_LOOPBACK};
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector, wait_for};
use support::{logs, ws};

#[test]
fn a_run_with_a_script_does_not_log_its_seed() {
    let vector = Vector::of(KO);
    let rows = serde_json::json!([{
        "seed": hex(&vector.seed),
        "spec_a": vector.a,
        "spec_b": vector.b,
    }]);
    let script = Arc::new(FightScript::from_json(&serde_json::to_vec(&rows).unwrap()).unwrap());
    let captured = logs::run("trace", |_captured| async {
        let db = TestDb::create().await;
        let (settings, clock) = f::settings_at(T0 - 1);
        let settings = FightSettings {
            script: Some(script),
            ..settings
        };
        let app_settings = Settings {
            fights: settings.clone(),
            ..Settings::default()
        };
        let (app, pool, shared) = f::start_app(&db, &[TRUST_LOOPBACK], &app_settings).await;
        let runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);
        wait_for("the announcement", || async { shared.hub().knows(1) }).await;
        let id = 1;
        let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
        ws::subscribe(&mut client, "fight:1").await;
        clock.set(T0);
        f::wait_state(&pool, id, FightState::BettingOpen).await;
        f::drive_to_settled(&pool, &clock, id).await;
        assert_eq!(
            f::row(&pool, id).await.revealed_seed.as_deref(),
            Some(vector.seed.as_slice())
        );
        // The positive control: the socket saw it to the end.
        let mut settled = false;
        let mut released = 0;
        while !settled || released < vector.log.events.len() {
            let message = ws::next_message(&mut client, Duration::from_secs(30)).await;
            match message.expect("the socket did not see the final") {
                ServerMessage::FightSync { fight, .. } => {
                    settled = fight.state == FightState::Settled;
                }
                ServerMessage::FightEvents {
                    from_index, events, ..
                } => released = usize::try_from(from_index).unwrap() + events.len(),
                _ => {}
            }
        }
        runner.stop().await;
        pool.close().await;
    });

    let text = captured.text();
    let seed = vector.seed.as_slice();
    for canary in [
        hex(seed),
        hex(seed).to_uppercase(),
        STANDARD.encode(seed),
        URL_SAFE_NO_PAD.encode(seed),
        serde_json::to_string(seed).unwrap(),
        format!("{seed:?}"),
        String::from_utf8(vector.canonical()).unwrap(),
    ] {
        assert!(!text.contains(&canary), "the logs have {canary}");
    }
    assert!(!captured.events("fights.settled").is_empty(), "{text}");
}
