//! RF-B10, RF-B16: `/api/health` with a cut-able database, a cache and one query in flight.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use server::app::{AppState, Settings};
use server::db::{self, DbProbe, PgProbe, PingFuture, RetryConfig};
use server::http::health::HealthConfig;
use support::db::TestDb;
use support::http;
use support::pg_proxy::PgProxy;

/// Wraps the real probe and counts the queries.
struct Counting {
    inner: PgProbe,
    calls: AtomicUsize,
}

impl DbProbe for Counting {
    fn ping(&self) -> PingFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.ping()
    }
}

struct Slow;

impl DbProbe for Slow {
    fn ping(&self) -> PingFuture<'_> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            true
        })
    }
}

fn settings(cache_ttl: Duration) -> Settings {
    Settings {
        health: HealthConfig {
            cache_ttl,
            db_timeout: Duration::from_secs(1),
        },
        ..Settings::default()
    }
}

async fn counting_probe(db: &TestDb, proxy: &PgProxy) -> Arc<Counting> {
    let pool = db::connect_with_retry(db.options_via(proxy.addr()), RetryConfig::default())
        .await
        .unwrap();
    Arc::new(Counting {
        inner: PgProbe::new(pool),
        calls: AtomicUsize::new(0),
    })
}

#[tokio::test]
async fn health_follows_the_database_without_restarting() {
    let db = TestDb::create().await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let probe = counting_probe(&db, &proxy).await;
    let app = support::app::start(AppState::new(
        probe.clone(),
        &settings(Duration::from_millis(100)),
    ))
    .await;

    let ok = http::get(app.addr, "/api/health").await;
    assert_eq!(ok.status, 200);
    assert_eq!(ok.text(), r#"{"status":"ok","version":"dev"}"#);
    assert_eq!(ok.header("cache-control"), Some("no-store"));
    assert_eq!(ok.header("content-type"), Some("application/json"));

    proxy.cut();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let down = http::get(app.addr, "/api/health").await;
    assert_eq!(down.status, 503);
    assert_eq!(down.text(), r#"{"status":"db_unavailable"}"#);
    assert_eq!(down.header("cache-control"), Some("no-store"));

    proxy.restore();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let back = http::get(app.addr, "/api/health").await;
    assert_eq!(back.status, 200);
    assert_eq!(back.text(), r#"{"status":"ok","version":"dev"}"#);
}

#[tokio::test]
async fn fifty_concurrent_requests_make_one_query() {
    let db = TestDb::create().await;
    let proxy = PgProxy::start(db.server_addr()).await;
    let probe = counting_probe(&db, &proxy).await;
    // A long TTL so every request of the burst lands inside it; the cache starts empty.
    let app = support::app::start(AppState::new(
        probe.clone(),
        &settings(Duration::from_secs(5)),
    ))
    .await;

    let requests = (0..50).map(|_| http::get(app.addr, "/api/health"));
    let responses = futures_util::future::join_all(requests).await;
    assert!(responses.iter().all(|r| r.status == 200));
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_slow_database_answers_503_at_the_timeout() {
    let app = support::app::start(AppState::new(
        Arc::new(Slow),
        &settings(Duration::from_millis(100)),
    ))
    .await;
    let started = Instant::now();
    let response = http::get(app.addr, "/api/health").await;
    assert_eq!(response.status, 503);
    assert!(
        started.elapsed() <= Duration::from_millis(1200),
        "{:?}",
        started.elapsed()
    );
}
