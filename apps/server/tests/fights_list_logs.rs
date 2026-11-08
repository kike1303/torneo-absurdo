//! 011 RF8 (ADR-0012 A3): a row left out of `GET /api/fights` logs `fights.list_row_invalid`, with its
//! kind of error and nothing that says which fight it is.
//!
//! Exactly one test in this binary (`tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use protocol::FightsPage;
use server::app::Settings;
use server::telemetry::ALLOWED_FIELDS;
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector};
use support::{http, logs};

#[test]
fn a_row_left_out_of_the_list_logs_only_its_kind_of_error() {
    let vector = Vector::of(KO);
    let slugs = (vector.a.slug.clone(), vector.b.slug.clone());
    let captured = logs::run("warn", |_captured| async {
        let db = TestDb::create().await;
        let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
        let bad = f::announce(&pool, T0, &vector.a, &vector.b).await;
        sqlx::query("ALTER TABLE fights DROP CONSTRAINT fights_cancel_reason_ck")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE fights SET state = 'settled', cancel_reason = 'other', settled_at = now() \
             WHERE id = $1",
        )
        .bind(bad)
        .execute(&pool)
        .await
        .unwrap();

        let response = http::send(app.addr, "GET", "/api/fights", &[]).await;
        assert_eq!(response.status, 200, "{}", response.text());
        let page: FightsPage = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(page.fights, []);
        pool.close().await;
    });

    let lines = captured.events("fights.list_row_invalid");
    assert_eq!(lines.len(), 1, "{}", captured.text());
    let line = &lines[0];
    assert_eq!(line["level"], "warn", "{line}");
    assert_eq!(line["error_kind"], "bad_row", "{line}");
    assert!(!ALLOWED_FIELDS.contains(&"fight_id"));
    let fields: Vec<&String> = line.as_object().unwrap().keys().collect();
    for field in &fields {
        assert!(
            ["event", "error_kind", "ts", "level", "target"].contains(&field.as_str()),
            "{field} in {line}"
        );
    }
    for slug in [&slugs.0, &slugs.1] {
        assert!(!line.to_string().contains(slug.as_str()), "{line}");
    }
}
