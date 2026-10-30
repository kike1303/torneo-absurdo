//! 008 phase C, plan-c decisions 10 and 11: `admin_audit` is insert-only (the ledger trigger),
//! its `CHECK`s hold, and `users_privileged_guard` refuses every write of `role` or `banned_at`
//! outside a transaction that set `torneo.actor = 'cli'` locally.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::db::{TestDb, privileged};
use uuid::Uuid;

async fn plain_user(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, display_name, display_name_key) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Test{}", id.simple()))
        .bind(format!("test{}", id.simple()))
        .execute(pool)
        .await
        .unwrap();
    id
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
}

fn constraint(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.constraint())
        .map(str::to_owned)
}

async fn audit_row(pool: &PgPool, target: &str) {
    sqlx::query(
        "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
         VALUES ('cli', NULL, 'ban', $1, '{}'::jsonb)",
    )
    .bind(target)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn admin_audit_is_insert_only() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    audit_row(&pool, "t").await;
    for sql in [
        "UPDATE admin_audit SET target = 'x'",
        "DELETE FROM admin_audit",
        "TRUNCATE admin_audit",
    ] {
        let error = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA001"), "{sql}");
        assert_eq!(server::db::error_kind(&error), "ledger_immutable", "{sql}");
    }
    let count: i64 = sqlx::query("SELECT count(*) FROM admin_audit")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn admin_audit_checks_actor_admin_id_and_action() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = plain_user(&pool).await;
    let cases: [(&str, Option<Uuid>, &str, &str); 3] = [
        ("web", None, "ban", "admin_audit_actor_ck"),
        ("cli", Some(admin), "ban", "admin_audit_cli_ck"),
        ("cli", None, "delete", "admin_audit_action_ck"),
    ];
    for (actor, admin_id, action, name) in cases {
        let error = sqlx::query(
            "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
             VALUES ($1, $2, $3, 't', '{}'::jsonb)",
        )
        .bind(actor)
        .bind(admin_id)
        .bind(action)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            constraint(&error).as_deref(),
            Some(name),
            "{actor} {action}"
        );
    }
    for action in ["grant", "revoke", "ban", "unban"] {
        sqlx::query(
            "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
             VALUES ('cli', NULL, $1, 't', '{}'::jsonb)",
        )
        .bind(action)
        .execute(&pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn role_and_banned_at_are_written_only_by_the_cli_actor() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let user = plain_user(&pool).await;
    for sql in [
        "UPDATE users SET role = 'admin' WHERE id = $1",
        "UPDATE users SET banned_at = now() WHERE id = $1",
    ] {
        let error = sqlx::query(sql)
            .bind(user)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA004"), "{sql}");
        assert_eq!(server::db::error_kind(&error), "privileged_write", "{sql}");
    }
    // A write that does not touch them passes, also naming them with their current values.
    sqlx::query("UPDATE users SET display_name = 'Otro Nombre' WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET role = 'user', banned_at = NULL WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    // An insert with a privileged value is refused; the defaults pass.
    for sql in [
        "INSERT INTO users (display_name, display_name_key, role) VALUES ('Abc', 'abc', 'admin')",
        "INSERT INTO users (display_name, display_name_key, banned_at) VALUES ('Abd', 'abd', now())",
    ] {
        let error = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA004"), "{sql}");
    }
    // Through the helper (the CLI's `SET LOCAL`), both pass.
    privileged(
        &pool,
        sqlx::query("UPDATE users SET role = 'admin', banned_at = now() WHERE id = $1").bind(user),
    )
    .await;
    privileged(
        &pool,
        sqlx::query(
            "INSERT INTO users (display_name, display_name_key, role) VALUES ('Abe', 'abe', 'admin')",
        ),
    )
    .await;
    let row = sqlx::query("SELECT role, banned_at IS NOT NULL FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "admin");
    assert!(row.get::<bool, _>(1));
}

/// The actor of the helper dies with its transaction: on the same (single) connection, the
/// next write without it is refused again.
#[tokio::test]
async fn the_cli_actor_never_leaks_to_the_next_statement_of_the_connection() {
    let db = TestDb::create().await;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(db.options())
        .await
        .unwrap();
    server::db::migrate(&pool).await.unwrap();
    let user = plain_user(&pool).await;
    privileged(
        &pool,
        sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1").bind(user),
    )
    .await;
    let error = sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("TA004"));
}

/// `torneo.actor` appears in `tests/` only in the helper and in this file, never with a
/// session-wide `SET` or a non-local `set_config`. The needles are built in pieces so this file
/// does not match its own search.
#[test]
fn the_cli_actor_is_set_only_locally_and_only_by_the_helper() {
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let actor = ["torneo", ".actor"].concat();
    let session_set = ["set ", "torneo", ".actor"].concat();
    let set_config = ["set_", "config("].concat();
    let allowed = [
        tests.join("support").join("db.rs"),
        tests.join("admin_audit.rs"),
    ];
    let mut stack = vec![tests.clone()];
    let mut helper_hits = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap().to_ascii_lowercase();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.contains(&actor) {
                assert!(
                    allowed.contains(&path),
                    "{} names the actor",
                    path.display()
                );
            }
            assert!(
                !text.contains(&session_set),
                "{} sets the actor for the session",
                path.display()
            );
            for (i, _) in text.match_indices(&set_config) {
                let call: String = text[i..].chars().take_while(|c| *c != ')').collect();
                assert!(
                    call.ends_with("true"),
                    "{} calls {set_config} without is_local = true",
                    path.display()
                );
                if path == allowed[0] {
                    helper_hits += 1;
                }
            }
        }
    }
    assert_eq!(helper_hits, 1, "the helper sets the actor exactly once");
}
