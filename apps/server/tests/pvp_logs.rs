//! 021 phase 2 and ADR-0012 A8: with `RUST_LOG=trace`, attacks (against a user, against the
//! tournament, a retry, a refused one and one whose defender is skipped) and the three reads log no
//! seed, no spec, no log of a fight, no client key, no user id, no name and no session token: only
//! `event` and `error_kind`, and the fields of the request log.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use server::app::Settings;
use server::rate::{RateSettings, ServerClock};
use server::telemetry::ALLOWED_FIELDS;
use sqlx::Row;
use support::app::ORIGIN;
use support::db::TestDb;
use support::{http, logs};
use uuid::Uuid;

#[test]
fn the_player_fights_log_no_seed_no_key_and_no_user() {
    let mut canaries: Vec<String> = Vec::new();
    let captured = logs::run("trace", |_captured| async {
        let db = TestDb::create().await;
        let clock = ServerClock::manual();
        let settings = Settings {
            rate: RateSettings {
                clock: clock.clone(),
                ..RateSettings::default()
            },
            ..Settings::default()
        };
        let content = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/characters");
        let (app, pool) = support::app::start_db(&db, &[("CONTENT_DIR", content)], &settings).await;
        let catalog = server::botarga::BotargaCatalog::embedded().unwrap();
        let mut players = Vec::new();
        for _ in 0..3 {
            let (user, token) = support::app::user_with_session(&pool).await;
            let mut tx = pool.begin().await.unwrap();
            server::botarga::store::create(&mut tx, user, catalog.starter())
                .await
                .unwrap();
            tx.commit().await.unwrap();
            canaries.extend([user.to_string(), user.simple().to_string(), token.clone()]);
            players.push((user, token));
        }
        // The third one has a build the catalog no longer has: it is skipped as a defender.
        sqlx::query("UPDATE botargas SET abilities = ARRAY['retired_move'] WHERE user_id = $1")
            .bind(players[2].0)
            .execute(&pool)
            .await
            .unwrap();
        let send = |method: &'static str, path: String, token: String, body: String| {
            clock.advance(Duration::from_secs(60));
            async move {
                let cookie = support::oauth::session_cookie(&token);
                http::send_body(
                    app.addr,
                    method,
                    &path,
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
        let token = players[0].1.clone();
        for _ in 0..3 {
            let key = Uuid::new_v4().to_string();
            canaries.push(key.clone());
            let body = format!(r#"{{"client_key":"{key}"}}"#);
            let created = send(
                "POST",
                "/api/pvp/fights".to_owned(),
                token.clone(),
                body.clone(),
            )
            .await;
            assert_eq!(created.status, 201, "{}", created.text());
            let again = send("POST", "/api/pvp/fights".to_owned(), token.clone(), body).await;
            assert_eq!(again.status, 200, "{}", again.text());
        }
        // Refused: not a key.
        let refused = send(
            "POST",
            "/api/pvp/fights".to_owned(),
            token.clone(),
            r#"{"client_key":"not-a-key"}"#.to_owned(),
        )
        .await;
        assert_eq!(refused.status, 422);
        // The third player attacks with a build that no longer resolves.
        let broken = send(
            "POST",
            "/api/pvp/fights".to_owned(),
            players[2].1.clone(),
            format!(r#"{{"client_key":"{}"}}"#, Uuid::new_v4()),
        )
        .await;
        assert_eq!(broken.status, 409);
        let rows = sqlx::query("SELECT id, seed, spec_a, spec_b, log FROM pvp_fights")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        for row in &rows {
            let id: i32 = row.get(0);
            let seed: Vec<u8> = row.get(1);
            canaries.push(server::fights::hex(&seed));
            for at in 2..=4 {
                canaries.push(String::from_utf8(row.get(at)).unwrap());
            }
            let detail = http::get(app.addr, &format!("/api/pvp/fights/{id}")).await;
            assert_eq!(detail.status, 200);
        }
        let list = send(
            "GET",
            "/api/pvp/fights".to_owned(),
            token.clone(),
            String::new(),
        )
        .await;
        assert_eq!(list.status, 200);
        let botarga = send("GET", "/api/botarga".to_owned(), token, String::new()).await;
        assert_eq!(botarga.status, 200);
        let names: Vec<String> = sqlx::query_scalar("SELECT display_name FROM users")
            .fetch_all(&pool)
            .await
            .unwrap();
        canaries.extend(names);
    });
    let text = captured.text();
    assert!(canaries.len() >= 20, "{canaries:?}");
    for canary in &canaries {
        assert!(!text.contains(canary.as_str()), "the logs have {canary}");
    }
    for fragment in [r#""t":"#, "client_key", "daily_index", "coins_attacker"] {
        assert!(!text.contains(fragment), "the logs have {fragment}");
    }
    // Positive controls: the run did log at trace, the skipped defender included.
    assert!(
        !captured.events("pvp.defender_build_invalid").is_empty(),
        "{text}"
    );
    assert!(text.contains("/api/pvp/fights"), "{text}");
    for line in captured.lines() {
        for (field, _) in line.as_object().unwrap() {
            let known = ALLOWED_FIELDS.contains(&field.as_str())
                || matches!(field.as_str(), "ts" | "level" | "target");
            assert!(known, "{field} in {line}");
        }
    }
}
