//! ADR-0012 A9 and 008 plan-b decision 13: with the log parameters of `infra/compose.dev.yml`,
//! no value of a failed statement reaches the Postgres log. Each case puts a canary where an
//! error would print it; `just _check-pg-log` then reads the container's log of the run and
//! fails if the canary is there, or if the log has no `ERROR` line at all (the positive control
//! below), so the check never passes on an empty log.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::db::TestDb;

#[tokio::test]
async fn failing_statements_never_log_their_values() {
    // One file per run (the file name is the canary), so `just _check-pg-log` checks every run
    // of the interval and never a stale value.
    let canary = format!("pglog-canary-{}", uuid::Uuid::new_v4().simple());
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join(format!("{canary}.txt")), &canary).unwrap();

    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;

    // A NOTIFY with the canary as payload.
    sqlx::query("SELECT pg_notify('session_revoked', $1)")
        .bind(&canary)
        .execute(&pool)
        .await
        .unwrap();

    // A unique violation with the canary in the key (`Key (…)=(…)` without A9).
    let insert = "INSERT INTO users (display_name, display_name_key) VALUES ($1, $1)";
    sqlx::query(insert)
        .bind(&canary)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        sqlx::query(insert)
            .bind(&canary)
            .execute(&pool)
            .await
            .is_err()
    );

    // A ledger UPDATE refused by the trigger, with the canary in the statement's values.
    let (user, _) = support::app::user_with_session(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    server::ledger::create_user_account(&mut tx, user)
        .await
        .unwrap();
    server::ledger::grant(&mut tx, user, server::ledger::Coins(5))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let refused = sqlx::query("UPDATE ledger_postings SET amount = length($1)")
        .bind(&canary)
        .execute(&pool)
        .await;
    assert!(refused.is_err());

    // An unbalanced entry (no postings) committed with the canary in the same transaction, for
    // a user without today's grant.
    let (user, _) = support::app::user_with_session(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
        .bind(&canary)
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    let entry = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO ledger_entries (id, kind, user_id) VALUES ($1, 'grant', $2)")
        .bind(entry)
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = tx.commit().await.unwrap_err();
    assert_eq!(server::db::error_kind(&error), "ledger_unbalanced");

    // The positive control: an error A9 does log (without its statement).
    assert!(sqlx::query("SELEC 1").execute(&pool).await.is_err());
}

/// 008 RF-C9 (plan-c decision 13): a rename that collides with the unique index, through the
/// real statement of the server over HTTP, never writes the name nor its key to the Postgres log
/// (`Key (display_name_key)=(…)` without A9). Its unique violation is also an `ERROR` line,
/// another positive control for `just _check-pg-log`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_colliding_rename_never_logs_the_name() {
    let id = uuid::Uuid::new_v4().simple().to_string();
    // `Pglog` and 16 hex characters: 21 graphemes, valid for D1 and not a reserved name.
    let name = format!("Pglog{}", &id[..16]);
    let display = server::profile::name::validate(&name).unwrap();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join(format!("pglog-canary-rename-{id}.txt")), &name).unwrap();
    // The key can differ from the name (case folding and the skeleton): it is what the DETAIL
    // of the violation would print.
    std::fs::write(
        dir.join(format!("pglog-canary-renamekey-{id}.txt")),
        display.key(),
    )
    .unwrap();

    let db = TestDb::create().await;
    let (app, pool) = support::app::start_db(&db, &[], &server::app::Settings::default()).await;
    sqlx::query("INSERT INTO users (display_name, display_name_key) VALUES ($1, $2)")
        .bind(display.as_str())
        .bind(display.key())
        .execute(&pool)
        .await
        .unwrap();
    let (_, token) = support::app::user_with_session(&pool).await;
    let name_of = support::config::config(&[]).cookies.session_name();
    let cookie = format!("{name_of}={token}");
    let body = serde_json::json!({ "display_name": name }).to_string();
    let refused = support::http::send_body(
        app.addr,
        "PUT",
        "/api/me/display-name",
        &[
            ("origin", support::app::ORIGIN),
            ("cookie", cookie.as_str()),
            ("content-type", "application/json"),
        ],
        &body,
    )
    .await;
    assert_eq!(refused.status, 409, "{}", refused.text());
    assert_eq!(refused.text(), r#"{"error":"profile.display_name_taken"}"#);
}
