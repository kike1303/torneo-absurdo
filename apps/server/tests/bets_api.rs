//! 012 RF5, RF25 to RF28 and RF33: `POST /api/bets` and `GET /api/bets/mine` against the app with a
//! real runner on a manual clock, so the window is the one of 010 RF29. Every test starts the app,
//! so all run multi-thread (see `TestDb`'s drop).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use protocol::{BetPlaced, MyFightBets};
use server::app::{AppState, Settings};
use server::bets::BetsHook;
use server::bets::api::BETS_MAX_PER_FIGHT;
use server::fights::{FightClock, FightSettings};
use server::rate::{RateSettings, ServerClock};
use sqlx::{PgPool, Row};
use support::app::{ORIGIN, RunningApp};
use support::bets;
use support::db::{TestDb, begin_privileged};
use support::fights::{self as f, KO, T0, Vector, wait_for};
use support::http;
use uuid::Uuid;

/// `DAILY_GRANT` of the test config: `B = 100`, so the cap is 100 in probation and 500 outside.
const B: i64 = 100;

struct World {
    _db: TestDb,
    app: RunningApp,
    pool: PgPool,
    clock: FightClock,
    /// The limiters' clock: each bet moves it 2 s, one token of `BET_USER` (30/min), so the tests of
    /// the rows never meet D7.
    rate: ServerClock,
    _runner: f::Running,
    shared: Arc<server::fights::FightsShared>,
    fight: i32,
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
    let fight = f::open_vector(&pool, T0, &Vector::of(KO)).await;
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
        shared: state.fights.clone(),
        app: support::app::start(state).await,
        pool,
        clock,
        rate,
        _runner: runner,
        fight,
    }
}

impl World {
    async fn bettor(&self, coins: i64) -> (Uuid, String) {
        let user = bets::funded_user(&self.pool, coins).await;
        (user, support::app::session_for(&self.pool, user).await)
    }

    async fn post_raw(&self, token: &str, body: &str) -> http::Response {
        self.rate.advance(Duration::from_secs(2));
        let cookie = support::oauth::session_cookie(token);
        http::send_body(
            self.app.addr,
            "POST",
            "/api/bets",
            &[
                ("origin", ORIGIN),
                ("cookie", cookie.as_str()),
                ("content-type", "application/json"),
            ],
            body,
        )
        .await
    }

    async fn bet_with(
        &self,
        token: &str,
        fight: i64,
        side: &str,
        amount: i64,
        key: Uuid,
    ) -> http::Response {
        self.post_raw(
            token,
            &format!(
                r#"{{"fight_id":{fight},"side":"{side}","amount":{amount},"client_key":"{key}"}}"#
            ),
        )
        .await
    }

    async fn bet(&self, token: &str, side: &str, amount: i64) -> http::Response {
        self.bet_with(token, i64::from(self.fight), side, amount, Uuid::new_v4())
            .await
    }

    async fn mine(&self, token: &str, query: &str) -> http::Response {
        let cookie = support::oauth::session_cookie(token);
        http::send(
            self.app.addr,
            "GET",
            &format!("/api/bets/mine{query}"),
            &[("cookie", cookie.as_str())],
        )
        .await
    }

    /// Every row a bet writes: bets, entries and events.
    async fn written(&self) -> (i64, i64, i64) {
        let row = sqlx::query(
            "SELECT (SELECT count(*) FROM bets), (SELECT count(*) FROM ledger_entries), \
             (SELECT count(*) FROM auth_events WHERE kind = 'bet')",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();
        (row.get(0), row.get(1), row.get(2))
    }
}

fn placed(response: &http::Response, status: u16) -> BetPlaced {
    assert_eq!(response.status, status, "{}", response.text());
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    serde_json::from_slice(&response.body).unwrap()
}

fn refusal(response: &http::Response, status: u16, code: &str) {
    assert_eq!(response.status, status, "{}", response.text());
    assert_eq!(response.text(), format!(r#"{{"error":"{code}"}}"#));
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
}

/// Out of probation (RF27): 8 days old, and 10 fights with a bet lost. The fights stay scheduled, so
/// their stakes of 1 count as open; the ledger is not touched.
async fn mature(pool: &PgPool, user: Uuid, fights: i64) {
    sqlx::query("UPDATE users SET created_at = now() - interval '8 days' WHERE id = $1")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    for k in 1..=fights {
        let id = f::announce(
            pool,
            T0 + k * f::CADENCE,
            &f::golden_spec("striker"),
            &f::golden_spec("tank"),
        )
        .await;
        settled_bet(pool, id, user, "lost").await;
    }
}

/// A bet and its result written straight, on a fight that takes none: the guard is off for it.
async fn settled_bet(pool: &PgPool, fight: i32, user: Uuid, outcome: &str) {
    let bet = Uuid::new_v4();
    let mut tx = begin_privileged(pool).await;
    sqlx::query("ALTER TABLE bets DISABLE TRIGGER USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         VALUES ($1, $2, $3, 'a', 1, false, $4)",
    )
    .bind(bet)
    .bind(fight)
    .bind(user)
    .bind(Uuid::new_v4())
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE bets ENABLE TRIGGER USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    let (payout, net) = if outcome == "refunded" {
        (1, 0)
    } else {
        (0, -1)
    };
    sqlx::query(
        "INSERT INTO bet_settlements (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
         VALUES ($1, $2, $3, $4, $5, $6, $6)",
    )
    .bind(bet)
    .bind(fight)
    .bind(user)
    .bind(outcome)
    .bind(payout)
    .bind(net)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// RF25 and RF5: a bet goes in whole, with its stake, its event and the photo of probation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bet_goes_in_with_its_stake_and_its_event() {
    let w = world().await;
    let (user, token) = w.bettor(B).await;
    let answer = placed(&w.bet(&token, "a", 25).await, 201);
    assert_eq!(answer.wallet.balance.get(), B - 25);
    assert_eq!(answer.wallet.open_stakes.get(), 25);
    assert_eq!(answer.mine.side, Some(protocol::Side::A));
    assert_eq!(answer.mine.staked.get(), 25);
    assert_eq!(answer.mine.bets.len(), 1);
    assert_eq!(answer.mine.max_stake.get(), B - 25);
    let row = sqlx::query(
        "SELECT b.stake, b.ranked_stake, p.amount FROM bets b \
         JOIN ledger_entries e ON e.bet_id = b.id AND e.kind = 'bet_stake' \
         JOIN ledger_postings p ON p.entry_id = e.id AND p.account_id = $2 \
         WHERE b.user_id = $1",
    )
    .bind(user)
    .bind(server::ledger::ESCROW_ACCOUNT)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>(0), 25);
    // RF5: an account in probation stakes outside the ranking.
    assert!(!row.get::<bool, _>(1));
    assert_eq!(row.get::<i64, _>(2), 25);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = 'bet' AND bet_id IS NOT NULL",
    )
    .bind(user)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(events, 1);

    // RF5: out of probation, `true`, and the cap is `m_cap · B`.
    let (grown, grown_token) = w.bettor(1000).await;
    mature(&w.pool, grown, 10).await;
    let answer = placed(&w.bet(&grown_token, "b", 300).await, 201);
    assert_eq!(answer.wallet.probation, None);
    assert_eq!(answer.wallet.fight_cap.get(), 5 * B);
    let ranked: bool =
        sqlx::query_scalar("SELECT ranked_stake FROM bets WHERE user_id = $1 AND fight_id = $2")
            .bind(grown)
            .bind(w.fight)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert!(ranked);
}

/// RF25, row 3: the same key with the same body is the same bet, without writing; with another
/// body, `bet.key_reused`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_with_its_key_is_the_same_bet() {
    let w = world().await;
    let (_, token) = w.bettor(B).await;
    let key = Uuid::new_v4();
    let fight = i64::from(w.fight);
    let first = placed(&w.bet_with(&token, fight, "a", 25, key).await, 201);
    let written = w.written().await;
    let again = placed(&w.bet_with(&token, fight, "a", 25, key).await, 200);
    assert_eq!(again.mine.bets, first.mine.bets);
    assert_eq!(again.wallet.balance, first.wallet.balance);
    assert_eq!(w.written().await, written);
    refusal(
        &w.bet_with(&token, fight, "a", 30, key).await,
        409,
        "bet.key_reused",
    );
    assert_eq!(w.written().await, written);
}

/// RF25, one case per row, and a refused bet leaves nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_row_refuses_with_its_code_and_writes_nothing() {
    let w = world().await;
    let (_, token) = w.bettor(B).await;
    let before = w.written().await;
    // The body: every rejection is `bet.invalid_request`, without serde's text.
    let key = Uuid::new_v4();
    for body in [
        "not json".to_owned(),
        format!(r#"{{"fight_id":{},"side":"a","amount":25}}"#, w.fight),
        format!(
            r#"{{"fight_id":{},"side":"a","amount":25,"client_key":"{key}","user_id":"x"}}"#,
            w.fight
        ),
        format!(
            r#"{{"fight_id":{},"side":"c","amount":25,"client_key":"{key}"}}"#,
            w.fight
        ),
        format!(
            r#"{{"fight_id":{},"side":"a","amount":-25,"client_key":"{key}"}}"#,
            w.fight
        ),
        // A key that is not the canonical form of a UUID.
        format!(
            r#"{{"fight_id":{},"side":"a","amount":25,"client_key":"{}"}}"#,
            w.fight,
            key.to_string().to_uppercase()
        ),
        format!(
            r#"{{"fight_id":{},"side":"a","amount":25,"client_key":"{}"}}"#,
            w.fight,
            key.simple()
        ),
        format!(
            r#"{{"fight_id":{},"side":"a","amount":25,"client_key":"k"}}"#,
            w.fight
        ),
        // Over `MAX_FORM_BYTES`.
        format!(
            r#"{{"fight_id":{},"side":"a","amount":25,"client_key":"{key}","x":"{}"}}"#,
            w.fight,
            "x".repeat(5000)
        ),
    ] {
        refusal(&w.post_raw(&token, &body).await, 422, "bet.invalid_request");
    }
    // Row 4: a fight that does not exist, and one out of `i32`.
    refusal(
        &w.bet_with(&token, 999_999, "a", 25, key).await,
        409,
        "bet.window_closed",
    );
    refusal(
        &w.bet_with(&token, 2_147_483_648, "a", 25, key).await,
        409,
        "bet.window_closed",
    );
    // Row 5.
    refusal(&w.bet(&token, "a", 24).await, 422, "bet.below_minimum");
    // Row 10: more than the balance.
    let (_, poor) = w.bettor(30).await;
    placed(&w.bet(&poor, "a", 25).await, 201);
    refusal(
        &w.bet(&poor, "a", 25).await,
        409,
        "bet.insufficient_balance",
    );
    let after_poor = w.written().await;
    assert_eq!((after_poor.0, after_poor.2), (before.0 + 1, before.2 + 1));
    // Row 6 (RF26): never both sides.
    placed(&w.bet(&token, "a", 25).await, 201);
    refusal(&w.bet(&token, "b", 25).await, 409, "bet.side_locked");
    // Row 9 and test 24: in probation, no more than `1 B`, not even with several bets.
    placed(&w.bet(&token, "a", 50).await, 201);
    refusal(&w.bet(&token, "a", 26).await, 409, "bet.fight_cap_exceeded");
    placed(&w.bet(&token, "a", 25).await, 201);
    refusal(&w.bet(&token, "a", 25).await, 409, "bet.fight_cap_exceeded");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM bets")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(rows, 4);
    // Without a session, 401.
    let anonymous = http::send_body(
        w.app.addr,
        "POST",
        "/api/bets",
        &[("origin", ORIGIN), ("content-type", "application/json")],
        "{}",
    )
    .await;
    assert_eq!(anonymous.status, 401);
}

/// RF15 and rows 7 and 8: the eleventh bet of a user is `bet.too_many_bets`; a fight with
/// `BETS_MAX_PER_FIGHT` takes no more.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_caps_of_rf15() {
    let w = world().await;
    let (user, token) = w.bettor(1000).await;
    mature(&w.pool, user, 10).await;
    for _ in 0..10 {
        placed(&w.bet(&token, "a", 25).await, 201);
    }
    refusal(&w.bet(&token, "a", 25).await, 409, "bet.too_many_bets");
    let mine: MyFightBets =
        serde_json::from_slice(&w.mine(&token, &format!("?fight_id={}", w.fight)).await.body)
            .unwrap();
    assert_eq!(mine.max_stake.get(), 0);
    // Fill the fight up to its cap with someone else's rows (no ledger: only the count matters).
    let filler = bets::funded_user(&w.pool, 1).await;
    let missing = BETS_MAX_PER_FIGHT - 10;
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         SELECT gen_random_uuid(), $1, $2, 'b', 1, false, gen_random_uuid() \
         FROM generate_series(1, $3)",
    )
    .bind(w.fight)
    .bind(filler)
    .bind(missing)
    .execute(&w.pool)
    .await
    .unwrap();
    let (_, late) = w.bettor(B).await;
    refusal(&w.bet(&late, "a", 25).await, 409, "bet.pool_full");
}

/// Test 23: two bets at once under the cap that together pass it: exactly one goes in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_bets_at_once_never_pass_the_cap() {
    let w = Arc::new(world().await);
    let (user, token) = w.bettor(1000).await;
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let (w, token) = (w.clone(), token.clone());
        tasks.push(tokio::spawn(async move { w.bet(&token, "a", 60).await }));
    }
    let mut statuses = Vec::new();
    for task in tasks {
        statuses.push(task.await.unwrap().status.as_u16());
    }
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 409]);
    let staked: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(stake), 0)::bigint FROM bets WHERE user_id = $1")
            .bind(user)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert_eq!(staked, 60);
}

/// Test 24: 8 days old with 9 fights is still in probation, and refunds do not count; the tenth
/// fight ends it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nine_fights_and_refunds_are_still_probation() {
    let w = world().await;
    let (user, token) = w.bettor(1000).await;
    mature(&w.pool, user, 9).await;
    for k in 20..23 {
        let id = f::announce(
            &w.pool,
            T0 + k * f::CADENCE,
            &f::golden_spec("striker"),
            &f::golden_spec("tank"),
        )
        .await;
        settled_bet(&w.pool, id, user, "refunded").await;
    }
    refusal(
        &w.bet(&token, "a", B + 1).await,
        409,
        "bet.fight_cap_exceeded",
    );
    let id = f::announce(
        &w.pool,
        T0 + 30 * f::CADENCE,
        &f::golden_spec("striker"),
        &f::golden_spec("tank"),
    )
    .await;
    settled_bet(&w.pool, id, user, "won").await;
    placed(&w.bet(&token, "a", B + 1).await, 201);
}

/// Waits until one backend of this database waits on a lock.
async fn one_waiting(pool: &PgPool) {
    wait_for("a request waiting on a lock", || async {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        waiting >= 1
    })
    .await;
}

/// RF25, the clock: a bet whose user is held by another transaction while the window closes reads
/// the clock after the lock, and is `bet.window_closed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_clock_is_read_after_the_user_is_locked() {
    let w = Arc::new(world().await);
    let (user, token) = w.bettor(B).await;
    let mut holder = w.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(user)
        .execute(&mut *holder)
        .await
        .unwrap();
    let request = {
        let (w, token) = (w.clone(), token.clone());
        tokio::spawn(async move { w.bet(&token, "a", 25).await })
    };
    one_waiting(&w.pool).await;
    let (closes_at, _) = f::hours(T0);
    w.clock.set(closes_at);
    holder.commit().await.unwrap();
    refusal(&request.await.unwrap(), 409, "bet.window_closed");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM bets")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

/// Plan-b D1: with the fight held past `lock_timeout`, row 4 fails closed: 409, nothing written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_held_fight_fails_closed() {
    let w = Arc::new(world().await);
    let (_, token) = w.bettor(B).await;
    let before = w.written().await;
    let mut holder = w.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM fights WHERE id = $1 FOR UPDATE")
        .bind(w.fight)
        .execute(&mut *holder)
        .await
        .unwrap();
    let response = w.bet(&token, "a", 25).await;
    holder.rollback().await.unwrap();
    refusal(&response, 409, "bet.window_closed");
    assert_eq!(w.written().await, before);
}

/// Bet against close: a bet that saw the window open holds the fight `FOR SHARE` until it commits,
/// so the close waits for it, even with the clock past `closes_at`; after the close, none goes in.
/// No clock is compared: the fight's hours come from the manual clock, `created_at` from Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_bet_goes_in_after_the_close() {
    let w = world().await;
    let user = bets::funded_user(&w.pool, B).await;
    // A bet in flight: the window taken, as `POST /api/bets` takes it.
    let mut tx = w.pool.begin().await.unwrap();
    w.shared
        .lock_open_for_bet(&mut tx, w.fight, T0)
        .await
        .expect("the window is open");
    let (closes_at, _) = f::hours(T0);
    w.clock.set(closes_at);
    // The runner tries to close and waits on the bet's lock.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        f::row(&w.pool, w.fight).await.state,
        protocol::FightState::BettingOpen
    );
    let bet = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         VALUES ($1, $2, $3, 'a', 25, false, $4)",
    )
    .bind(bet)
    .bind(w.fight)
    .bind(user)
    .bind(Uuid::new_v4())
    .execute(&mut *tx)
    .await
    .unwrap();
    server::ledger::bet_stake(&mut tx, bet, w.fight, user, server::ledger::Coins(25))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // Now the close goes through, and after it nothing enters.
    f::wait_state(&w.pool, w.fight, protocol::FightState::BettingClosed).await;
    let (_, late) = w.bettor(B).await;
    refusal(&w.bet(&late, "a", 25).await, 409, "bet.window_closed");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM bets WHERE fight_id = $1")
        .bind(w.fight)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

/// RF33: only one's own bets, a canonical id, `max_stake` at 0 when nothing would be taken, and no
/// `ranked_profit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn my_bets_are_mine_only() {
    let w = world().await;
    let (_, alice) = w.bettor(B).await;
    let (_, bob) = w.bettor(B).await;
    placed(&w.bet(&alice, "a", 25).await, 201);
    let query = format!("?fight_id={}", w.fight);
    let response = w.mine(&bob, &query).await;
    assert_eq!(response.status, 200);
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    let theirs: MyFightBets = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(theirs.side, None);
    assert!(theirs.bets.is_empty());
    assert_eq!(theirs.staked.get(), 0);
    assert_eq!(theirs.max_stake.get(), B);
    let text = w.mine(&alice, &query).await.text();
    assert!(!text.contains("ranked"), "{text}");
    for bad in [
        "",
        "?fight_id=",
        "?fight_id=07",
        "?fight_id=x",
        "?fight_id=999999",
    ] {
        refusal(&w.mine(&alice, bad).await, 404, "fight.not_found");
    }
    // Below the minimum, nothing.
    let (_, broke) = w.bettor(24).await;
    let mine: MyFightBets = serde_json::from_slice(&w.mine(&broke, &query).await.body).unwrap();
    assert_eq!(mine.max_stake.get(), 0);
    // A closed window, nothing.
    let (closes_at, _) = f::hours(T0);
    w.clock.set(closes_at);
    f::wait_state(&w.pool, w.fight, protocol::FightState::BettingClosed).await;
    let mine: MyFightBets = serde_json::from_slice(&w.mine(&bob, &query).await.body).unwrap();
    assert_eq!(mine.max_stake.get(), 0);
}

/// Plan-b D6: `wallet` and `bets/mine` share one bucket per user (60/min).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wallet_and_my_bets_share_their_quota() {
    let w = world().await;
    let (_, token) = w.bettor(B).await;
    let cookie = support::oauth::session_cookie(&token);
    for _ in 0..60 {
        let response = http::send(
            w.app.addr,
            "GET",
            "/api/wallet",
            &[("cookie", cookie.as_str())],
        )
        .await;
        assert_eq!(response.status, 200);
    }
    let limited = w.mine(&token, &format!("?fight_id={}", w.fight)).await;
    assert_eq!(limited.status, 429, "{}", limited.text());
}

/// RF31: a ban committed while a bet waits on the user's lock stops it, with nothing written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ban_during_a_bet_stops_it() {
    let w = Arc::new(world().await);
    let (user, token) = w.bettor(B).await;
    let before = w.written().await;
    let mut ban = begin_privileged(&w.pool).await;
    sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1")
        .bind(user)
        .execute(&mut *ban)
        .await
        .unwrap();
    let request = {
        let (w, token) = (w.clone(), token.clone());
        tokio::spawn(async move { w.bet(&token, "a", 25).await })
    };
    one_waiting(&w.pool).await;
    ban.commit().await.unwrap();
    let response = request.await.unwrap();
    assert_eq!(response.status, 401, "{}", response.text());
    assert_eq!(w.written().await, before);
}

/// RF33: once the runner settles the fight, each one reads its own result, and the loser reads
/// what it lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn my_bets_carry_the_result_once_settled() {
    let w = world().await;
    let (alice, alice_token) = w.bettor(B).await;
    let (bob, bob_token) = w.bettor(B).await;
    placed(&w.bet(&alice_token, "a", 40).await, 201);
    placed(&w.bet(&bob_token, "b", 40).await, 201);
    f::drive_to_settled(&w.pool, &w.clock, w.fight).await;
    let query = format!("?fight_id={}", w.fight);
    let a: MyFightBets = serde_json::from_slice(&w.mine(&alice_token, &query).await.body).unwrap();
    let b: MyFightBets = serde_json::from_slice(&w.mine(&bob_token, &query).await.body).unwrap();
    let (won, lost) = if a.result.as_ref().unwrap().outcome == protocol::BetOutcome::Won {
        (a, b)
    } else {
        (b, a)
    };
    let won = won.result.unwrap();
    let lost = lost.result.unwrap();
    // `T = 80`, `r = 0`: the winner collects all of it.
    assert_eq!((won.payout.get(), won.net.get()), (80, 40));
    assert_eq!(lost.outcome, protocol::BetOutcome::Lost);
    assert_eq!((lost.payout.get(), lost.net.get()), (0, -40));
    assert_eq!(won.refund_reason, None);
    let total: i64 = server::ledger::balance(&w.pool, alice).await.unwrap().0
        + server::ledger::balance(&w.pool, bob).await.unwrap().0;
    assert_eq!(total, 2 * B);
}
