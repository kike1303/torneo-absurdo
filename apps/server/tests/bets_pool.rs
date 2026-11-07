//! 012 RF34, RF35 and RF47: the pools, from the cache that the task fills and from the database, what
//! goes to the hub, and the quote. The task is called by hand (`pool::refresh`), as `f::start` does with
//! the runner (plan-b D9). Every test starts the app, so all run multi-thread.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use protocol::{BetQuote, FightPool, PoolOutcome, Side};
use server::app::{AppState, Settings};
use server::bets::BetsHook;
use server::bets::pool;
use server::fights::{FightClock, FightSettings};
use server::rate::{RateSettings, ServerClock};
use sqlx::PgPool;
use support::app::{ORIGIN, RunningApp};
use support::bets;
use support::db::TestDb;
use support::fights::{self as f, DRAW, KO, T0, Vector, wait_for};
use support::http;
use uuid::Uuid;

struct World {
    _db: TestDb,
    app: RunningApp,
    pool: PgPool,
    state: AppState,
    clock: FightClock,
    rate: ServerClock,
    _runner: f::Running,
}

async fn world() -> World {
    let db = TestDb::create().await;
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let pool = db.migrated_pool().await;
    let (fights, clock) = f::settings_at(T0);
    let rate = ServerClock::manual();
    let settings = Settings {
        fights: FightSettings {
            hook: Arc::new(BetsHook),
            ..fights
        },
        rate: RateSettings {
            clock: rate.clone(),
            ..RateSettings::default()
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
    let runner = f::spawn_runner(
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
    World {
        _db: db,
        app: support::app::start(state.clone()).await,
        pool,
        state,
        clock,
        rate,
        _runner: runner,
    }
}

impl World {
    async fn refresh(&self) {
        pool::refresh(&self.state.db, &self.state.pools, &self.state.fights).await;
    }

    async fn get_pool(&self, id: &str) -> http::Response {
        http::get(self.app.addr, &format!("/api/fights/{id}/pool")).await
    }

    async fn quote(&self, token: &str, id: &str, query: &str) -> http::Response {
        self.rate.advance(Duration::from_secs(1));
        let cookie = support::oauth::session_cookie(token);
        http::send(
            self.app.addr,
            "GET",
            &format!("/api/fights/{id}/quote{query}"),
            &[("cookie", cookie.as_str())],
        )
        .await
    }

    async fn bettor(&self, coins: i64) -> (Uuid, String) {
        let user = bets::funded_user(&self.pool, coins).await;
        (user, support::app::session_for(&self.pool, user).await)
    }

    fn next_pools(&self, seen: &mut u64) -> Vec<FightPool> {
        let hub = self.state.fights.hub();
        std::iter::from_fn(|| hub.next_pool(seen))
            .map(|text| {
                let message: protocol::ServerMessage = serde_json::from_str(text.as_str()).unwrap();
                match message {
                    protocol::ServerMessage::PoolUpdate { pool } => *pool,
                    other => panic!("not a pool: {other:?}"),
                }
            })
            .collect()
    }
}

fn pool_of(response: &http::Response) -> FightPool {
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(response.header("cache-control"), Some("no-store"));
    serde_json::from_slice(&response.body).unwrap()
}

/// 7 and 5 on A, 9 on B, by three users.
async fn sow(pool: &PgPool, id: i32) -> Vec<Uuid> {
    let (a, b) = (
        bets::funded_user(pool, 100).await,
        bets::funded_user(pool, 100).await,
    );
    bets::seed_bet(pool, id, a, "a", 7).await;
    bets::seed_bet(pool, id, a, "a", 5).await;
    bets::seed_bet(pool, id, b, "b", 9).await;
    vec![a, b]
}

/// RF34: the exact sums, `payout_bp` by the function of RF10, the same from the database and from the
/// cache, and no bettor in it. A settled fight carries its settlement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pool_has_sums_and_counts_and_no_one_in_it() {
    let w = world().await;
    let id = f::open_vector_with_rake(&w.pool, T0, &Vector::of(KO), 500).await;
    let users = sow(&w.pool, id).await;
    let from_db = pool_of(&w.get_pool(&id.to_string()).await);
    assert_eq!((from_db.a.stake.get(), from_db.a.bets), (12, 2));
    assert_eq!((from_db.b.stake.get(), from_db.b.bets), (9, 1));
    // `T = 21`, `rake = 1`, `N = 20`: A pays ⌊10000 · 20 / 12⌋, B ⌊10000 · 20 / 9⌋.
    assert_eq!(from_db.a.payout_bp, Some(16_666));
    assert_eq!(from_db.b.payout_bp, Some(22_222));
    assert_eq!(from_db.rake_bp, 500);
    assert_eq!(from_db.settlement, None);
    w.refresh().await;
    let cached = pool_of(&w.get_pool(&id.to_string()).await);
    assert_eq!(
        (cached.a, cached.b, cached.rake_bp),
        (from_db.a, from_db.b, 500)
    );
    let text = w.get_pool(&id.to_string()).await.text();
    for user in users {
        assert!(!text.contains(&user.to_string()), "{text}");
    }
    for bad in ["07", "x", "999999"] {
        assert_eq!(w.get_pool(bad).await.status, 404, "{bad}");
    }
    f::drive_to_settled(&w.pool, &w.clock, id).await;
    let settled = pool_of(&w.get_pool(&id.to_string()).await);
    let settlement = settled.settlement.unwrap();
    assert_eq!(settlement.outcome, PoolOutcome::Paid);
    // `T = 21` with 500 bp: a rake of 1, whoever wins.
    assert_eq!(settlement.rake.get(), 1);
}

/// RF35: a pool goes to the hub only when it changed and has bets; after the close, its last value
/// once, and then nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_changes_with_bets_and_a_last_value_go_out() {
    let w = world().await;
    let id = f::open_vector(&w.pool, T0, &Vector::of(KO)).await;
    let mut seen = 0;
    w.refresh().await;
    assert!(w.next_pools(&mut seen).is_empty(), "no bets, nothing");
    let user = bets::funded_user(&w.pool, 100).await;
    bets::seed_bet(&w.pool, id, user, "a", 25).await;
    w.refresh().await;
    let sent = w.next_pools(&mut seen);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].a.stake.get(), 25);
    w.refresh().await;
    assert!(w.next_pools(&mut seen).is_empty(), "nothing changed");
    // A bet that the task has not seen when the window closes: its last value still goes out.
    bets::seed_bet(&w.pool, id, user, "a", 30).await;
    let (closes_at, _) = f::hours(T0);
    w.clock.set(closes_at);
    f::wait_state(&w.pool, id, protocol::FightState::BettingClosed).await;
    w.refresh().await;
    let last = w.next_pools(&mut seen);
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].a.stake.get(), 55);
    w.refresh().await;
    assert!(
        w.next_pools(&mut seen).is_empty(),
        "closed, and already sent"
    );
}

/// RF47: the vectors of RF10, a one-sided pool, every refusal, and a real bet with the same pool
/// collects what was quoted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_quote_is_what_the_bet_collects() {
    let w = world().await;
    let vector = Vector::of(KO);
    let winner = match vector.log.events.last() {
        Some(protocol::FightEvent::End {
            winner: Some(side), ..
        }) => *side,
        other => panic!("a KO has a winner: {other:?}"),
    };
    let (own, other) = match winner {
        Side::A => ("a", "b"),
        Side::B => ("b", "a"),
    };
    let id = f::open_vector_with_rake(&w.pool, T0, &vector, 500).await;
    let fight = id.to_string();
    let (_, token) = w.bettor(100).await;
    w.refresh().await;
    // No pool yet: one-sided, the bet comes back whole.
    let lonely: BetQuote = serde_json::from_slice(
        &w.quote(&token, &fight, &format!("?side={own}&amount=25"))
            .await
            .body,
    )
    .unwrap();
    assert!(lonely.one_sided);
    assert_eq!((lonely.payout.get(), lonely.net.get()), (25, 0));
    // 75 on the winner's side and 40 on the other: 25 more collects ⌊25 · 133 / 100⌋ = 33.
    let (a, b) = (
        bets::funded_user(&w.pool, 100).await,
        bets::funded_user(&w.pool, 100).await,
    );
    bets::seed_bet(&w.pool, id, a, own, 75).await;
    bets::seed_bet(&w.pool, id, b, other, 40).await;
    w.refresh().await;
    let response = w
        .quote(&token, &fight, &format!("?side={own}&amount=25"))
        .await;
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    let quote: BetQuote = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(
        (quote.payout.get(), quote.net.get(), quote.one_sided),
        (33, 8, false)
    );
    assert_eq!(quote.rake_bp, 500);
    // The largest amount does not panic.
    let huge = w
        .quote(&token, &fight, &format!("?side={own}&amount={}", u32::MAX))
        .await;
    assert_eq!(huge.status, 200, "{}", huge.text());
    // The refusals.
    let code = |response: &http::Response| response.text();
    assert_eq!(
        code(
            &w.quote(&token, &fight, &format!("?side={own}&amount=24"))
                .await
        ),
        r#"{"error":"bet.below_minimum"}"#
    );
    for query in [
        "?side=c&amount=25",
        "?side=a&amount=x",
        "?side=a",
        "?side=a&amount=-25",
        "?side=a&amount=025",
    ] {
        let response = w.quote(&token, &fight, query).await;
        assert_eq!(response.status, 422, "{query}");
        assert_eq!(
            code(&response),
            r#"{"error":"bet.invalid_request"}"#,
            "{query}"
        );
    }
    for bad in ["07", "x", "999999"] {
        let response = w.quote(&token, bad, "?side=a&amount=25").await;
        assert_eq!(response.status, 409, "{bad}");
        assert_eq!(code(&response), r#"{"error":"bet.window_closed"}"#);
    }
    // The real bet, with the pool that was quoted, collects the quote.
    let cookie = support::oauth::session_cookie(&token);
    let placed = http::send_body(
        w.app.addr,
        "POST",
        "/api/bets",
        &[
            ("origin", ORIGIN),
            ("cookie", cookie.as_str()),
            ("content-type", "application/json"),
        ],
        &format!(
            r#"{{"fight_id":{id},"side":"{own}","amount":25,"client_key":"{}"}}"#,
            Uuid::new_v4()
        ),
    )
    .await;
    assert_eq!(placed.status, 201, "{}", placed.text());
    f::drive_to_settled(&w.pool, &w.clock, id).await;
    let collected: i64 = sqlx::query_scalar(
        "SELECT s.payout FROM bet_settlements s JOIN bets b ON b.id = s.bet_id \
         WHERE b.fight_id = $1 AND b.stake = 25",
    )
    .bind(id)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(collected, quote.payout.get());
    // Settled: no quote.
    assert_eq!(
        w.quote(&token, &fight, "?side=a&amount=25").await.status,
        409
    );
}

/// RF27 and RF41 of 010 with bets: the pool does not depend on the seed or the fighters. Two fights
/// with the same stakes give the same pools.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pool_does_not_depend_on_the_seed() {
    let w = world().await;
    let first = f::open_vector(&w.pool, T0, &Vector::of(KO)).await;
    let second = f::open_vector(&w.pool, T0 + f::CADENCE, &Vector::of(DRAW)).await;
    sow(&w.pool, first).await;
    sow(&w.pool, second).await;
    let one = pool_of(&w.get_pool(&first.to_string()).await);
    let two = pool_of(&w.get_pool(&second.to_string()).await);
    assert_eq!((one.a, one.b, one.rake_bp), (two.a, two.b, two.rake_bp));
}
