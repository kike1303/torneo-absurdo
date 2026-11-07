//! 012 RF16 and RF18, ADR-0012 A8: the real binary of `server fight void`. Its exit codes, its line of
//! fixed text, and logs with only `event` and `error_kind`, at every level: no seed in any form, no
//! log, no id. And a failure that is not a refusal has its own event (RF16).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use server::fights::hex;
use sqlx::PgPool;
use support::bets;
use support::bin;
use support::db::{TestDb, privileged};
use support::fights::{self as f, KO, T0, Vector};
use uuid::Uuid;

const LEVELS: [&str; 3] = ["info", "trace", "debug,sqlx=trace"];

async fn run(args: &[&str], envs: &[(&str, &str)]) -> bin::Finished {
    bin::spawn(args, envs).finish(Duration::from_secs(30)).await
}

/// The lines that are not a log: the output of the command.
fn output(finished: &bin::Finished) -> Vec<&str> {
    finished
        .stdout
        .iter()
        .filter(|line| line.starts_with("void."))
        .map(String::as_str)
        .collect()
}

fn events(finished: &bin::Finished) -> Vec<serde_json::Value> {
    finished
        .stdout
        .iter()
        .filter(|line| !line.starts_with("void."))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The event `name` is there, with this code, and has nothing else of its own.
fn assert_event(finished: &bin::Finished, name: &str, kind: Option<&str>, case: &str) {
    let lines = events(finished);
    let own: Vec<_> = lines.iter().filter(|l| l["event"] == name).collect();
    assert_eq!(own.len(), 1, "{case}: {:?}", finished.stdout);
    let mut keys: Vec<_> = own[0].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    match kind {
        Some(kind) => {
            assert_eq!(keys, ["error_kind", "event", "level", "ts"], "{case}");
            assert_eq!(own[0]["error_kind"], kind, "{case}");
        }
        None => assert_eq!(keys, ["event", "level", "ts"], "{case}"),
    }
}

fn assert_clean(finished: &bin::Finished, vector: &Vector, admin: Uuid, case: &str) {
    let seed = vector.seed.as_slice();
    let mut canaries = vec![
        hex(seed),
        hex(seed).to_uppercase(),
        STANDARD.encode(seed),
        URL_SAFE_NO_PAD.encode(seed),
        serde_json::to_string(seed).unwrap(),
        format!("{seed:?}"),
        String::from_utf8(vector.canonical()).unwrap(),
        admin.to_string(),
        admin.simple().to_string(),
    ];
    canaries.push(String::from_utf8(engine::canonical_spec_json(&vector.a).unwrap()).unwrap());
    for line in finished.stdout.iter().chain(&finished.stderr) {
        for canary in &canaries {
            assert!(!line.contains(canary.as_str()), "{case}: {line}");
        }
    }
    assert!(finished.stderr.is_empty(), "{case}: {:?}", finished.stderr);
}

async fn admin_of(pool: &PgPool) -> Uuid {
    let admin = bets::user(pool).await;
    privileged(
        pool,
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
    )
    .await;
    admin
}

/// A closed fight of `vector`, with these changes.
async fn closed(
    pool: &PgPool,
    slot: i64,
    vector: &Vector,
    change: impl FnOnce(&mut f::RawFight),
) -> i32 {
    let opens = T0 + slot * f::CADENCE;
    let id = f::announce_with(pool, opens, &vector.a, &vector.b, change).await;
    f::open_with(pool, id, &vector.seed, opens).await;
    f::close_by_sql(pool, id, opens + f::WINDOW).await;
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_binary_applies_and_refuses_with_clean_logs() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let url = db.url();
    let vector = Vector::of(KO);
    let admin = admin_of(&pool).await;
    let by = admin.to_string();
    let user = bets::user(&pool).await.to_string();
    for (slot, level) in (0_i64..).zip(LEVELS) {
        let envs = [("DATABASE_URL", url.as_str()), ("RUST_LOG", level)];
        // Applied.
        let bad = closed(&pool, slot * 3, &vector, |raw| {
            raw.snapshot_sha256 = vec![0; 32];
        })
        .await
        .to_string();
        let done = run(&["fight", "void", &bad, "--by", &by], &envs).await;
        assert_eq!(done.status.code(), Some(0), "{level}: {:?}", done.stdout);
        assert_eq!(output(&done), ["void.applied snapshot_invalid"], "{level}");
        assert_event(&done, "fight.void_applied", None, level);
        assert_clean(&done, &vector, admin, level);

        // A healthy fight.
        let healthy = closed(&pool, slot * 3 + 1, &vector, |_| {})
            .await
            .to_string();
        let done = run(&["fight", "void", &healthy, "--by", &by], &envs).await;
        assert_eq!(done.status.code(), Some(1), "{level}");
        assert_eq!(output(&done), ["void.fight_healthy"], "{level}");
        assert_event(
            &done,
            "fight.void_refused",
            Some("void.fight_healthy"),
            level,
        );
        assert_clean(&done, &vector, admin, level);

        // Not an admin.
        let done = run(&["fight", "void", &healthy, "--by", &user], &envs).await;
        assert_eq!(done.status.code(), Some(1), "{level}");
        assert_eq!(output(&done), ["void.not_admin"], "{level}");
        assert_event(&done, "fight.void_refused", Some("void.not_admin"), level);

        // An invalid line reads nothing: the reason is never chosen.
        let done = run(
            &[
                "fight",
                "void",
                &healthy,
                "--by",
                &by,
                "--reason",
                "engine_error",
            ],
            &envs,
        )
        .await;
        assert_eq!(done.status.code(), Some(2), "{level}");
        assert!(output(&done).is_empty(), "{level}");
    }
}

/// RF18 and RF24: a `secret_lost` by the binary: its two lines, the flag it asks for first, and no
/// id nor seed in the logs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_binary_voids_a_lost_secret() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let url = db.url();
    let vector = Vector::of(KO);
    let admin = admin_of(&pool).await;
    let by = admin.to_string();
    let now: i64 = sqlx::query_scalar("SELECT (extract(epoch FROM now()) * 1000)::bigint")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (slot, level) in (1_i64..).zip(LEVELS) {
        let opens = now - slot * 10 * f::CADENCE;
        let id = f::announce(&pool, opens, &vector.a, &vector.b).await;
        f::open_with(&pool, id, &vector.seed, opens).await;
        f::close_by_sql(&pool, id, opens + f::WINDOW).await;
        f::lose_secret(&pool, id).await;
        let id = id.to_string();
        let envs = [("DATABASE_URL", url.as_str()), ("RUST_LOG", level)];
        let done = run(&["fight", "void", &id, "--by", &by], &envs).await;
        assert_eq!(done.status.code(), Some(1), "{level}");
        assert_eq!(output(&done), ["void.backup_not_checked"], "{level}");
        assert_event(
            &done,
            "fight.void_refused",
            Some("void.backup_not_checked"),
            level,
        );
        let done = run(
            &["fight", "void", &id, "--by", &by, "--backup-checked"],
            &envs,
        )
        .await;
        assert_eq!(done.status.code(), Some(0), "{level}: {:?}", done.stdout);
        assert_eq!(
            output(&done),
            ["void.applied secret_lost", server::admin::REOPEN_ACCEPTANCE],
            "{level}"
        );
        assert_event(&done, "fight.void_applied", None, level);
        assert_clean(&done, &vector, admin, level);

        // A deadline in the `COMMIT` loses those lines: a second run gives the warning again.
        let done = run(
            &["fight", "void", &id, "--by", &by, "--backup-checked"],
            &envs,
        )
        .await;
        assert_eq!(done.status.code(), Some(1), "{level}");
        assert_eq!(
            output(&done),
            ["void.bad_state", server::admin::REOPEN_ACCEPTANCE],
            "{level}"
        );
        assert_event(&done, "fight.void_refused", Some("void.bad_state"), level);
    }
}

/// RF16: only `DATABASE_URL` and `RUST_LOG`, and a failure that is not a refusal says so: exit 1, its
/// own event with its kind, no line of output, and nothing written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_is_not_a_refusal() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let url = db.url();
    let vector = Vector::of(KO);
    let admin = admin_of(&pool).await;
    let by = admin.to_string();
    let bad = closed(&pool, 0, &vector, |raw| raw.snapshot_sha256 = vec![0; 32]).await;
    let id = bad.to_string();
    let envs = [("DATABASE_URL", url.as_str())];

    // Someone holds the row for longer than the 5 s of RF16.
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM fights WHERE id = $1 FOR UPDATE")
        .bind(bad)
        .execute(&mut *holder)
        .await
        .unwrap();
    let before = bets::written(&pool).await;
    let done = run(&["fight", "void", &id, "--by", &by], &envs).await;
    assert_eq!(done.status.code(), Some(1), "{:?}", done.stdout);
    assert!(output(&done).is_empty(), "{:?}", done.stdout);
    assert_event(&done, "fight.void_failed", Some("lock_timeout"), "lock");
    assert!(
        events(&done)
            .iter()
            .all(|l| l["event"] != "fight.void_refused")
    );
    assert_clean(&done, &vector, admin, "lock");
    holder.rollback().await.unwrap();
    assert_eq!(bets::written(&pool).await, before);

    // A database that does not answer.
    let port = support::free_port();
    let nowhere = format!("postgres://torneo:unused@127.0.0.1:{port}/torneo");
    let done = run(
        &["fight", "void", &id, "--by", &by],
        &[("DATABASE_URL", nowhere.as_str())],
    )
    .await;
    assert_eq!(done.status.code(), Some(1), "{:?}", done.stdout);
    assert!(output(&done).is_empty());
    assert_event(&done, "fight.void_failed", Some("io"), "no database");
    assert_eq!(bets::written(&pool).await, before);

    // With the row free, and only these two variables, it applies: nothing else of `serve` is read.
    let done = bin::spawn(&["fight", "void", &id, "--by", &by], &envs)
        .finish(Duration::from_secs(30))
        .await;
    assert_eq!(done.status.code(), Some(0), "{:?}", done.stdout);
}

/// The help documents the command without reading anything, as the rest of `admin --help`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_help_documents_the_command() {
    bin::warm_up().await;
    let done = run(&["admin", "--help"], &[]).await;
    assert_eq!(done.status.code(), Some(0));
    let text = done.stdout.join("\n");
    for words in [
        "server fight void",
        "--by",
        "--previous-binary",
        "--backup-checked",
        "deduced",
    ] {
        assert!(text.contains(words), "{words}");
    }
}
