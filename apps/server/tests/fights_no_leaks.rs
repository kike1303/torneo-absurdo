//! 010 RF27, test 12 of ADR-0008: for each state of the cycle, as an anonymous visitor, with a session
//! and with a session of an admin, every route of the manifest and the `/ws` channels. Before
//! `Settled` neither the seed nor the log appears, in any encoding, in a body, an error or a header.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use protocol::FightState;
use server::app::Settings;
use server::fights::{FightClock, hex};
use server::http::routes::{Handler, ROUTES, RouteSpec};
use sqlx::PgPool;
use support::app::{ORIGIN, TRUST_LOOPBACK, session_for, user_with_dev_account, user_with_session};
use support::db::{TestDb, privileged};
use support::fights::{self as f, KO, T0, Vector, wait_for, wait_state};
use support::http::{self, Response};
use support::ws;

/// What must not appear before `Settled`, in every encoding.
fn canaries(vector: &Vector) -> Vec<String> {
    let seed = vector.seed.as_slice();
    let mut out = vec![
        hex(seed),
        hex(seed).to_uppercase(),
        STANDARD.encode(seed),
        URL_SAFE_NO_PAD.encode(seed),
        serde_json::to_string(seed).unwrap(),
        format!("{seed:?}"),
        format!("{seed:?}").replace(' ', ""),
        String::from_utf8(vector.canonical()).unwrap(),
    ];
    // Every event, with its `t` and its data, `ko` and `end` included.
    for event in &vector.log.events {
        out.push(serde_json::to_string(event).unwrap());
    }
    out
}

/// What a request of a route is: an exhaustive `match`, so a new route without a case does not
/// compile.
fn request_of(route: &RouteSpec, id: i32) -> (String, Vec<(&'static str, &'static str)>, String) {
    let path = route
        .path
        .replace("{provider}", "google")
        .replace("{id}", &id.to_string());
    let json = ("content-type", "application/json");
    let form = ("content-type", "application/x-www-form-urlencoded");
    let (headers, body): (Vec<(&'static str, &'static str)>, String) = match route.handler {
        Handler::Health
        | Handler::Ws
        | Handler::AuthProviders
        | Handler::AuthReauthStatus
        | Handler::Me
        | Handler::FightsCurrent
        | Handler::FightDetail => (vec![], String::new()),
        Handler::AuthCallback => (vec![], String::new()),
        Handler::AuthStart | Handler::AuthLink | Handler::AuthReauth => {
            (vec![form], "next=%2F".to_owned())
        }
        Handler::AuthUnlink | Handler::Logout | Handler::LogoutAll => (vec![form], String::new()),
        Handler::MeDisplayName => (vec![json], r#"{"display_name":"TestRename"}"#.to_owned()),
        Handler::MePreferences => (vec![json], r#"{"locale":"es"}"#.to_owned()),
        #[cfg(feature = "dev-auth")]
        Handler::DevAuthorize | Handler::DevUserinfo => (vec![], String::new()),
        #[cfg(feature = "dev-auth")]
        Handler::DevToken => (vec![form], "grant_type=authorization_code".to_owned()),
    };
    (path, headers, body)
}

struct Identities {
    user: uuid::Uuid,
    admin: uuid::Uuid,
}

async fn admin_session(pool: &PgPool, admin: uuid::Uuid) -> String {
    let mut tx = pool.begin().await.unwrap();
    let issued = server::sessions::issue(
        &mut tx,
        admin,
        protocol::Role::Admin,
        server::sessions::Authentication::None,
        &[0; 32],
        server::sessions::UaFamily::Other,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    issued.token.as_str().to_owned()
}

/// A different client for each request, so no quota gets in the way.
static CLIENT: AtomicU32 = AtomicU32::new(1);

fn next_ip() -> String {
    let n = CLIENT.fetch_add(1, Ordering::SeqCst);
    format!("10.{}.{}.{}", (n >> 16) & 255, (n >> 8) & 255, n & 255)
}

fn assert_clean(what: &str, response: &Response, canaries: &[String]) {
    let body = String::from_utf8_lossy(&response.body);
    for canary in canaries {
        assert!(
            !body.contains(canary.as_str()),
            "{what}: the body has {canary}"
        );
        for (name, value) in &response.headers {
            let value = String::from_utf8_lossy(value.as_bytes());
            assert!(
                !value.contains(canary.as_str()),
                "{what}: {name} has {canary}"
            );
        }
    }
    // No fight data travels in a header, whatever its encoding.
    assert!(response.status.as_u16() < 600);
}

/// Every route, with each identity. In `Settled` the detail of the fight is the one that publishes.
async fn sweep(
    addr: SocketAddr,
    pool: &PgPool,
    who: &Identities,
    cookie_name: &str,
    id: i32,
    state: FightState,
    canaries: &[String],
) {
    let session = |token: String| format!("{cookie_name}={token}");
    for route in ROUTES {
        if state == FightState::Settled && route.handler == Handler::FightDetail {
            continue;
        }
        for identity in ["anonymous", "user", "admin"] {
            // A new session each time: a logout revokes what it is given.
            let cookie = match identity {
                "anonymous" => None,
                "user" => Some(session(session_for(pool, who.user).await)),
                _ => Some(session(admin_session(pool, who.admin).await)),
            };
            let (path, headers, body) = request_of(route, id);
            let ip = next_ip();
            let mut all: Vec<(&str, &str)> =
                vec![("origin", ORIGIN), ("x-forwarded-for", ip.as_str())];
            all.extend(headers);
            if let Some(cookie) = &cookie {
                all.push(("cookie", cookie.as_str()));
            }
            let method = route.method.http();
            let response = http::send_body(addr, method.as_str(), &path, &all, &body).await;
            let what = format!("{state:?} {identity} {method} {path}");
            assert_ne!(response.status.as_u16(), 500, "{what}");
            assert_clean(&what, &response, canaries);
        }
    }
    // The channels of `/ws` that exist in phase A: `self`, with a session, and the refusal of the
    // others. Anything that arrives is checked too.
    let token = session_for(pool, who.user).await;
    let cookie = session(token);
    let mut client = ws::connect_with(addr, ORIGIN, &[("cookie", cookie.as_str())])
        .await
        .unwrap();
    ws::send_text(&mut client, r#"{"type":"subscribe","channel":"self"}"#).await;
    ws::send_text(&mut client, r#"{"type":"subscribe","channel":"schedule"}"#).await;
    ws::send_text(&mut client, r#"{"type":"subscribe","channel":"fight:1"}"#).await;
    while let Some(message) = ws::next_data(&mut client, Duration::from_millis(100)).await {
        let text = message.into_text().unwrap();
        for canary in canaries {
            assert!(!text.contains(canary.as_str()), "/ws has {canary}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nothing_leaks_in_any_state_with_any_identity() {
    let db = TestDb::create().await;
    let (fight_settings, clock): (_, FightClock) = f::settings_at(T0 - 1);
    let settings = Settings {
        fights: fight_settings.clone(),
        ..Settings::default()
    };
    let (app, pool, shared) = f::start_app(&db, &[TRUST_LOOPBACK], &settings).await;

    let (user, _) = user_with_session(&pool).await;
    let (admin, _) = user_with_dev_account(&pool).await;
    privileged(
        &pool,
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
    )
    .await;
    let who = Identities { user, admin };
    let cookie_name = {
        let config = support::config::config(&[]);
        config.cookies.session_name().to_owned()
    };

    let vector = Vector::of(KO);
    let canaries = canaries(&vector);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let _runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), fight_settings);
    wait_for("the board", || async {
        shared.current().halted == Some(false)
    })
    .await;

    sweep(
        app.addr,
        &pool,
        &who,
        &cookie_name,
        id,
        FightState::Scheduled,
        &canaries,
    )
    .await;

    // The test puts the seed of its vector on the legal route, so its canaries are the real ones.
    f::open_with(&pool, id, &vector.seed, T0).await;
    clock.set(T0);
    wait_for("the board", || async {
        let current = shared.current().current;
        current.is_some_and(|fight| fight.state == FightState::BettingOpen)
    })
    .await;
    sweep(
        app.addr,
        &pool,
        &who,
        &cookie_name,
        id,
        FightState::BettingOpen,
        &canaries,
    )
    .await;

    let (closes_at, live_at) = f::hours(T0);
    clock.set(closes_at);
    wait_state(&pool, id, FightState::BettingClosed).await;
    sweep(
        app.addr,
        &pool,
        &who,
        &cookie_name,
        id,
        FightState::BettingClosed,
        &canaries,
    )
    .await;

    clock.set(live_at);
    wait_state(&pool, id, FightState::Live).await;
    sweep(
        app.addr,
        &pool,
        &who,
        &cookie_name,
        id,
        FightState::Live,
        &canaries,
    )
    .await;

    clock.set(live_at + 100_000);
    wait_state(&pool, id, FightState::Settled).await;
    // RF34: and now it is the detail that has them, and no other route.
    sweep(
        app.addr,
        &pool,
        &who,
        &cookie_name,
        id,
        FightState::Settled,
        &canaries,
    )
    .await;
    let published = http::send(app.addr, "GET", &format!("/api/fights/{id}"), &[]).await;
    assert_eq!(published.status, 200);
    assert!(published.text().contains(&hex(&vector.seed)));
}
