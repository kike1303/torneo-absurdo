//! 012 and ADR-0012 A4: with `RUST_LOG=trace`, a whole round of bets (claims, bets, reads, the pool,
//! the quote and a paid settlement) logs no user, bet or key id, no session token, and no field
//! outside the allowlist, so no amount either.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use server::app::{AppState, Settings};
use server::bets::BetsHook;
use server::fights::FightSettings;
use server::telemetry::ALLOWED_FIELDS;
use support::app::ORIGIN;
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector, wait_for};
use support::{http, logs};
use uuid::Uuid;

#[test]
fn a_round_of_bets_logs_no_id_and_no_amount() {
    let mut canaries: Vec<String> = Vec::new();
    let captured = logs::run("trace", |_captured| async {
        let db = TestDb::create().await;
        let url = db.url();
        let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
        let pool = db.migrated_pool().await;
        let (fights, clock) = f::settings_at(T0);
        let settings = Settings {
            fights: FightSettings {
                hook: Arc::new(BetsHook),
                ..fights
            },
            ..Settings::default()
        };
        let state = AppState::from_config(
            &config,
            &settings,
            pool.clone(),
            Arc::new(support::app::AlwaysUp),
        )
        .unwrap();
        let id = f::open_vector_with_rake(&pool, T0, &Vector::of(KO), 500).await;
        let _runner = f::spawn_runner(
            &pool,
            &state.fights,
            f::config_without_roster(),
            settings.fights.clone(),
        );
        let shared = state.fights.clone();
        wait_for("the window to take bets", || {
            let shared = shared.clone();
            async move { shared.accepts_bets() }
        })
        .await;
        let app = support::app::start(state.clone()).await;
        // The quote reads the cache that the task fills.
        server::bets::pool::refresh(&state.db, &state.pools, &state.fights).await;
        for side in ["a", "b"] {
            let user = support::bets::funded_user(&pool, 24).await;
            let token = support::app::session_for(&pool, user).await;
            let cookie = support::oauth::session_cookie(&token);
            let key = Uuid::new_v4();
            canaries.extend([user.to_string(), token.clone(), key.to_string()]);
            let post = |path: &'static str, body: String| {
                let cookie = cookie.clone();
                async move {
                    http::send_body(
                        app.addr,
                        "POST",
                        path,
                        &[
                            ("origin", ORIGIN),
                            ("cookie", cookie.as_str()),
                            ("content-type", "application/json"),
                        ],
                        &body,
                    )
                    .await
                }
            };
            assert_eq!(post("/api/bonus/claim", String::new()).await.status, 409);
            assert_eq!(post("/api/bailout/claim", String::new()).await.status, 200);
            let bet = post(
                "/api/bets",
                format!(r#"{{"fight_id":{id},"side":"{side}","amount":50,"client_key":"{key}"}}"#),
            )
            .await;
            assert_eq!(bet.status, 201, "{}", bet.text());
            // A refused one too.
            assert_eq!(
                post(
                    "/api/bets",
                    format!(
                        r#"{{"fight_id":{id},"side":"{side}","amount":1,"client_key":"{key}"}}"#
                    ),
                )
                .await
                .status,
                409
            );
            for path in [
                "/api/wallet".to_owned(),
                format!("/api/bets/mine?fight_id={id}"),
                "/api/bets/history".to_owned(),
                format!("/api/fights/{id}/quote?side={side}&amount=25"),
            ] {
                let response =
                    http::send(app.addr, "GET", &path, &[("cookie", cookie.as_str())]).await;
                assert_eq!(response.status, 200, "{path}: {}", response.text());
            }
        }
        server::bets::pool::refresh(&state.db, &state.pools, &state.fights).await;
        assert_eq!(
            http::get(app.addr, &format!("/api/fights/{id}/pool"))
                .await
                .status,
            200
        );
        let bets: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM bets")
            .fetch_all(&pool)
            .await
            .unwrap();
        canaries.extend(bets.iter().map(Uuid::to_string));
        f::drive_to_settled(&pool, &clock, id).await;
        let paid: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pool_settlements WHERE outcome = 'paid'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(paid, 1);
    });
    let text = captured.text();
    assert!(canaries.len() >= 8, "{canaries:?}");
    for canary in &canaries {
        assert!(!text.contains(canary.as_str()), "the logs have {canary}");
    }
    // A positive control: the run did log at trace.
    assert!(!captured.events("fights.settled").is_empty(), "{text}");
    for line in captured.lines() {
        for (field, _) in line.as_object().unwrap() {
            let known = ALLOWED_FIELDS.contains(&field.as_str())
                || matches!(field.as_str(), "ts" | "level" | "target");
            assert!(known, "{field} in {line}");
        }
    }
    for field in ["amount", "stake", "payout", "balance", "user_id", "bet_id"] {
        assert!(!ALLOWED_FIELDS.contains(&field), "{field}");
    }
}
