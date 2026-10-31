//! 010 RF4, RF8, RF9, RF25 and the base of RF59: what the database alone enforces, with direct SQL
//! and one case per rule. Nothing here goes through the server: these rules hold for any code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, PgPool, Row};
use support::db::TestDb;

const SEED: [u8; 32] = [7; 32];

fn commit() -> Vec<u8> {
    Sha256::digest(SEED).to_vec()
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
}

/// A scheduled fight, in the hour `slot` so that many fit in one database.
async fn scheduled(pool: &PgPool, slot: i32) -> i32 {
    sqlx::query(
        "INSERT INTO fights (state, opens_at, closes_at, live_at, ruleset_version, spec_a, spec_b, \
         snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, time_limit_ms, max_duration_ms, \
         matchup_round, roster_sha256) \
         VALUES ('scheduled', \
         timestamptz '2030-01-01' + $1 * interval '1 hour', \
         timestamptz '2030-01-01' + $1 * interval '1 hour' + interval '10 minutes', \
         timestamptz '2030-01-01' + $1 * interval '1 hour' + interval '11 minutes', \
         1, '\\x7b7d', '\\x7b7d', $2, 'x-a', 'x-b', 100, 200, 86000, 90000, 1, $2) \
         RETURNING id",
    )
    .bind(slot)
    .bind(vec![1_u8; 32])
    .fetch_one(pool)
    .await
    .unwrap()
    .get(0)
}

async fn secret(pool: &PgPool, id: i32) {
    sqlx::query("INSERT INTO fight_secrets (fight_id, seed) VALUES ($1, $2)")
        .bind(id)
        .bind(SEED.to_vec())
        .execute(pool)
        .await
        .unwrap();
}

async fn run(pool: &PgPool, sql: &str, id: i32) -> Result<u64, sqlx::Error> {
    sqlx::query(AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .bind(SEED.to_vec())
        .bind(commit())
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

async fn refused(pool: &PgPool, sql: &str, id: i32) {
    let error = run(pool, sql, id).await.expect_err(sql);
    assert_eq!(sqlstate(&error).as_deref(), Some("TA006"), "{sql}");
    assert_eq!(server::db::error_kind(&error), "fight_guard", "{sql}");
}

async fn accepted(pool: &PgPool, sql: &str, id: i32) {
    assert_eq!(run(pool, sql, id).await.expect(sql), 1, "{sql}");
}

// Every statement binds the same three values, so a typed unused one is harmless: `$2` is the
// seed and `$3` the commit, cast so that they are always typed.
const TYPES: &str = " AND $2::bytea IS NOT NULL AND $3::bytea IS NOT NULL";

fn sql(update: &str) -> String {
    format!("UPDATE fights SET {update} WHERE id = $1{TYPES}")
}

const TO_OPEN: &str = "state = 'betting_open', seed_commit = $3, opened_at = now()";
const TO_CLOSED: &str = "state = 'betting_closed', closed_at = now()";
const TO_LIVE: &str = "state = 'live', live_started_at = now()";
const TO_DECIDED: &str = "state = 'settled', settled_at = now(), winner = 'a', \
                          end_reason = 'ko', revealed_seed = $2, log = '\\x7b7d'";
const TO_VOIDED: &str = "state = 'settled', settled_at = now(), void_reason = 'engine_error', \
                         revealed_seed = $2";
const TO_CANCELLED: &str = "state = 'settled', settled_at = now(), \
                            cancel_reason = 'missed_window'";

/// A fight legally taken to `state`.
async fn fight_in(pool: &PgPool, slot: i32, state: &str) -> i32 {
    let id = scheduled(pool, slot).await;
    if state == "scheduled" {
        return id;
    }
    secret(pool, id).await;
    accepted(pool, &sql(TO_OPEN), id).await;
    if state == "betting_open" {
        return id;
    }
    accepted(pool, &sql(TO_CLOSED), id).await;
    if state == "betting_closed" {
        return id;
    }
    accepted(pool, &sql(TO_LIVE), id).await;
    if state == "live" {
        return id;
    }
    accepted(pool, &sql(TO_DECIDED), id).await;
    assert_eq!(state, "settled");
    id
}

#[tokio::test]
async fn a_row_is_born_scheduled_with_its_whole_cycle_ahead() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    const INSERT: &str = "INSERT INTO fights (state, opens_at, closes_at, live_at, \
        ruleset_version, spec_a, spec_b, snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, \
        time_limit_ms, max_duration_ms, matchup_round, roster_sha256";
    const VALUES: &str = "timestamptz '2031-01-01', timestamptz '2031-01-02', \
        timestamptz '2031-01-03', 1, '\\x7b7d', '\\x7b7d', $2, 'a', 'b', 1, 1, 1, 1, 1, $2";
    for state in ["betting_open", "betting_closed", "live", "settled"] {
        let insert = format!("{INSERT}) VALUES ('{state}', {VALUES}) RETURNING $1::int, $3::bytea");
        let error = sqlx::query(AssertSqlSafe(insert))
            .bind(0)
            .bind(vec![1_u8; 32])
            .bind(commit())
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA006"), "{state}");
    }
    for (column, value) in [
        ("seed_commit", "$2"),
        ("opened_at", "now()"),
        ("closed_at", "now()"),
        ("live_started_at", "now()"),
        ("settled_at", "now()"),
        ("winner", "'a'"),
        ("end_reason", "'ko'"),
        ("cancel_reason", "'missed_window'"),
        ("void_reason", "'engine_error'"),
        ("revealed_seed", "$2"),
        ("log", "'\\x7b7d'"),
    ] {
        let insert = format!(
            "{INSERT}, {column}) VALUES ('scheduled', {VALUES}, {value}) \
             RETURNING $1::int, $3::bytea"
        );
        let error = sqlx::query(AssertSqlSafe(insert))
            .bind(0)
            .bind(vec![1_u8; 32])
            .bind(commit())
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA006"), "{column}");
    }
}

#[tokio::test]
async fn what_was_fixed_at_the_announcement_never_changes() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    for (slot, state) in ["scheduled", "betting_open", "betting_closed", "live"]
        .into_iter()
        .enumerate()
    {
        let id = fight_in(&pool, i32::try_from(slot).unwrap(), state).await;
        for update in [
            "id = DEFAULT",
            "opens_at = opens_at + interval '1 second'",
            "closes_at = closes_at + interval '1 second'",
            "live_at = live_at + interval '1 second'",
            "ruleset_version = 2",
            "spec_a = '\\x7b20207d'",
            "spec_b = '\\x7b20207d'",
            "snapshot_sha256 = $3",
            "slug_a = 'z'",
            "slug_b = 'z'",
            "max_hp_a = 1",
            "max_hp_b = 1",
            "time_limit_ms = 1",
            "max_duration_ms = 99999",
            "matchup_round = 9",
            "roster_sha256 = $3",
        ] {
            refused(&pool, &sql(update), id).await;
        }
    }
}

#[tokio::test]
async fn a_written_value_is_never_rewritten_not_even_in_a_transition() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let open = fight_in(&pool, 0, "betting_open").await;
    for update in [
        "seed_commit = $2",
        "seed_commit = NULL",
        "opened_at = now() + interval '1 day'",
    ] {
        refused(&pool, &sql(&format!("{TO_CLOSED}, {update}")), open).await;
    }
    let closed = fight_in(&pool, 1, "betting_closed").await;
    refused(
        &pool,
        &sql(&format!("{TO_LIVE}, closed_at = now() + interval '1 day'")),
        closed,
    )
    .await;
    let live = fight_in(&pool, 2, "live").await;
    refused(
        &pool,
        &sql(&format!(
            "{TO_DECIDED}, live_started_at = now() + interval '1 day'"
        )),
        live,
    )
    .await;
}

#[tokio::test]
async fn every_update_is_a_transition() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let scheduled_id = fight_in(&pool, 0, "scheduled").await;
    refused(&pool, &sql("seed_commit = $3"), scheduled_id).await;
    refused(&pool, &sql("state = state"), scheduled_id).await;
    let open = fight_in(&pool, 1, "betting_open").await;
    refused(&pool, &sql("closed_at = now()"), open).await;
    refused(&pool, &sql("state = state"), open).await;
}

#[tokio::test]
async fn only_the_edges_of_the_graph_are_accepted() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let states = [
        "scheduled",
        "betting_open",
        "betting_closed",
        "live",
        "settled",
    ];
    let mut slot = 0;
    for from in states {
        for (to, update) in [
            ("betting_open", TO_OPEN),
            ("betting_closed", TO_CLOSED),
            ("live", TO_LIVE),
            ("settled", TO_DECIDED),
            ("settled", TO_VOIDED),
            ("settled", TO_CANCELLED),
            ("scheduled", "state = 'scheduled'"),
        ] {
            let id = fight_in(&pool, slot, from).await;
            slot += 1;
            let legal = matches!(
                (from, to, update),
                ("scheduled", "betting_open", _)
                    | ("betting_open", "betting_closed", _)
                    | ("betting_closed", "live", _)
                    | ("live", "settled", TO_DECIDED)
                    | ("live", "settled", TO_VOIDED)
                    | ("betting_closed", "settled", TO_VOIDED)
                    | ("scheduled", "settled", TO_CANCELLED)
            );
            if legal {
                if update == TO_OPEN {
                    secret(&pool, id).await;
                }
                accepted(&pool, &sql(update), id).await;
            } else {
                refused(&pool, &sql(update), id).await;
            }
        }
    }
}

#[tokio::test]
async fn each_edge_requires_the_columns_it_writes_and_writes_no_other() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let cases = [
        ("scheduled", "state = 'betting_open', opened_at = now()"),
        ("scheduled", "state = 'betting_open', seed_commit = $3"),
        (
            "scheduled",
            "state = 'betting_open', seed_commit = $3, opened_at = now(), closed_at = now()",
        ),
        ("betting_open", "state = 'betting_closed'"),
        (
            "betting_open",
            "state = 'betting_closed', closed_at = now(), live_started_at = now()",
        ),
        ("betting_closed", "state = 'live'"),
        (
            "betting_closed",
            "state = 'live', live_started_at = now(), winner = 'a'",
        ),
        // Toward settled, without settled_at.
        (
            "live",
            "state = 'settled', winner = 'a', end_reason = 'ko', revealed_seed = $2, log = '\\x7b7d'",
        ),
        (
            "betting_closed",
            "state = 'settled', void_reason = 'engine_error', revealed_seed = $2",
        ),
        (
            "scheduled",
            "state = 'settled', cancel_reason = 'missed_window'",
        ),
    ];
    for (slot, (from, update)) in cases.into_iter().enumerate() {
        let id = fight_in(&pool, i32::try_from(slot).unwrap(), from).await;
        refused(&pool, &sql(update), id).await;
    }
}

#[tokio::test]
async fn the_final_is_unique_and_follows_its_rules() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    const SETTLE: &str = "state = 'settled', settled_at = now()";
    let mut slot = 0;
    let cases: Vec<(&str, String)> = vec![
        // No final at all.
        ("live", format!("{SETTLE}, revealed_seed = $2")),
        // decided: only from live, with a log, a seed and a coherent winner.
        (
            "betting_closed",
            format!(
                "{SETTLE}, winner = 'a', end_reason = 'ko', revealed_seed = $2, log = '\\x7b7d'"
            ),
        ),
        (
            "live",
            format!("{SETTLE}, winner = 'a', end_reason = 'ko', revealed_seed = $2"),
        ),
        (
            "live",
            format!("{SETTLE}, winner = 'a', end_reason = 'ko', log = '\\x7b7d'"),
        ),
        (
            "live",
            format!(
                "{SETTLE}, winner = 'a', end_reason = 'draw', revealed_seed = $2, log = '\\x7b7d'"
            ),
        ),
        (
            "live",
            format!("{SETTLE}, end_reason = 'ko', revealed_seed = $2, log = '\\x7b7d'"),
        ),
        (
            "live",
            format!("{SETTLE}, winner = 'a', revealed_seed = $2, log = '\\x7b7d'"),
        ),
        // A seed that does not match the commit.
        (
            "live",
            format!(
                "{SETTLE}, winner = 'a', end_reason = 'ko', revealed_seed = $3, log = '\\x7b7d'"
            ),
        ),
        // cancelled: never from live, never with a commit.
        (
            "live",
            format!("{SETTLE}, cancel_reason = 'missed_window', revealed_seed = $2"),
        ),
        (
            "betting_open",
            format!("{SETTLE}, cancel_reason = 'missed_window'"),
        ),
        (
            "scheduled",
            format!("{SETTLE}, cancel_reason = 'missed_window', seed_commit = $3"),
        ),
        // voided: never without its seed, never with a log, never before the close.
        ("live", format!("{SETTLE}, void_reason = 'engine_error'")),
        (
            "live",
            format!("{SETTLE}, void_reason = 'engine_error', revealed_seed = $3"),
        ),
        (
            "live",
            format!("{SETTLE}, void_reason = 'engine_error', revealed_seed = $2, log = '\\x7b7d'"),
        ),
        (
            "betting_open",
            format!("{SETTLE}, void_reason = 'engine_error', revealed_seed = $2"),
        ),
        (
            "scheduled",
            format!("{SETTLE}, void_reason = 'engine_error', revealed_seed = $2"),
        ),
        // Two finals.
        (
            "live",
            format!(
                "{SETTLE}, winner = 'a', end_reason = 'ko', void_reason = 'engine_error', revealed_seed = $2"
            ),
        ),
    ];
    for (from, update) in &cases {
        let id = fight_in(&pool, slot, from).await;
        slot += 1;
        refused(&pool, &sql(update), id).await;
    }

    // A draw and a win are both accepted, and a settled row admits nothing else.
    let draw = fight_in(&pool, slot, "live").await;
    slot += 1;
    accepted(
        &pool,
        &sql(&format!(
            "{SETTLE}, end_reason = 'draw', revealed_seed = $2, log = '\\x7b7d'"
        )),
        draw,
    )
    .await;
    refused(&pool, &sql("winner = 'a'"), draw).await;
    refused(&pool, &sql("end_reason = 'ko'"), draw).await;
    refused(&pool, &sql("state = 'live'"), draw).await;
    refused(&pool, &sql("settled_at = now()"), draw).await;
    let cancelled = fight_in(&pool, slot, "scheduled").await;
    slot += 1;
    accepted(&pool, &sql(TO_CANCELLED), cancelled).await;
    refused(&pool, &sql("cancel_reason = 'fight_unrunnable'"), cancelled).await;
    let voided = fight_in(&pool, slot, "betting_closed").await;
    accepted(&pool, &sql(TO_VOIDED), voided).await;
    refused(&pool, &sql("void_reason = 'snapshot_invalid'"), voided).await;
}

#[tokio::test]
async fn fights_are_never_deleted_nor_truncated() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let id = fight_in(&pool, 0, "settled").await;
    refused(
        &pool,
        &format!("DELETE FROM fights WHERE id = $1{TYPES}"),
        id,
    )
    .await;
    for table in ["fights", "fight_secrets"] {
        let error = sqlx::query(AssertSqlSafe(format!("TRUNCATE {table} CASCADE")))
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA006"), "{table}");
    }
}

#[tokio::test]
async fn the_seed_table_follows_its_rules() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    const UPDATE: &str =
        "UPDATE fight_secrets SET seed = $3 WHERE fight_id = $1 AND $2::bytea IS NOT NULL";
    const DELETE: &str = "DELETE FROM fight_secrets WHERE fight_id = $1 AND $2::bytea IS NOT NULL AND $3::bytea IS NOT NULL";
    const WRONG: &str = "INSERT INTO fight_secrets (fight_id, seed) \
        SELECT $1, $3 WHERE $2::bytea IS NOT NULL";
    const RIGHT: &str = "INSERT INTO fight_secrets (fight_id, seed) \
        SELECT $1, $2 WHERE $3::bytea IS NOT NULL";

    // R2: no commit is published without its seed, or with another one.
    let no_secret = scheduled(&pool, 0).await;
    refused(&pool, &sql(TO_OPEN), no_secret).await;
    let other_secret = scheduled(&pool, 1).await;
    accepted(&pool, WRONG, other_secret).await;
    refused(&pool, &sql(TO_OPEN), other_secret).await;

    // No UPDATE, ever; no DELETE before settled; in those states the secret stays.
    let mut slot = 2;
    for state in ["betting_open", "betting_closed", "live"] {
        let id = fight_in(&pool, slot, state).await;
        slot += 1;
        refused(&pool, UPDATE, id).await;
        refused(&pool, DELETE, id).await;
        // R2: with a commit published, only the secret that matches it can be inserted. A restore
        // goes through here (worry 13), once the row is gone.
        let error = run(&pool, WRONG, id).await.unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA006"), "{state}");
    }

    // The restore of worry 13: the row was lost (only with the guard off), and the right secret
    // goes back in. The wrong one does not.
    let lost = fight_in(&pool, slot, "live").await;
    slot += 1;
    sqlx::query("ALTER TABLE fight_secrets DISABLE TRIGGER fight_secrets_guard")
        .execute(&pool)
        .await
        .unwrap();
    run(&pool, DELETE, lost).await.unwrap();
    sqlx::query("ALTER TABLE fight_secrets ENABLE TRIGGER fight_secrets_guard")
        .execute(&pool)
        .await
        .unwrap();
    refused(&pool, WRONG, lost).await;
    accepted(&pool, RIGHT, lost).await;

    // Settled: the DELETE passes. The settling transaction does it, and sees its own state.
    let live = fight_in(&pool, slot, "live").await;
    slot += 1;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(AssertSqlSafe(sql(TO_DECIDED)))
        .bind(live)
        .bind(SEED.to_vec())
        .bind(commit())
        .execute(&mut *tx)
        .await
        .unwrap();
    // Another transaction does not see that state yet.
    let error = run(&pool, DELETE, live).await;
    assert!(
        matches!(&error, Err(e) if sqlstate(e).as_deref() == Some("TA006")),
        "{error:?}"
    );
    sqlx::query("DELETE FROM fight_secrets WHERE fight_id = $1")
        .bind(live)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // DP-6: and once settled, nothing goes back in.
    refused(&pool, RIGHT, live).await;
    let cancelled = scheduled(&pool, slot).await;
    accepted(&pool, &sql(TO_CANCELLED), cancelled).await;
    refused(&pool, RIGHT, cancelled).await;

    // The size of a seed.
    let short = scheduled(&pool, slot + 1).await;
    let error = sqlx::query("INSERT INTO fight_secrets (fight_id, seed) VALUES ($1, '\\x00')")
        .bind(short)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("23514"));
}

/// The rules a CHECK can express hold with the guard off (M2), which also gives their real code.
#[tokio::test]
async fn the_checks_are_a_second_barrier() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let open = fight_in(&pool, 0, "betting_open").await;
    let closed = fight_in(&pool, 1, "betting_closed").await;
    let live = fight_in(&pool, 2, "live").await;
    let scheduled_id = scheduled(&pool, 3).await;
    sqlx::query("ALTER TABLE fights DISABLE TRIGGER fights_guard")
        .execute(&pool)
        .await
        .unwrap();
    const SETTLE: &str = "state = 'settled', settled_at = now()";
    let cases: Vec<(i32, String, &str)> = vec![
        (
            scheduled_id,
            "state = 'betting_open'".to_owned(),
            "fights_commit_ck",
        ),
        (
            open,
            "revealed_seed = $2".to_owned(),
            "fights_revealed_seed_ck",
        ),
        (open, "log = '\\x7b7d'".to_owned(), "fights_log_ck"),
        (live, SETTLE.to_owned(), "fights_final_ck"),
        (live, "state = 'settled'".to_owned(), "fights_final_ck"),
        (
            open,
            "settled_at = now()".to_owned(),
            "fights_settled_at_ck",
        ),
        (
            live,
            format!("{SETTLE}, winner = 'a', void_reason = 'engine_error', revealed_seed = $2"),
            "fights_winner_ck",
        ),
        (
            live,
            format!(
                "{SETTLE}, winner = 'a', end_reason = 'draw', revealed_seed = $2, log = '\\x7b7d'"
            ),
            "fights_draw_ck",
        ),
        (
            live,
            format!("{SETTLE}, end_reason = 'draw', revealed_seed = $2"),
            "fights_decided_log_ck",
        ),
        (
            live,
            format!("{SETTLE}, cancel_reason = 'missed_window', revealed_seed = $2"),
            "fights_cancelled_ck",
        ),
        (
            closed,
            format!("{SETTLE}, void_reason = 'engine_error', revealed_seed = $2, log = '\\x7b7d'"),
            "fights_voided_ck",
        ),
        (
            closed,
            format!("{SETTLE}, void_reason = 'engine_error'"),
            "fights_reveal_ck",
        ),
        (
            closed,
            format!("{SETTLE}, void_reason = 'engine_error', revealed_seed = $3"),
            "fights_reveal_ck",
        ),
    ];
    for (id, update, constraint) in cases {
        let error = run(&pool, &sql(&update), id).await.expect_err(&update);
        assert_eq!(sqlstate(&error).as_deref(), Some("23514"), "{update}");
        let name = error.as_database_error().and_then(|e| e.constraint());
        assert_eq!(name, Some(constraint), "{update}");
    }
    sqlx::query("ALTER TABLE fights ENABLE TRIGGER fights_guard")
        .execute(&pool)
        .await
        .unwrap();
}

/// DP-3: the bounds of the summary are only CHECKs, so they answer 23514 at the announcement.
#[tokio::test]
async fn the_bounds_of_the_summary_and_the_hours_are_checks() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    for (time_limit, max_duration, closes, live, constraint) in [
        (
            0,
            90_000,
            "2031-01-02",
            "2031-01-03",
            "fights_time_limit_ms_ck",
        ),
        (
            86_000,
            85_999,
            "2031-01-02",
            "2031-01-03",
            "fights_max_duration_ms_ck",
        ),
        (
            86_000,
            90_000,
            "2031-01-01",
            "2031-01-03",
            "fights_hours_ck",
        ),
        (
            86_000,
            90_000,
            "2031-01-02",
            "2031-01-01",
            "fights_hours_ck",
        ),
    ] {
        let error = sqlx::query(
            "INSERT INTO fights (state, opens_at, closes_at, live_at, ruleset_version, spec_a, \
             spec_b, snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, time_limit_ms, \
             max_duration_ms, matchup_round, roster_sha256) \
             VALUES ('scheduled', timestamptz '2031-01-01', $1::timestamptz, $2::timestamptz, 1, \
             '\\x7b7d', '\\x7b7d', $3, 'a', 'b', 1, 1, $4, $5, 1, $3)",
        )
        .bind(closes)
        .bind(live)
        .bind(vec![1_u8; 32])
        .bind(time_limit)
        .bind(max_duration)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("23514"), "{constraint}");
        assert_eq!(
            error.as_database_error().and_then(|e| e.constraint()),
            Some(constraint)
        );
    }
    // UNIQUE (opens_at).
    let id = scheduled(&pool, 0).await;
    let error = sqlx::query(
        "INSERT INTO fights (state, opens_at, closes_at, live_at, ruleset_version, spec_a, spec_b, \
         snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, time_limit_ms, max_duration_ms, \
         matchup_round, roster_sha256) \
         SELECT state, opens_at, closes_at, live_at, ruleset_version, spec_a, spec_b, \
         snapshot_sha256, slug_a, slug_b, max_hp_a, max_hp_b, time_limit_ms, max_duration_ms, \
         matchup_round, roster_sha256 FROM fights WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(server::db::is_unique_violation(
        &error,
        "fights_opens_at_key"
    ));
}
