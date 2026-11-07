//! 012 RF23: the public counter, with no session: it goes up with every void, `voided` includes
//! `secret_lost`, and every answer says `no-store`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;
use std::sync::Arc;

use protocol::VoidStats;
use server::admin::{VoidCommand, run_fight_void};
use server::app::Settings;
use server::bets::BetsHook;
use server::fights::sealed::RealCompute;
use server::rate::{RateSettings, ServerClock};
use support::bets;
use support::db::{TestDb, privileged};
use support::fights::{self as f, KO, T0, Vector};
use support::http;

async fn stats(addr: SocketAddr) -> VoidStats {
    let response = http::send(addr, "GET", "/api/fights/void-stats", &[]).await;
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(response.header("cache-control"), Some("no-store"));
    serde_json::from_slice(&response.body).unwrap()
}

#[tokio::test]
async fn it_goes_up_with_every_void() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    assert_eq!(
        stats(app.addr).await,
        VoidStats {
            voided: 0,
            secret_lost: 0
        }
    );
    let admin = bets::user(&pool).await;
    privileged(
        &pool,
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
    )
    .await;
    let vector = Vector::of(KO);
    let void = |id: i32, backup_checked: bool| {
        let (options, compute) = (db.options(), Arc::new(RealCompute));
        async move {
            run_fight_void(
                &options,
                &VoidCommand {
                    fight: id,
                    by: admin,
                    previous_binary: None,
                    backup_checked,
                },
                compute,
                &BetsHook,
            )
            .await
            .unwrap();
        }
    };

    // A bad snapshot counts in `voided` only.
    let bad = f::announce_with(&pool, T0, &vector.a, &vector.b, |raw| {
        raw.snapshot_sha256 = vec![0; 32];
    })
    .await;
    f::open_with(&pool, bad, &vector.seed, T0).await;
    f::close_by_sql(&pool, bad, T0 + f::WINDOW).await;
    void(bad, false).await;
    assert_eq!(
        stats(app.addr).await,
        VoidStats {
            voided: 1,
            secret_lost: 0
        }
    );

    // A lost secret counts in both.
    let now: i64 = sqlx::query_scalar("SELECT (extract(epoch FROM now()) * 1000)::bigint")
        .fetch_one(&pool)
        .await
        .unwrap();
    let opens = now - 100 * f::CADENCE;
    let lost = f::announce(&pool, opens, &vector.a, &vector.b).await;
    f::open_with(&pool, lost, &vector.seed, opens).await;
    f::close_by_sql(&pool, lost, opens + f::WINDOW).await;
    f::lose_secret(&pool, lost).await;
    void(lost, true).await;
    assert_eq!(
        stats(app.addr).await,
        VoidStats {
            voided: 2,
            secret_lost: 1
        }
    );

    // A fight that is not voided counts nothing.
    let open = f::open_vector(&pool, T0 + f::CADENCE, &vector).await;
    assert!(open > 0);
    assert_eq!(stats(app.addr).await.voided, 2);
}

#[tokio::test]
async fn a_failure_of_the_database_is_a_503_with_no_store() {
    let app = support::app::start(support::app::state(&[], &Settings::default())).await;
    let response = http::send(app.addr, "GET", "/api/fights/void-stats", &[]).await;
    assert_eq!(response.status, 503, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"fight.unavailable"}"#);
    assert_eq!(response.header("cache-control"), Some("no-store"));
}

/// The limit of RF34, in the bucket of the detail: the same 30 of burst for both, and its 429 says
/// `no-store` too.
#[tokio::test]
async fn it_shares_the_quota_of_the_detail() {
    let settings = Settings {
        rate: RateSettings {
            clock: ServerClock::manual(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    let db = TestDb::create().await;
    let (app, _pool, _shared) = f::start_app(&db, &[], &settings).await;
    for _ in 0..15 {
        assert_eq!(
            http::send(app.addr, "GET", "/api/fights/1", &[])
                .await
                .status,
            404
        );
        stats(app.addr).await;
    }
    let limited = http::send(app.addr, "GET", "/api/fights/void-stats", &[]).await;
    assert_eq!(limited.status, 429);
    assert_eq!(limited.text(), r#"{"error":"http.rate_limited"}"#);
    assert_eq!(limited.header("cache-control"), Some("no-store"));
}
