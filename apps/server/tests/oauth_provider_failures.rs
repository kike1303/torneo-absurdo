//! 008 RF-C1 and T19: every way the fake provider can fail is `auth.provider_error`, without a
//! user, and with `auth.provider_failed` and a code in the log. In its own binary with a single
//! test: the in-process log capture (`support::logs`) needs no other test running beside it
//! (plan-c deviation 7).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use sqlx::{PgPool, Row};
use support::db::TestDb;
use support::fake_provider::{Kind, Mode};
use support::oauth;

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

fn identity(kind: Kind) -> String {
    match kind {
        Kind::Google => format!("g-sub-{}", uuid::Uuid::new_v4().simple()),
        Kind::Discord => "100000000000000042".to_owned(),
    }
}

/// Each way the provider can fail is `auth.provider_error`, without a user, and with
/// `auth.provider_failed` and a code in the log.
#[test]
fn every_failure_of_the_provider_is_provider_error() {
    let modes = [
        (Mode::TokenStatus500, Kind::Google),
        (Mode::UserinfoStatus500, Kind::Discord),
        (Mode::InvalidJson, Kind::Google),
        (Mode::NoIdentity, Kind::Discord),
        (Mode::ControlIdentity, Kind::Google),
        (Mode::TooLarge, Kind::Discord),
        (Mode::Silent, Kind::Google),
    ];
    let logs = support::logs::run("info", |_| async move {
        let db = TestDb::create().await;
        let (app, pool) = oauth::start_app(&db).await;
        for (mode, kind) in modes {
            app.fake(kind).set_mode(mode);
            let done = oauth::login(&app, kind, &identity(kind)).await;
            assert_eq!(oauth::error_of(&done), "auth.provider_error", "{mode:?}");
            assert_eq!(done.cookie(oauth::flow_name()), Some(""), "{mode:?}");
            assert_eq!(count(&pool, "users").await, 0, "{mode:?}");
            app.fake(kind).set_mode(Mode::Ok);
        }
        // A Discord id that is not a snowflake is refused too.
        let done = oauth::login(&app, Kind::Discord, "not-a-snowflake").await;
        assert_eq!(oauth::error_of(&done), "auth.provider_error");
        let done = oauth::login(&app, Kind::Discord, "123456789012345678901").await;
        assert_eq!(oauth::error_of(&done), "auth.provider_error");
        assert_eq!(count(&pool, "users").await, 0);
    });
    let failed = logs.events("auth.provider_failed");
    assert_eq!(failed.len(), 9, "{}", logs.text());
    let kinds: Vec<&str> = failed
        .iter()
        .map(|l| l["error_kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "token_status",
            "userinfo_status",
            "userinfo_invalid",
            "userinfo_invalid",
            "userinfo_invalid",
            "too_large",
            "timeout",
            "userinfo_invalid",
            "userinfo_invalid",
        ]
    );
    for line in &failed {
        assert_eq!(line["level"], "warn");
        let mut keys: Vec<_> = line.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["error_kind", "event", "level", "ts"], "{line}");
    }
}
