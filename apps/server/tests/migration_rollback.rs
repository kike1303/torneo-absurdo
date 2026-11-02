//! TA-43 (ADR-0006 D6 and D10): going back to the previous image after a migration. That image
//! finds a version it does not know in `_sqlx_migrations` and must start, while another checksum
//! in a version it does know still stops it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use server::app::StartupError;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use support::bin;
use support::db::TestDb;
use support::http;

/// Above every real migration, like the one a newer image brings.
const NEWER: i64 = 99_990_101_000_000;

/// A directory with the migrations of this image plus an additive one of a newer image.
struct NewerImage(PathBuf);

impl NewerImage {
    fn create() -> Self {
        let dir = std::env::temp_dir().join(format!("torneo-newer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let own = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for entry in std::fs::read_dir(own).unwrap() {
            let path = entry.unwrap().path();
            std::fs::copy(&path, dir.join(path.file_name().unwrap())).unwrap();
        }
        std::fs::write(
            dir.join(format!("{NEWER}_newer_image.sql")),
            "ALTER TABLE users ADD COLUMN newer_image_column TEXT;",
        )
        .unwrap();
        Self(dir)
    }

    async fn migrate(&self, pool: &PgPool) {
        Migrator::new(self.0.clone())
            .await
            .unwrap()
            .run(pool)
            .await
            .unwrap();
    }
}

impl Drop for NewerImage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn pool_on(db: &TestDb) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(db.options())
        .await
        .unwrap()
}

async fn applied(pool: &PgPool, version: i64) -> i64 {
    sqlx::query("SELECT count(*) FROM _sqlx_migrations WHERE version = $1")
        .bind(version)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// The real binary, as after a rollback: it opens the port, says that the database is ahead of
/// it, and the newer image can come back later over the same database.
#[tokio::test(flavor = "multi_thread")]
async fn the_previous_image_starts_after_a_newer_migration() {
    let db = TestDb::create().await;
    let pool = pool_on(&db).await;
    let newer = NewerImage::create();
    newer.migrate(&pool).await;

    let port = support::free_port();
    let port_text = port.to_string();
    let url = db.url();
    let process = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", "http://localhost:5173"),
            ("PORT", &port_text),
            ("BIND_ADDR", "127.0.0.1"),
        ],
    );
    assert_eq!(process.wait_listening(Duration::from_secs(20)).await, port);
    let line = process
        .stdout()
        .into_iter()
        .find(|l| l.contains("\"db.migrations_ahead\""))
        .expect("the start must say that the database is ahead");
    let log: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(log["level"], "warn", "{line}");
    // What Coolify's healthcheck and `verify` look at, not only the port.
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    assert_eq!(http::get(addr, "/api/health").await.status, 200);
    process.kill().await;

    newer.migrate(&pool).await;
    assert_eq!(applied(&pool, NEWER).await, 1);
    pool.close().await;
}

/// Without unknown versions there is nothing to warn about.
#[tokio::test(flavor = "multi_thread")]
async fn the_normal_start_says_nothing_about_migrations() {
    let db = TestDb::create().await;
    let port = support::free_port();
    let port_text = port.to_string();
    let url = db.url();
    let process = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", "http://localhost:5173"),
            ("PORT", &port_text),
            ("BIND_ADDR", "127.0.0.1"),
        ],
    );
    assert_eq!(process.wait_listening(Duration::from_secs(20)).await, port);
    assert!(
        !process
            .stdout()
            .iter()
            .any(|l| l.contains("\"db.migrations_ahead\""))
    );
    process.kill().await;
}

/// A row marked as failed is not a rollback to tolerate. sqlx 0.9 never writes one itself, so the
/// test marks it by hand: this pins what the start does with it, not a failure it can detect
/// (ADR-0006 D10, "Migración a medias").
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_newer_migration_still_stops_the_start() {
    let db = TestDb::create().await;
    let pool = pool_on(&db).await;
    NewerImage::create().migrate(&pool).await;
    sqlx::query("UPDATE _sqlx_migrations SET success = false WHERE version = $1")
        .bind(NEWER)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        server::db::migrate(&pool).await.err(),
        Some(StartupError::Migrate)
    );
    pool.close().await;
}

/// A migration this image knows, with other content in the database, is not a rollback either.
#[tokio::test(flavor = "multi_thread")]
async fn a_known_migration_with_another_checksum_still_stops_the_start() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = '\\x00'::bytea \
         WHERE version = (SELECT min(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        server::db::migrate(&pool).await.err(),
        Some(StartupError::Migrate)
    );
    pool.close().await;
}
