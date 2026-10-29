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
    let canary = format!("pglog-canary-{}", uuid::Uuid::new_v4().simple());
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join("pglog-canary.txt"), &canary).unwrap();

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
