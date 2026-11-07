//! 010 RF34, RF35, RF36 and the security review R3: the read routes. 011 RF7 to RF9: the list.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;

use protocol::{CancelReason, FightDetail, FightState, FightsCurrent, FightsPage};
use server::app::Settings;
use server::fights::{FightSettings, hex, store};
use server::http::routes::{Effect, ROUTES};
use server::rate::{RateSettings, ServerClock};
use sqlx::PgPool;
use support::db::TestDb;
use support::fights::{self as f, CADENCE, KO, T0, Vector, WINDOW, wait_for, wait_state};
use support::http::{self, Response};

async fn get(addr: SocketAddr, path: &str) -> Response {
    http::send(addr, "GET", path, &[]).await
}

fn assert_no_store(response: &Response) {
    assert_eq!(response.header("cache-control"), Some("no-store"));
}

fn detail_of(response: &Response) -> FightDetail {
    assert_eq!(response.status, 200, "{}", response.text());
    assert_no_store(response);
    serde_json::from_slice(&response.body).unwrap()
}

fn current_of(response: &Response) -> FightsCurrent {
    assert_eq!(response.status, 200, "{}", response.text());
    assert_no_store(response);
    serde_json::from_slice(&response.body).unwrap()
}

#[test]
fn the_routes_only_read() {
    for path in [
        "/api/fights",
        "/api/fights/current",
        "/api/fights/void-stats",
        "/api/fights/{id}",
    ] {
        let route = ROUTES.iter().find(|route| route.path == path).unwrap();
        assert_eq!(route.effect, Effect::Read);
        assert_eq!(route.method.http().as_str(), "GET");
    }
}

/// RF34: an id is a canonical decimal in range. Everything else, and a fight that does not exist,
/// is the same 404. Never the text of axum, not even with bytes that are not UTF-8.
#[tokio::test]
async fn an_id_is_canonical_or_it_is_not_found() {
    let db = TestDb::create().await;
    let (app, _pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    for id in [
        "0",
        "01",
        "%2B1",
        "-1",
        "1.0",
        "2147483648",
        "99999999999999999999",
        "uno",
        "%FF",
        "%20",
        "1%00",
        "2147483647",
        "12345",
    ] {
        let response = get(app.addr, &format!("/api/fights/{id}")).await;
        assert_eq!(response.status, 404, "{id}");
        assert_eq!(response.text(), r#"{"error":"fight.not_found"}"#, "{id}");
        assert_no_store(&response);
    }
}

/// RF34: a failure of the database is 503, and `current` does not touch it (RF35).
#[tokio::test]
async fn with_the_database_down() {
    let state = support::app::state(&[], &Settings::default());
    let app = support::app::start(state).await;
    let response = get(app.addr, "/api/fights/1").await;
    assert_eq!(response.status, 503, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"fight.unavailable"}"#);
    assert_no_store(&response);

    // RF35: nobody loaded it, so everything is null, with a 200.
    let current = current_of(&get(app.addr, "/api/fights/current").await);
    assert_eq!(current.halted, None);
    assert_eq!(current.current, None);
    assert_eq!(current.next, None);
    assert_eq!(current.last, None);
}

/// RF34: `FIGHT_READ_IP`, 120 per minute with a burst of 30 per `IpKey`; `current` has no limit.
#[tokio::test]
async fn the_detail_has_a_quota_and_the_board_does_not() {
    let settings = Settings {
        rate: RateSettings {
            clock: ServerClock::manual(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    let db = TestDb::create().await;
    let (app, _pool, _shared) = f::start_app(&db, &[], &settings).await;
    for _ in 0..30 {
        assert_eq!(get(app.addr, "/api/fights/1").await.status, 404);
    }
    let limited = get(app.addr, "/api/fights/1").await;
    assert_eq!(limited.status, 429);
    assert_eq!(limited.text(), r#"{"error":"http.rate_limited"}"#);
    assert!(limited.header("retry-after").is_some());
    assert_no_store(&limited);
    for _ in 0..50 {
        assert_eq!(get(app.addr, "/api/fights/current").await.status, 200);
    }
}

/// RF34: the bytes as stored, in every state, and `seed` and `log` only once settled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seed_and_log_only_appear_in_a_settled_fight() {
    let db = TestDb::create().await;
    let (fight_settings, clock): (FightSettings, _) = f::settings_at(T0 - 1);
    let settings = Settings {
        fights: fight_settings.clone(),
        ..Settings::default()
    };
    let (app, pool, shared) = f::start_app(&db, &[], &settings).await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let path = format!("/api/fights/{id}");
    let spec_a = String::from_utf8(engine::canonical_spec_json(&vector.a).unwrap()).unwrap();

    // RF35: without the lock nothing is known yet, also with rows in the database.
    let current = current_of(&get(app.addr, "/api/fights/current").await);
    assert_eq!(current.halted, None);
    assert_eq!(current.current, None);

    // From `scheduled` on.
    let scheduled = detail_of(&get(app.addr, &path).await);
    assert_eq!(scheduled.fight.state, FightState::Scheduled);
    assert_eq!(scheduled.spec_a.as_deref(), Some(spec_a.as_str()));
    assert_eq!(scheduled.seed, None);
    assert_eq!(scheduled.log, None);
    assert_eq!(scheduled.fight.seed_commit, None);

    let _runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), fight_settings);
    wait_for("the board", || async {
        shared.current().halted == Some(false)
    })
    .await;
    let board = current_of(&get(app.addr, "/api/fights/current").await);
    assert_eq!(
        board.current.as_ref().unwrap().id,
        u32::try_from(id).unwrap()
    );
    assert_eq!(board.next, None);
    assert_eq!(board.server_time.as_ms(), Some(T0 - 1));

    clock.set(T0);
    wait_state(&pool, id, FightState::BettingOpen).await;
    let open = detail_of(&get(app.addr, &path).await);
    assert_eq!(open.fight.state, FightState::BettingOpen);
    assert!(open.fight.seed_commit.is_some());
    assert_eq!((open.seed, open.log), (None, None));

    clock.set(T0 + WINDOW);
    wait_state(&pool, id, FightState::BettingClosed).await;
    let closed = detail_of(&get(app.addr, &path).await);
    assert_eq!((closed.seed, closed.log), (None, None));

    let (_, live_at) = f::hours(T0);
    clock.set(live_at);
    wait_state(&pool, id, FightState::Live).await;
    let live = detail_of(&get(app.addr, &path).await);
    assert_eq!(live.fight.live_started_at.unwrap().as_ms(), Some(live_at));
    assert_eq!((live.seed, live.log), (None, None));

    clock.set(live_at + 100_000);
    wait_state(&pool, id, FightState::Settled).await;
    let settled = detail_of(&get(app.addr, &path).await);
    assert!(settled.seed.is_some());
    assert!(settled.log.is_some());
    wait_for("the board", || async { shared.current().last.is_some() }).await;
    let board = current_of(&get(app.addr, "/api/fights/current").await);
    assert_eq!(board.current, None);
    assert_eq!(board.last.unwrap().id, u32::try_from(id).unwrap());
}

/// DP-10 and R3: a spec that is not UTF-8 goes `null` and hides nothing else. On a voided fight its
/// seed comes out and the commit can be checked; before `settled` there is no seed, whatever the row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bytes_that_are_not_utf8_never_hide_nor_reveal_a_seed() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let vector = Vector::of(KO);
    let id = f::announce_with(&pool, T0, &vector.a, &vector.b, |raw| {
        raw.spec_a = vec![0xff, 0xfe];
    })
    .await;
    let path = format!("/api/fights/{id}");
    f::open_with(&pool, id, &vector.seed, T0).await;
    f::close_by_sql(&pool, id, T0 + WINDOW).await;
    let (_, live_at) = f::hours(T0);
    f::live_by_sql(&pool, id, live_at).await;

    // R3: in `live`, the spec that cannot be shown, and still no seed.
    let live = detail_of(&get(app.addr, &path).await);
    assert_eq!(live.spec_a, None);
    assert!(live.spec_b.is_some());
    assert_eq!(live.seed, None);
    assert_eq!(live.log, None);

    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, id, &vector.seed, "snapshot_invalid").await;
    tx.commit().await.unwrap();

    let voided = detail_of(&get(app.addr, &path).await);
    assert_eq!(voided.spec_a, None);
    assert!(voided.spec_b.is_some());
    let seed = voided.seed.clone().unwrap();
    assert_eq!(seed, hex(&vector.seed));
    support::public_verifier::verify(&voided).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(
        hex(&Sha256::digest(vector.seed)),
        voided.fight.seed_commit.unwrap()
    );
}

/// RF34: a query waiting for a lock answers 503 in a bounded time, and the pool is fine afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_query_that_waits_for_a_lock_is_a_503() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let path = format!("/api/fights/{id}");

    let mut locked = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE fights IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *locked)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    for _ in 0..3 {
        let response = get(app.addr, &path).await;
        assert_eq!(response.status, 503, "{}", response.text());
        assert_eq!(response.text(), r#"{"error":"fight.unavailable"}"#);
        assert_no_store(&response);
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
    locked.rollback().await.unwrap();
    assert_eq!(get(app.addr, &path).await.status, 200);
}

fn page_of(response: &Response) -> FightsPage {
    assert_eq!(response.status, 200, "{}", response.text());
    assert_no_store(response);
    serde_json::from_slice(&response.body).unwrap()
}

fn ids(page: &FightsPage) -> Vec<i32> {
    page.fights
        .iter()
        .map(|fight| i32::try_from(fight.id).unwrap())
        .collect()
}

/// `count` fights cancelled on the legal route, the oldest first.
async fn cancelled_fights(pool: &PgPool, count: i64) -> Vec<i32> {
    let vector = Vector::of(KO);
    let mut out = Vec::new();
    for slot in 0..count {
        let id = f::announce(pool, T0 + slot * CADENCE, &vector.a, &vector.b).await;
        cancel(pool, id, CancelReason::MissedWindow).await;
        out.push(id);
    }
    out
}

async fn cancel(pool: &PgPool, id: i32, reason: CancelReason) {
    let mut tx = pool.begin().await.unwrap();
    assert!(store::cancel(&mut tx, id, reason, T0).await.unwrap());
    tx.commit().await.unwrap();
}

/// 011 RF7: the highest id first, 20 at most, `next_before` while there are more, and every page
/// walks on from where the last one ended.
#[tokio::test]
async fn the_list_goes_from_the_most_recent_id_page_by_page() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;

    let empty = page_of(&get(app.addr, "/api/fights").await);
    assert_eq!(empty.fights, []);
    assert_eq!(empty.next_before, None);

    let all = cancelled_fights(&pool, 41).await;
    let newest: Vec<i32> = all.iter().rev().copied().collect();

    let first = page_of(&get(app.addr, "/api/fights").await);
    assert_eq!(ids(&first), newest[..20]);
    assert_eq!(first.next_before, u32::try_from(newest[19]).ok());

    let path = format!("/api/fights?before={}", first.next_before.unwrap());
    let second = page_of(&get(app.addr, &path).await);
    assert_eq!(ids(&second), newest[20..40]);
    assert_eq!(second.next_before, u32::try_from(newest[39]).ok());

    let path = format!("/api/fights?before={}", second.next_before.unwrap());
    let third = page_of(&get(app.addr, &path).await);
    assert_eq!(ids(&third), newest[40..]);
    assert_eq!(third.next_before, None);

    // Below the lowest there is nothing, and that is not an error.
    let path = format!("/api/fights?before={}", all[0]);
    let past = page_of(&get(app.addr, &path).await);
    assert_eq!(past.fights, []);
    assert_eq!(past.next_before, None);
    // Above the highest, and at the top of the range, it is the first page.
    for before in [newest[0] + 1, i32::MAX] {
        let path = format!("/api/fights?before={before}");
        assert_eq!(ids(&page_of(&get(app.addr, &path).await)), newest[..20]);
    }
    // There is no `limit`, and an unknown parameter changes nothing.
    let path = "/api/fights?limit=1";
    assert_eq!(ids(&page_of(&get(app.addr, path).await)), newest[..20]);
}

/// 011 RF7: exactly a full page is the end of the list: no page that comes empty.
#[tokio::test]
async fn a_list_of_exactly_one_page_ends_there() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    cancelled_fights(&pool, 20).await;
    let page = page_of(&get(app.addr, "/api/fights").await);
    assert_eq!(page.fights.len(), 20);
    assert_eq!(page.next_before, None);
}

/// 011 RF7: `i32::MAX` itself enters the first page, and a `before` of it leaves it out.
#[tokio::test]
async fn the_highest_id_of_the_range_is_in_the_list() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    sqlx::query("ALTER TABLE fights ALTER COLUMN id RESTART WITH 2147483647")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cancelled_fights(&pool, 1).await, [i32::MAX]);
    let first = page_of(&get(app.addr, "/api/fights").await);
    assert_eq!(ids(&first), [i32::MAX]);
    let path = format!("/api/fights?before={}", i32::MAX);
    assert_eq!(page_of(&get(app.addr, &path).await).fights, []);
}

/// 011 RF7: the rule of `parse_id`, and the same 404. Also a `before` that comes twice.
#[tokio::test]
async fn a_before_that_is_not_an_id_is_not_found() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    cancelled_fights(&pool, 1).await;
    for before in [
        "",
        "0",
        "01",
        "%2B1",
        "+1",
        "-1",
        "1.0",
        "0x1",
        "2147483648",
        "99999999999999999999",
        "uno",
        "%FF",
        "%20",
        "1%00",
    ] {
        let response = get(app.addr, &format!("/api/fights?before={before}")).await;
        assert_eq!(response.status, 404, "{before}");
        assert_eq!(
            response.text(),
            r#"{"error":"fight.not_found"}"#,
            "{before}"
        );
        assert_no_store(&response);
    }
    let response = get(app.addr, "/api/fights?before=1&before=2").await;
    assert_eq!(response.status, 404);
    assert_eq!(response.text(), r#"{"error":"fight.not_found"}"#);
    assert_no_store(&response);
}

/// 011 RF8: a fight in each state: only the settled ones, with exactly what the detail says of them,
/// and neither seed nor log.
#[tokio::test]
async fn the_list_only_has_settled_fights_with_nothing_of_the_detail() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let vector = Vector::of(KO);
    let slot = |n: i64| T0 + n * CADENCE;

    let scheduled = f::announce(&pool, slot(0), &vector.a, &vector.b).await;
    let open = f::open_vector(&pool, slot(1), &vector).await;
    let closed = f::open_vector(&pool, slot(2), &vector).await;
    f::close_by_sql(&pool, closed, slot(2) + WINDOW).await;
    let live = f::open_vector(&pool, slot(3), &vector).await;
    f::close_by_sql(&pool, live, slot(3) + WINDOW).await;
    f::live_by_sql(&pool, live, f::hours(slot(3)).1).await;

    let cancelled = f::announce(&pool, slot(4), &vector.a, &vector.b).await;
    cancel(&pool, cancelled, CancelReason::FightUnrunnable).await;
    let voided = f::open_vector(&pool, slot(5), &vector).await;
    f::close_by_sql(&pool, voided, slot(5) + WINDOW).await;
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, voided, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();
    let decided = f::open_vector(&pool, slot(6), &vector).await;
    f::close_by_sql(&pool, decided, slot(6) + WINDOW).await;
    f::live_by_sql(&pool, decided, f::hours(slot(6)).1).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE fights SET state = 'settled', settled_at = now(), winner = 'a', end_reason = 'ko', \
         revealed_seed = $2, log = $3 WHERE id = $1",
    )
    .bind(decided)
    .bind(vector.seed.as_slice())
    .bind(vector.canonical())
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("DELETE FROM fight_secrets WHERE fight_id = $1")
        .bind(decided)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let response = get(app.addr, "/api/fights").await;
    let page = page_of(&response);
    assert_eq!(ids(&page), [decided, voided, cancelled]);
    for id in [scheduled, open, closed, live] {
        assert!(!ids(&page).contains(&id), "{id}");
    }
    for fight in &page.fights {
        assert_eq!(fight.state, FightState::Settled);
        let path = format!("/api/fights/{}", fight.id);
        assert_eq!(*fight, detail_of(&get(app.addr, &path).await).fight);
    }
    // What the detail has of a settled fight, in no encoding of the list.
    let text = response.text();
    assert!(!text.contains(&hex(&vector.seed)));
    assert!(!text.contains(r#""t":"#));
    for field in [r#""seed""#, r#""log""#, r#""spec_a""#, r#""spec_b""#] {
        assert!(!text.contains(field), "{field}");
    }
}

/// 011 RF8: a row the `CHECK`s forbid does not take the page down, and the rest of the list stays.
#[tokio::test]
async fn a_row_without_a_summary_is_left_out_of_the_list() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let all = cancelled_fights(&pool, 3).await;
    let vector = Vector::of(KO);
    let bad = f::announce(&pool, T0 + 3 * CADENCE, &vector.a, &vector.b).await;
    let last = f::announce(&pool, T0 + 4 * CADENCE, &vector.a, &vector.b).await;
    cancel(&pool, last, CancelReason::MissedWindow).await;
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

    let page = page_of(&get(app.addr, "/api/fights").await);
    assert_eq!(ids(&page), [last, all[2], all[1], all[0]]);
    // The one left out is no surprise for the detail, which has always said 503.
    let detail = get(app.addr, &format!("/api/fights/{bad}")).await;
    assert_eq!(detail.status, 503);
}

/// 011 RF7: a failure of the database is the 503 of the detail.
#[tokio::test]
async fn the_list_with_the_database_down() {
    let state = support::app::state(&[], &Settings::default());
    let app = support::app::start(state).await;
    let response = get(app.addr, "/api/fights").await;
    assert_eq!(response.status, 503, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"fight.unavailable"}"#);
    assert_no_store(&response);
}

/// 011 RF7: one bucket, shared with the detail, also for the 404 of a bad `before`.
#[tokio::test]
async fn the_list_shares_its_quota_with_the_detail() {
    let settings = Settings {
        rate: RateSettings {
            clock: ServerClock::manual(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    let db = TestDb::create().await;
    let (app, _pool, _shared) = f::start_app(&db, &[], &settings).await;
    for _ in 0..10 {
        assert_eq!(get(app.addr, "/api/fights").await.status, 200);
    }
    for _ in 0..10 {
        assert_eq!(get(app.addr, "/api/fights/1").await.status, 404);
    }
    for _ in 0..10 {
        assert_eq!(get(app.addr, "/api/fights?before=x").await.status, 404);
    }
    for path in ["/api/fights", "/api/fights/1", "/api/fights?before=x"] {
        let limited = get(app.addr, path).await;
        assert_eq!(limited.status, 429, "{path}");
        assert_eq!(limited.text(), r#"{"error":"http.rate_limited"}"#);
        assert!(limited.header("retry-after").is_some());
        assert_no_store(&limited);
    }
    assert_eq!(get(app.addr, "/api/fights/current").await.status, 200);
}

/// 011 RF8: the list is bounded like the detail while a query waits for a lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_list_waiting_for_a_lock_is_a_503() {
    let db = TestDb::create().await;
    let (app, pool, _shared) = f::start_app(&db, &[], &Settings::default()).await;
    let mut locked = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE fights IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *locked)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let response = get(app.addr, "/api/fights").await;
    assert_eq!(response.status, 503, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"fight.unavailable"}"#);
    assert_no_store(&response);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
    locked.rollback().await.unwrap();
    assert_eq!(get(app.addr, "/api/fights").await.status, 200);
}
