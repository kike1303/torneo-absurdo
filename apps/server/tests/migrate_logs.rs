//! TA-47: `db.migrate_failed` says why with `error_kind`, one code per cause, against a real
//! Postgres and the server's real log layer.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::app::StartupError;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use support::db::TestDb;
use support::logs;

async fn pool_on(db: &TestDb) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(db.options())
        .await
        .unwrap()
}

async fn assert_stops(pool: &PgPool) {
    assert_eq!(
        server::db::migrate(pool).await.err(),
        Some(StartupError::Migrate)
    );
}

#[test]
fn every_migrate_failure_logs_its_kind() {
    let captured = logs::run("info", |_captured| async {
        // A version this image knows, applied with other content.
        let db = TestDb::create().await;
        let pool = db.migrated_pool().await;
        sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = '\\x00'::bytea \
             WHERE version = (SELECT min(version) FROM _sqlx_migrations)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_stops(&pool).await;
        pool.close().await;

        // A row marked as failed.
        let db = TestDb::create().await;
        let pool = db.migrated_pool().await;
        sqlx::query(
            "UPDATE _sqlx_migrations SET success = false \
             WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_stops(&pool).await;
        pool.close().await;

        // The SQL of a migration fails: the table it creates is already there.
        let db = TestDb::create().await;
        let pool = pool_on(&db).await;
        sqlx::query("CREATE TABLE users (id BIGINT)")
            .execute(&pool)
            .await
            .unwrap();
        assert_stops(&pool).await;
        pool.close().await;

        // The database is gone before the migrations start.
        let db = TestDb::create().await;
        let pool = pool_on(&db).await;
        pool.close().await;
        assert_stops(&pool).await;
    });
    let kinds: Vec<_> = captured
        .events("db.migrate_failed")
        .into_iter()
        .map(|line| {
            assert_eq!(line["level"], "error", "{line}");
            line["error_kind"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "checksum_mismatch",
            "migration_dirty",
            "migration_failed",
            "pool"
        ]
    );
}
