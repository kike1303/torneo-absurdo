//! RF-B9: database wait, migrations before the port, pool size.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::process::ExitCode;
use std::str::FromStr;
use std::time::{Duration, Instant};

use server::app::StartupError;
use server::db::{self, RetryConfig};
use sqlx::Row;
use sqlx::postgres::PgConnectOptions;
use support::bin;
use support::db::TestDb;
use support::pg_proxy::PgProxy;

#[tokio::test]
async fn connect_with_retry_gives_up_at_the_deadline() {
    let port = support::free_port();
    let options =
        PgConnectOptions::from_str(&format!("postgres://torneo:x@127.0.0.1:{port}/torneo"))
            .unwrap();
    let started = Instant::now();
    let result = db::connect_with_retry(
        options,
        RetryConfig {
            interval: Duration::from_millis(50),
            deadline: Duration::from_millis(300),
        },
    )
    .await;
    let elapsed = started.elapsed();
    assert_eq!(result.err(), Some(StartupError::DbUnavailable));
    assert!(
        elapsed >= Duration::from_millis(300) && elapsed <= Duration::from_millis(1000),
        "{elapsed:?}"
    );
    assert_eq!(
        server::exit_code(&server::CliError::Startup(StartupError::DbUnavailable)),
        ExitCode::FAILURE
    );
}

#[tokio::test]
async fn the_port_opens_only_after_the_database_and_the_migrations() {
    let db = TestDb::create().await;
    let proxy = PgProxy::start_cut(db.server_addr()).await;
    let port = support::free_port();
    let port_text = port.to_string();
    let url = db.url_via(proxy.addr());
    let process = bin::spawn(
        &[],
        &[
            ("DATABASE_URL", &url),
            ("PUBLIC_ORIGIN", "http://localhost:5173"),
            ("PORT", &port_text),
            ("BIND_ADDR", "127.0.0.1"),
        ],
    );

    assert!(
        process
            .wait_for_stdout("\"server.listening\"", Duration::from_secs(3))
            .await
            .is_none()
    );
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err(),
        "the port must stay closed while the database is down"
    );
    assert!(
        process
            .stdout()
            .iter()
            .any(|l| l.contains("\"db.unavailable\""))
    );

    proxy.restore();
    let logged_port = process.wait_listening(Duration::from_secs(10)).await;
    assert_eq!(logged_port, port);

    let pool = sqlx::PgPool::connect_with(db.options()).await.unwrap();
    let rows = sqlx::query("SELECT version, success FROM _sqlx_migrations")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i64, _>("version"), 20_261_023_000_000);
    assert!(rows[0].get::<bool, _>("success"));
    pool.close().await;
    process.kill().await;
}

#[tokio::test]
async fn the_pool_has_eight_connections() {
    let db = TestDb::create().await;
    let pool = db::connect_with_retry(db.options(), RetryConfig::default())
        .await
        .unwrap();
    assert_eq!(pool.options().get_max_connections(), 8);
    pool.close().await;
}
