//! 010 RF34, RF35, RF36 and the security review R3: the two read routes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;

use protocol::{FightDetail, FightState, FightsCurrent};
use server::app::Settings;
use server::fights::{FightSettings, hex};
use server::http::routes::{Effect, ROUTES};
use server::rate::{RateSettings, ServerClock};
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector, WINDOW, wait_for, wait_state};
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
    for path in ["/api/fights/current", "/api/fights/{id}"] {
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
