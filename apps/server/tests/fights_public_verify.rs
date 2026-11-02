//! 010 RF7: with only `GET /api/fights/{id}`, anyone reproduces the fight, also after the content
//! changed. The verifier itself (`support/public_verifier.rs`) imports nothing of the server.
//!
//! 011 RF14: `tests/golden/fight-verify-vectors.json`, the same table for this verifier and for the
//! one of the browser (`apps/web/tests/replay-vectors.test.ts`). Written with
//! `UPDATE_GOLDEN=1 cargo test -p server --test fights_public_verify`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::BTreeSet;
use std::net::SocketAddr;

use engine::{Ruleset, canonical_spec_json};
use protocol::{
    CancelReason, EndReason, FightDetail, FightFinal, FightState, FightSummary, FighterCard,
    TimestampMs, VoidReason,
};
use serde_json::{Value, json};
use server::app::Settings;
use server::fights::FightSettings;
use server::fights::snapshot::snapshot_sha256;
use sha2::{Digest, Sha256};
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
    verify(&detail(app.addr, id).await).unwrap();

    std::fs::remove_dir_all(&content).unwrap();
    let body = detail(app.addr, id).await;
    verify(&body).unwrap();
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
    verify(&body).unwrap();

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

const VECTORS_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/fight-verify-vectors.json"
);

/// `id` of every fight of the file, and so the id of the URL the browser verifier is given.
const VECTOR_FIGHT_ID: u32 = 12;

const DECISION: (&str, &str, u32) = ("tank", "turtle", 0);

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A final and, with it, what its fight publishes: the bytes of the engine, canonical specs and fixed
/// hours (011 D5), so the file depends neither on a run nor on Postgres.
fn detail_of_vector(
    vector: &Vector,
    seed: &[u8; 32],
    final_result: FightFinal,
    played: bool,
) -> Value {
    let ruleset = Ruleset::for_version(1).unwrap();
    let spec_a = canonical_spec_json(&vector.a).unwrap();
    let spec_b = canonical_spec_json(&vector.b).unwrap();
    let cancelled = matches!(final_result, FightFinal::Cancelled { .. });
    let (closes_at, live_at) = f::hours(T0);
    let detail = FightDetail {
        fight: FightSummary {
            id: VECTOR_FIGHT_ID,
            state: FightState::Settled,
            a: FighterCard {
                slug: vector.a.slug.clone(),
                max_hp: vector.log.a.max_hp,
            },
            b: FighterCard {
                slug: vector.b.slug.clone(),
                max_hp: vector.log.b.max_hp,
            },
            ruleset_version: 1,
            snapshot_sha256: hex_of(&snapshot_sha256(1, &spec_a, &spec_b)),
            seed_commit: (!cancelled).then(|| hex_of(&Sha256::digest(seed))),
            opens_at: TimestampMs::from_ms(T0),
            closes_at: TimestampMs::from_ms(closes_at),
            live_at: TimestampMs::from_ms(live_at),
            live_started_at: played.then(|| TimestampMs::from_ms(live_at)),
            time_limit_ms: ruleset.time_limit_ms(),
            max_duration_ms: ruleset.time_limit_ms() + ruleset.end_ms(),
            playback_delay_ms: 3500,
            final_result: Some(final_result),
        },
        spec_a: Some(String::from_utf8(spec_a).unwrap()),
        spec_b: Some(String::from_utf8(spec_b).unwrap()),
        seed: (!cancelled).then(|| hex_of(seed)),
        log: played.then(|| String::from_utf8(vector.canonical()).unwrap()),
    };
    serde_json::to_value(detail).unwrap()
}

/// A decided fight, with the final of its own log.
fn decided_detail(spec: (&str, &str, u32), reason: EndReason) -> Value {
    let vector = Vector::of(spec);
    let (winner, end) = vector.log.outcome().unwrap();
    assert_eq!(end, reason, "the vector is not the one it says");
    let final_result = FightFinal::Decided {
        winner,
        reason: end,
    };
    detail_of_vector(&vector, &vector.seed, final_result, true)
}

fn voided_detail(seed: &[u8; 32], reason: VoidReason) -> Value {
    let vector = Vector::of(KO);
    detail_of_vector(&vector, seed, FightFinal::Voided { reason }, false)
}

fn cancelled_detail(reason: CancelReason) -> Value {
    let vector = Vector::of(KO);
    let seed = vector.seed;
    detail_of_vector(&vector, &seed, FightFinal::Cancelled { reason }, false)
}

/// The text of a field of a detail, which has to be there.
fn text_of(detail: &Value, pointer: &str) -> String {
    detail
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no text at {pointer}"))
        .to_owned()
}

/// The same text with its first digit changed: the smallest change, and it is still text.
fn one_byte_off(text: &str) -> String {
    let at = text.find(|c: char| c.is_ascii_digit()).unwrap();
    let digit = text.as_bytes()[at];
    let other = if digit == b'0' { "1" } else { "0" };
    format!("{}{other}{}", &text[..at], &text[at + 1..])
}

/// `snapshot_sha256` as if the publisher had hashed these exact fields.
fn rehash(detail: &mut Value) {
    let version = detail["fight"]["ruleset_version"].as_u64().unwrap();
    let text = format!(
        r#"{{"ruleset_version":{version},"a":{},"b":{}}}"#,
        text_of(detail, "/spec_a"),
        text_of(detail, "/spec_b")
    );
    detail["fight"]["snapshot_sha256"] = json!(hex_of(&Sha256::digest(text.as_bytes())));
}

/// A line of the file, and it says what each verifier has to answer.
fn vector_line(name: &str, detail: Value, native: &str, browser: &str) -> Value {
    json!({
        "name": name,
        "expected_id": VECTOR_FIGHT_ID,
        "detail": detail,
        "native": native,
        "browser": browser,
        // The declared differences between the two (RF14): a build that is too old is no evidence.
        "exception": browser.starts_with("cannot_check"),
    })
}

/// The whole table, base fights first and then each of them altered.
#[allow(clippy::too_many_lines)]
fn verify_vectors() -> Vec<Value> {
    let ko = decided_detail(KO, EndReason::Ko);
    let decision = decided_detail(DECISION, EndReason::Decision);
    let draw = decided_detail(f::DRAW, EndReason::Draw);
    let seed_of_ko = Vector::of(KO).seed;
    let voided = voided_detail(&seed_of_ko, VoidReason::EngineError);
    let bad_hash = "00".repeat(32);
    let mut voided_invalid = voided_detail(&seed_of_ko, VoidReason::SnapshotInvalid);
    voided_invalid["fight"]["snapshot_sha256"] = json!(bad_hash);
    let missed = cancelled_detail(CancelReason::MissedWindow);
    let mut unrunnable = cancelled_detail(CancelReason::FightUnrunnable);
    unrunnable["fight"]["snapshot_sha256"] = json!(bad_hash);

    let mut out = vec![
        vector_line("ko", ko.clone(), "ok", "match:fight"),
        vector_line("decision", decision, "ok", "match:fight"),
        vector_line("draw", draw.clone(), "ok", "match:fight"),
        vector_line("voided_engine_error", voided.clone(), "ok", "match:commit"),
        vector_line(
            "voided_snapshot_invalid",
            voided_invalid.clone(),
            "ok",
            "match:commit",
        ),
        vector_line(
            "cancelled_missed_window",
            missed.clone(),
            "ok",
            "match:snapshot",
        ),
        vector_line(
            "cancelled_fight_unrunnable",
            unrunnable.clone(),
            "ok",
            "not_applicable",
        ),
    ];

    // Each row: its name, the base, the change, and what each verifier answers.
    type Change = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, &Value, Change, &str, &str)> = vec![
        // The bytes of the log.
        (
            "log_one_byte_off",
            &ko,
            Box::new(|d| d["log"] = json!(one_byte_off(&text_of(d, "/log")))),
            "err",
            "mismatch:log",
        ),
        (
            "log_with_a_trailing_space",
            &ko,
            Box::new(|d| d["log"] = json!(format!("{} ", text_of(d, "/log")))),
            "err",
            "mismatch:log",
        ),
        (
            "log_null",
            &ko,
            Box::new(|d| d["log"] = Value::Null),
            "err",
            "mismatch:log",
        ),
        (
            "log_not_json",
            &ko,
            Box::new(|d| d["log"] = json!("not json")),
            "err",
            "mismatch:log",
        ),
        // The seed and its commit.
        (
            "seed_of_another_fight",
            &ko,
            Box::new(|d| d["seed"] = json!("00".repeat(32))),
            "err",
            "mismatch:commit",
        ),
        (
            "seed_null",
            &ko,
            Box::new(|d| d["seed"] = Value::Null),
            "err",
            "mismatch:commit",
        ),
        // D2: the same bytes, written in a way RF13 does not admit.
        (
            "seed_in_upper_case",
            &ko,
            Box::new(|d| {
                let seed = text_of(d, "/seed");
                assert_ne!(seed, seed.to_uppercase());
                d["seed"] = json!(seed.to_uppercase());
            }),
            "err",
            "mismatch:commit",
        ),
        (
            "seed_too_short",
            &ko,
            Box::new(|d| d["seed"] = json!("ab".repeat(31))),
            "err",
            "mismatch:commit",
        ),
        (
            "seed_not_hexadecimal",
            &ko,
            Box::new(|d| d["seed"] = json!("zz".repeat(32))),
            "err",
            "mismatch:commit",
        ),
        (
            "seed_commit_one_byte_off",
            &ko,
            Box::new(|d| {
                let commit = text_of(d, "/fight/seed_commit");
                d["fight"]["seed_commit"] = json!(one_byte_off(&commit));
            }),
            "err",
            "mismatch:commit",
        ),
        (
            "seed_commit_null",
            &ko,
            Box::new(|d| d["fight"]["seed_commit"] = Value::Null),
            "err",
            "mismatch:commit",
        ),
        // The snapshot.
        (
            "snapshot_one_byte_off",
            &ko,
            Box::new(|d| {
                let hash = text_of(d, "/fight/snapshot_sha256");
                d["fight"]["snapshot_sha256"] = json!(one_byte_off(&hash));
            }),
            "err",
            "mismatch:snapshot",
        ),
        (
            "spec_a_null",
            &ko,
            Box::new(|d| d["spec_a"] = Value::Null),
            "err",
            "mismatch:snapshot",
        ),
        (
            "spec_b_null",
            &ko,
            Box::new(|d| d["spec_b"] = Value::Null),
            "err",
            "mismatch:snapshot",
        ),
        (
            "spec_a_one_byte_off",
            &ko,
            Box::new(|d| d["spec_a"] = json!(one_byte_off(&text_of(d, "/spec_a")))),
            "err",
            "mismatch:snapshot",
        ),
        (
            "ruleset_version_without_its_hash",
            &ko,
            Box::new(|d| d["fight"]["ruleset_version"] = json!(2)),
            "err",
            "mismatch:snapshot",
        ),
        // What the engine rejects, with a hash that agrees: the fight does not come out the same.
        (
            "spec_a_not_json_with_its_hash",
            &ko,
            Box::new(|d| {
                d["spec_a"] = json!("{");
                rehash(d);
            }),
            "err",
            "mismatch:log",
        ),
        (
            "spec_a_above_8_kib_with_its_hash",
            &ko,
            Box::new(|d| {
                let spec = text_of(d, "/spec_a");
                d["spec_a"] = json!(format!("{spec:<8193}"));
                rehash(d);
            }),
            "err",
            "mismatch:log",
        ),
        // The final.
        (
            "final_of_another_winner",
            &ko,
            Box::new(|d| {
                let winner = text_of(d, "/fight/final/winner");
                let other = if winner == "a" { "b" } else { "a" };
                d["fight"]["final"]["winner"] = json!(other);
            }),
            "err",
            "mismatch:final",
        ),
        (
            "final_of_another_reason",
            &ko,
            Box::new(|d| d["fight"]["final"]["reason"] = json!("decision")),
            "err",
            "mismatch:final",
        ),
        (
            "final_draw_with_a_winner",
            &draw,
            Box::new(|d| d["fight"]["final"]["winner"] = json!("a")),
            "err",
            "mismatch:final",
        ),
        (
            "final_cancelled_on_a_played_fight",
            &ko,
            Box::new(|d| {
                d["fight"]["final"] = json!({ "kind": "cancelled", "reason": "missed_window" });
            }),
            "err",
            "mismatch:commit",
        ),
        // What is not a finished fight.
        (
            "final_null",
            &ko,
            Box::new(|d| d["fight"]["final"] = Value::Null),
            "err",
            "mismatch:state",
        ),
        (
            "state_live",
            &ko,
            Box::new(|d| d["fight"]["state"] = json!("live")),
            "err",
            "mismatch:state",
        ),
        (
            "state_unknown",
            &ko,
            Box::new(|d| d["fight"]["state"] = json!("paused")),
            "undeserializable",
            "mismatch:state",
        ),
        // The summary.
        (
            "summary_slug_a",
            &ko,
            Box::new(|d| d["fight"]["a"]["slug"] = json!("someone-else")),
            "err",
            "mismatch:summary",
        ),
        (
            "summary_slug_b",
            &ko,
            Box::new(|d| d["fight"]["b"]["slug"] = json!("someone-else")),
            "err",
            "mismatch:summary",
        ),
        (
            "summary_max_hp_a",
            &ko,
            Box::new(|d| d["fight"]["a"]["max_hp"] = json!(1)),
            "err",
            "mismatch:summary",
        ),
        (
            "summary_max_hp_b",
            &ko,
            Box::new(|d| d["fight"]["b"]["max_hp"] = json!(1)),
            "err",
            "mismatch:summary",
        ),
        // The bounds.
        (
            "bounds_time_limit",
            &ko,
            Box::new(|d| d["fight"]["time_limit_ms"] = json!(1)),
            "err",
            "mismatch:bounds",
        ),
        (
            "bounds_max_duration",
            &ko,
            Box::new(|d| d["fight"]["max_duration_ms"] = json!(1)),
            "err",
            "mismatch:bounds",
        ),
        // A voided fight.
        (
            "voided_seed_null",
            &voided,
            Box::new(|d| d["seed"] = Value::Null),
            "err",
            "mismatch:commit",
        ),
        (
            "voided_seed_of_another_fight",
            &voided,
            Box::new(|d| d["seed"] = json!("00".repeat(32))),
            "err",
            "mismatch:commit",
        ),
        (
            "voided_with_a_log",
            &voided,
            Box::new(|d| d["log"] = json!("{}")),
            "err",
            "mismatch:log",
        ),
        (
            "voided_engine_error_with_a_bad_snapshot",
            &voided,
            Box::new(|d| d["fight"]["snapshot_sha256"] = json!("00".repeat(32))),
            "err",
            "mismatch:snapshot",
        ),
        (
            "voided_snapshot_invalid_without_a_spec",
            &voided_invalid,
            Box::new(|d| d["spec_a"] = Value::Null),
            "ok",
            "match:commit",
        ),
        // A cancelled fight.
        (
            "cancelled_with_a_seed",
            &missed,
            Box::new(|d| d["seed"] = json!("00".repeat(32))),
            "err",
            "mismatch:commit",
        ),
        (
            "cancelled_with_a_commit",
            &missed,
            Box::new(|d| d["fight"]["seed_commit"] = json!("00".repeat(32))),
            "err",
            "mismatch:commit",
        ),
        (
            "cancelled_with_a_log",
            &missed,
            Box::new(|d| d["log"] = json!("{}")),
            "err",
            "mismatch:log",
        ),
        (
            "cancelled_missed_window_with_a_bad_snapshot",
            &missed,
            Box::new(|d| d["fight"]["snapshot_sha256"] = json!("00".repeat(32))),
            "err",
            "mismatch:snapshot",
        ),
        (
            "cancelled_missed_window_without_a_spec",
            &missed,
            Box::new(|d| d["spec_b"] = Value::Null),
            "err",
            "mismatch:snapshot",
        ),
        (
            "cancelled_fight_unrunnable_without_a_spec",
            &unrunnable,
            Box::new(|d| d["spec_a"] = Value::Null),
            "ok",
            "not_applicable",
        ),
        (
            "cancelled_fight_unrunnable_with_a_seed",
            &unrunnable,
            Box::new(|d| d["seed"] = json!("00".repeat(32))),
            "err",
            "mismatch:commit",
        ),
        // The two declared exceptions (RF14), and they never become a match.
        (
            "unknown_ruleset_with_its_hash",
            &ko,
            Box::new(|d| {
                d["fight"]["ruleset_version"] = json!(2);
                rehash(d);
            }),
            "err",
            "cannot_check:ruleset_unknown",
        ),
        (
            "unknown_final_kind",
            &ko,
            Box::new(|d| d["fight"]["final"] = json!({ "kind": "postponed" })),
            "undeserializable",
            "cannot_check:unknown_final",
        ),
        (
            "unknown_decided_reason",
            &ko,
            Box::new(|d| d["fight"]["final"]["reason"] = json!("forfeit")),
            "undeserializable",
            "cannot_check:unknown_final",
        ),
        (
            "unknown_cancelled_reason",
            &missed,
            Box::new(|d| d["fight"]["final"]["reason"] = json!("rain")),
            "undeserializable",
            "cannot_check:unknown_final",
        ),
        (
            "unknown_voided_reason",
            &voided,
            Box::new(|d| d["fight"]["final"]["reason"] = json!("secret_lost")),
            "undeserializable",
            "cannot_check:unknown_final",
        ),
        // RF22: TA-27 will add `secret_lost`, without a seed, and today it must not break.
        (
            "unknown_voided_reason_without_a_seed",
            &voided,
            Box::new(|d| {
                d["fight"]["final"]["reason"] = json!("secret_lost");
                d["seed"] = Value::Null;
            }),
            "undeserializable",
            "cannot_check:unknown_final",
        ),
        // Without the shape of `FightDetail`.
        (
            "data_not_an_object",
            &ko,
            Box::new(|d| *d = json!("not a fight")),
            "undeserializable",
            "mismatch:data",
        ),
        (
            "data_without_the_fight",
            &ko,
            Box::new(|d| *d = json!({ "seed": null, "log": null })),
            "undeserializable",
            "mismatch:data",
        ),
        (
            "data_a_spec_that_is_not_text",
            &ko,
            Box::new(|d| d["spec_a"] = json!(1)),
            "undeserializable",
            "mismatch:data",
        ),
        (
            "data_a_ruleset_out_of_range",
            &ko,
            Box::new(|d| d["fight"]["ruleset_version"] = json!(70_000)),
            "undeserializable",
            "mismatch:data",
        ),
        (
            "data_a_ruleset_that_is_not_an_integer",
            &ko,
            Box::new(|d| d["fight"]["ruleset_version"] = json!(1.5)),
            "undeserializable",
            "mismatch:data",
        ),
        (
            "data_a_final_without_a_kind",
            &ko,
            Box::new(|d| d["fight"]["final"] = json!({ "reason": "ko" })),
            "undeserializable",
            "mismatch:data",
        ),
        (
            "data_a_final_kind_that_is_not_text",
            &ko,
            Box::new(|d| d["fight"]["final"] = json!({ "kind": 1 })),
            "undeserializable",
            "mismatch:data",
        ),
    ];
    for (name, base, change, native, browser) in cases {
        let mut detail = base.clone();
        change(&mut detail);
        out.push(vector_line(name, detail, native, browser));
    }

    // D2: `from_str_radix` took `+a` for `0a`, so the bytes are the ones of the commit.
    let plain_seed = [0x0a_u8; 32];
    let mut plus = voided_detail(&plain_seed, VoidReason::EngineError);
    plus["seed"] = json!(format!("+a{}", "0a".repeat(31)));
    out.push(vector_line(
        "seed_with_a_plus_sign",
        plus,
        "err",
        "mismatch:commit",
    ));
    out
}

/// 011 RF14: what this verifier says of each line of the file, read from the file. A line that does
/// not deserialize is a verdict too: only a value this version does not know can reach that far.
#[test]
fn the_native_verifier_answers_what_the_vectors_say() {
    let text = std::fs::read_to_string(VECTORS_FILE)
        .expect("no vectors: UPDATE_GOLDEN=1 cargo test -p server --test fights_public_verify");
    let vectors: Vec<Value> = serde_json::from_str(&text).unwrap();
    let mut oks = 0;
    for vector in &vectors {
        let name = vector["name"].as_str().unwrap();
        let got = match serde_json::from_value::<FightDetail>(vector["detail"].clone()) {
            Err(_) => "undeserializable",
            Ok(detail) => match verify(&detail) {
                Ok(()) => {
                    oks += 1;
                    "ok"
                }
                Err(_) => "err",
            },
        };
        assert_eq!(got, vector["native"].as_str().unwrap(), "{name}");
    }
    // Otherwise this test passes in vacuum.
    assert!(oks >= 7, "{oks}");
}

/// 011 RF14: the relation of the two verifiers: `Ok` is `match` or `not_applicable`, `Err` is
/// `mismatch`, and only the marked ones are `cannot_check`. Nothing that fails is ever a `match`.
#[test]
fn the_browser_column_keeps_the_frontier_of_the_spec() {
    let vectors = verify_vectors();
    let mut names = BTreeSet::new();
    let mut browsers = BTreeSet::new();
    for vector in &vectors {
        let name = vector["name"].as_str().unwrap();
        assert!(names.insert(name.to_owned()), "{name} twice");
        let browser = vector["browser"].as_str().unwrap();
        browsers.insert(browser.to_owned());
        let exception = vector["exception"].as_bool().unwrap();
        assert_eq!(exception, browser.starts_with("cannot_check:"), "{name}");
        let (native, browser_kind) = (
            vector["native"].as_str().unwrap(),
            browser.split(':').next().unwrap(),
        );
        match native {
            "ok" => assert!(matches!(browser_kind, "match" | "not_applicable"), "{name}"),
            "err" => assert!(
                matches!(browser_kind, "mismatch" | "cannot_check"),
                "{name}"
            ),
            "undeserializable" => {
                assert!(
                    matches!(browser_kind, "mismatch" | "cannot_check"),
                    "{name}"
                );
            }
            other => panic!("{name}: {other}"),
        }
    }
    for expected in [
        "match:fight",
        "match:commit",
        "match:snapshot",
        "not_applicable",
        "mismatch:data",
        "mismatch:state",
        "mismatch:commit",
        "mismatch:snapshot",
        "mismatch:log",
        "mismatch:final",
        "mismatch:summary",
        "mismatch:bounds",
        "cannot_check:ruleset_unknown",
        "cannot_check:unknown_final",
    ] {
        assert!(browsers.contains(expected), "no vector for {expected}");
    }
    for base in [
        "ko",
        "decision",
        "draw",
        "voided_engine_error",
        "voided_snapshot_invalid",
        "cancelled_missed_window",
        "cancelled_fight_unrunnable",
    ] {
        assert!(names.contains(base), "{base}");
    }
}

/// 011 RF14: the file is what this test builds, byte by byte, or it is written with `UPDATE_GOLDEN=1`.
#[test]
fn the_vectors_on_file_are_up_to_date() {
    let mut text = serde_json::to_string_pretty(&verify_vectors()).unwrap();
    text.push('\n');
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(VECTORS_FILE, &text).unwrap();
    }
    let on_file = std::fs::read_to_string(VECTORS_FILE).unwrap_or_default();
    assert!(
        on_file == text,
        "fight-verify-vectors.json is out of date: \
         UPDATE_GOLDEN=1 cargo test -p server --test fights_public_verify"
    );
}
