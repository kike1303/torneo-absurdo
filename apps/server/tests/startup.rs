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
    let rows = sqlx::query("SELECT version, success FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool)
        .await
        .unwrap();
    let versions: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("version")).collect();
    assert_eq!(versions, [20_261_023_000_000, 20_261_028_000_000]);
    assert!(rows.iter().all(|r| r.get::<bool, _>("success")));
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

async fn run_to_exit(envs: &[(&str, &str)]) -> bin::Finished {
    bin::spawn(&[], envs).finish(Duration::from_secs(10)).await
}

fn single_json_line(lines: &[String]) -> serde_json::Value {
    assert_eq!(lines.len(), 1, "{lines:?}");
    serde_json::from_str(&lines[0]).unwrap()
}

#[tokio::test]
async fn missing_public_origin_is_config_invalid() {
    let finished = run_to_exit(&[("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x")]).await;
    assert_eq!(finished.status.code(), Some(1));
    let line = single_json_line(&finished.stdout);
    assert_eq!(line["event"], "config.invalid");
    assert_eq!(line["var"], "PUBLIC_ORIGIN");
    assert!(finished.stderr.is_empty());
}

#[tokio::test]
async fn an_invalid_value_never_reaches_the_output() {
    let finished = run_to_exit(&[
        ("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x"),
        ("PUBLIC_ORIGIN", "https://x.example/ruta-canario"),
    ])
    .await;
    assert_eq!(finished.status.code(), Some(1));
    let line = single_json_line(&finished.stdout);
    assert_eq!(line["var"], "PUBLIC_ORIGIN");
    for text in finished.stdout.iter().chain(&finished.stderr) {
        assert!(!text.contains("canario"), "{text}");
    }
    assert!(finished.stderr.is_empty());
}

#[tokio::test]
async fn bind_addr_must_be_an_ip() {
    let finished = run_to_exit(&[
        ("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x"),
        ("PUBLIC_ORIGIN", "http://localhost:5173"),
        ("BIND_ADDR", "no-es-ip"),
    ])
    .await;
    assert_eq!(finished.status.code(), Some(1));
    let line = single_json_line(&finished.stdout);
    assert_eq!(line["event"], "config.invalid");
    assert_eq!(line["var"], "BIND_ADDR");
}

// With `dev-auth` a non-loopback BIND_ADDR never reaches the bind (see the gate test below).
#[cfg(not(feature = "dev-auth"))]
#[tokio::test]
async fn a_failed_bind_exits_with_one_without_the_address() {
    let db = TestDb::create().await;
    let url = db.url();
    let finished = run_to_exit(&[
        ("DATABASE_URL", &url),
        ("PUBLIC_ORIGIN", "http://localhost:5173"),
        ("BIND_ADDR", "192.0.2.1"),
        ("PORT", "8080"),
    ])
    .await;
    assert_eq!(finished.status.code(), Some(1));
    let line = finished
        .stdout
        .iter()
        .find(|l| l.contains("server.bind_failed"))
        .unwrap_or_else(|| panic!("{:?}", finished.stdout));
    let value: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(value["error_kind"], "addr_not_available");
    for text in finished.stdout.iter().chain(&finished.stderr) {
        assert!(
            !text.contains("192.0.2.1") && !text.contains("8080"),
            "{text}"
        );
    }
}

#[tokio::test]
async fn an_unknown_subcommand_exits_with_two() {
    let finished = bin::spawn(&["migrate"], &[])
        .finish(Duration::from_secs(10))
        .await;
    assert_eq!(finished.status.code(), Some(2));
    let line = single_json_line(&finished.stdout);
    assert_eq!(line["event"], "cli.invalid");
    assert!(finished.stderr.is_empty());
}

/// 008 RF-A3: an invalid `TRUSTED_PROXIES`, or a loopback proxy with a public origin, stops
/// the start with only the variable name in the log.
#[tokio::test]
async fn an_invalid_trusted_proxies_is_config_invalid_without_the_value() {
    for (proxies, origin) in [
        ("10.250.0.0/28", "https://torneo.example"),
        ("198.51.100.77,,198.51.100.78", "https://torneo.example"),
        ("127.0.0.1/32", "https://torneo.example"),
        ("127.0.0.1", "http://torneo.example"),
    ] {
        let finished = run_to_exit(&[
            ("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x"),
            ("PUBLIC_ORIGIN", origin),
            ("TRUSTED_PROXIES", proxies),
        ])
        .await;
        assert_eq!(finished.status.code(), Some(1), "{proxies}");
        let line = single_json_line(&finished.stdout);
        assert_eq!(line["event"], "config.invalid");
        assert_eq!(line["var"], "TRUSTED_PROXIES");
        for text in finished.stdout.iter().chain(&finished.stderr) {
            assert!(
                !text.contains("10.250") && !text.contains("198.51") && !text.contains("127.0"),
                "{text}"
            );
        }
        assert!(finished.stderr.is_empty());
    }
}

/// 008 RF-A6: an `INTERNAL_API_SECRET` shorter than 32 bytes stops the start.
#[tokio::test]
async fn a_short_internal_api_secret_is_config_invalid_without_the_value() {
    let finished = run_to_exit(&[
        ("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x"),
        ("PUBLIC_ORIGIN", "http://localhost:5173"),
        ("INTERNAL_API_SECRET", "c2hvcnQtY2FuYXJ5"),
    ])
    .await;
    assert_eq!(finished.status.code(), Some(1));
    let line = single_json_line(&finished.stdout);
    assert_eq!(line["var"], "INTERNAL_API_SECRET");
    for text in finished.stdout.iter().chain(&finished.stderr) {
        assert!(!text.contains("c2hvcnQtY2FuYXJ5"), "{text}");
    }
}

/// 008 RF-B18: the new required variables stop the start with only their name.
#[tokio::test]
async fn phase_b_variables_are_config_invalid_without_the_value() {
    let dev_secret = server::config::DEV_INTERNAL_API_SECRET;
    for (vars, var) in [
        (vec![("DAILY_GRANT", "10001")], "DAILY_GRANT"),
        (vec![("DAILY_GRANT", "0")], "DAILY_GRANT"),
        (vec![("IP_HASH_KEY", "c2hvcnQtY2FuYXJ5")], "IP_HASH_KEY"),
        // The public development secret outside a loopback http origin.
        (
            vec![
                ("PUBLIC_ORIGIN", "https://torneo.example"),
                ("INTERNAL_API_SECRET", dev_secret),
            ],
            "INTERNAL_API_SECRET",
        ),
    ] {
        let mut envs = vec![
            ("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x"),
            ("PUBLIC_ORIGIN", "http://localhost:5173"),
        ];
        envs.extend(vars.iter().copied());
        let finished = run_to_exit(&envs).await;
        assert_eq!(finished.status.code(), Some(1), "{vars:?}");
        let line = single_json_line(&finished.stdout);
        assert_eq!(line["event"], "config.invalid");
        assert_eq!(line["var"], var, "{vars:?}");
        for text in finished.stdout.iter().chain(&finished.stderr) {
            assert!(!text.contains("c2hvcnQtY2FuYXJ5") && !text.contains(dev_secret));
        }
    }
}

/// 008 RF-B6: with `dev-auth`, a public origin or a public BIND_ADDR stop the start.
#[cfg(feature = "dev-auth")]
#[tokio::test]
async fn dev_auth_needs_a_loopback_origin_and_bind_addr() {
    for (vars, var) in [
        (
            vec![
                ("PUBLIC_ORIGIN", "https://torneo.example"),
                ("BIND_ADDR", "127.0.0.1"),
            ],
            "DEV_AUTH",
        ),
        (
            vec![
                ("PUBLIC_ORIGIN", "http://localhost:5173"),
                ("BIND_ADDR", "0.0.0.0"),
            ],
            "BIND_ADDR",
        ),
    ] {
        let mut envs = vec![("DATABASE_URL", "postgres://u:p@127.0.0.1:1/x")];
        envs.extend(vars.iter().copied());
        let finished = run_to_exit(&envs).await;
        assert_eq!(finished.status.code(), Some(1), "{vars:?}");
        let line = single_json_line(&finished.stdout);
        assert_eq!(line["var"], var, "{vars:?}");
    }
}
