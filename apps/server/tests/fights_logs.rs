//! 010 RF28 (ADR-0012 A4.8, A4.9 and A8): with `RUST_LOG=trace`, a whole run with a known seed, with
//! failures in between: no line of any target has the seed, the log or a fragment of an event, and
//! no field outside the allowlist. It runs with a socket connected, which has to have received the
//! whole fight: if not, the absence of canaries would prove nothing.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use protocol::{FightState, ServerMessage};
use server::app::Settings;
use server::fights::hook::{SettleFuture, SettlementError, SettlementHook, SettlingFight};
use server::fights::{FightSettings, hex};
use server::telemetry::ALLOWED_FIELDS;
use sqlx::PgConnection;
use support::app::{ORIGIN, TRUST_LOOPBACK};
use support::db::TestDb;
use support::fights::{self as f, CADENCE, KO, T0, Vector, WINDOW, row, wait_for, wait_state};
use support::{logs, ws};

struct FailingUntilFixed {
    fixed: AtomicBool,
    called: AtomicBool,
}

impl SettlementHook for FailingUntilFixed {
    fn settle<'a>(
        &'a self,
        _tx: &'a mut PgConnection,
        _fight: &'a SettlingFight,
    ) -> SettleFuture<'a> {
        self.called.store(true, Ordering::SeqCst);
        let fixed = self.fixed.load(Ordering::SeqCst);
        Box::pin(async move {
            if fixed {
                Ok(())
            } else {
                Err(SettlementError { kind: "test" })
            }
        })
    }
}

#[test]
fn a_whole_run_logs_no_seed_and_no_fragment_of_the_log() {
    let vector = Vector::of(KO);
    let secret_lost = Vector::of(KO);
    let captured = logs::run("trace", |_captured| async {
        let db = TestDb::create().await;
        let hook = Arc::new(FailingUntilFixed {
            fixed: AtomicBool::new(false),
            called: AtomicBool::new(false),
        });
        let (settings, clock) = f::settings_at(T0);
        let settings = FightSettings {
            hook: hook.clone(),
            ..settings
        };
        let app_settings = Settings {
            fights: settings.clone(),
            ..Settings::default()
        };
        let (app, pool, shared) = f::start_app(&db, &[TRUST_LOOPBACK], &app_settings).await;
        let runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);

        // A fight from start to finish, opened with the known seed, with a socket on it.
        let id = f::open_vector(&pool, T0 + CADENCE, &vector).await;
        wait_for("the hub", || async { shared.hub().knows(id) }).await;
        let mut client = ws::connect(app.addr, ORIGIN).await.unwrap();
        ws::subscribe(&mut client, &format!("fight:{id}")).await;
        ws::send_text(&mut client, r#"{"type":"subscribe","channel":"schedule"}"#).await;
        clock.set(T0 + CADENCE + WINDOW);
        wait_state(&pool, id, FightState::BettingClosed).await;
        let (_, live_at) = f::hours(T0 + CADENCE);
        clock.set(live_at);
        wait_state(&pool, id, FightState::Live).await;
        clock.set(live_at + 100_000);
        // A settlement that fails, so its error path logs too.
        wait_for("the hook", || async { hook.called.load(Ordering::SeqCst) }).await;
        hook.fixed.store(true, Ordering::SeqCst);
        f::wait_state_advancing(&pool, &clock, id, FightState::Settled).await;
        assert_eq!(row(&pool, id).await.log, Some(vector.canonical()));

        // The positive control: the socket saw the whole fight, and its final.
        let mut seen_final = false;
        let mut seen_events = 0;
        while !seen_final || seen_events < vector.log.events.len() {
            let message = ws::next_message(&mut client, Duration::from_secs(30)).await;
            match message.expect("the socket did not see the whole fight") {
                ServerMessage::FightSync { fight, events, .. } => {
                    seen_final = fight.state == FightState::Settled;
                    seen_events = events.len();
                }
                ServerMessage::FightEvents { events, .. } => seen_events += events.len(),
                _ => {}
            }
        }

        // A safe stop, with the secret lost, so its path logs too.
        let lost = f::open_vector(&pool, T0 + 2 * CADENCE, &secret_lost).await;
        f::close_by_sql(&pool, lost, T0 + 2 * CADENCE + WINDOW).await;
        f::lose_secret(&pool, lost).await;
        clock.set(clock.now_ms().max(T0 + 2 * CADENCE + WINDOW + 1) + 60_000);
        wait_for("the halt", || async {
            shared.current().halted == Some(true)
        })
        .await;
        runner.stop().await;
        pool.close().await;
    });

    let text = captured.text();
    let seed = vector.seed.as_slice();
    let mut canaries = vec![
        hex(seed),
        hex(seed).to_uppercase(),
        STANDARD.encode(seed),
        URL_SAFE_NO_PAD.encode(seed),
        serde_json::to_string(seed).unwrap(),
        format!("{seed:?}"),
        String::from_utf8(vector.canonical()).unwrap(),
        // The hash of the snapshot is public, but a spec has no place in a log either.
        String::from_utf8(engine::canonical_spec_json(&vector.a).unwrap()).unwrap(),
    ];
    for event in &vector.log.events {
        canaries.push(serde_json::to_string(event).unwrap());
    }
    // Fragments of events: their names as they travel, which no event of the server has.
    for fragment in [
        r#""type":"#,
        r#""attacker":"#,
        r#""winner":"#,
        r#""ability":"#,
        // The messages of the socket, whole or by name.
        "fight_sync",
        "fight_events",
        "schedule_update",
    ] {
        canaries.push(fragment.to_owned());
    }
    for canary in &canaries {
        assert!(!text.contains(canary.as_str()), "the logs have {canary}");
    }

    // A positive control: the run did log, with the events of RF28.
    for event in [
        "fights.closed",
        "fights.live",
        "fights.settled",
        "fights.settle_failed",
        "fights.safe_stop",
        "fights.roster_invalid",
    ] {
        assert!(!captured.events(event).is_empty(), "no {event} in:\n{text}");
    }
    // Only the fields of the allowlist, and no `fight_id` among them.
    for line in captured.lines() {
        for (field, _) in line.as_object().unwrap() {
            let known = ALLOWED_FIELDS.contains(&field.as_str())
                || matches!(field.as_str(), "ts" | "level" | "target");
            assert!(known, "{field} in {line}");
        }
    }
    assert!(!ALLOWED_FIELDS.contains(&"fight_id"));
}
