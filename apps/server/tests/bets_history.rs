//! 019: `GET /api/bets/history`. The fights and their results are sown straight in the database, so
//! the tests read the history of states a runner would take minutes to reach. Every test starts the
//! app, so all run multi-thread (see `TestDb`'s drop).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use protocol::{BetHistoryPage, BetOutcome, FightState, RefundReason, Side};
use server::app::Settings;
use server::bets::history::HISTORY_PAGE_SIZE;
use sqlx::PgPool;
use support::app::RunningApp;
use support::bets;
use support::db::{TestDb, begin_privileged};
use support::fights::{self as f, KO, T0, Vector};
use support::http;
use uuid::Uuid;

struct World {
    _db: TestDb,
    app: RunningApp,
    pool: PgPool,
    /// The next free slot of the schedule.
    slot: std::cell::Cell<i64>,
}

async fn world() -> World {
    let db = TestDb::create().await;
    let (app, pool, _) = f::start_app(&db, &[], &Settings::default()).await;
    World {
        _db: db,
        app,
        pool,
        slot: std::cell::Cell::new(1),
    }
}

/// A result as the hook would write it, for one user on one fight.
#[derive(Clone, Copy)]
struct Settled {
    outcome: &'static str,
    payout: i64,
    pool: &'static str,
}

const WON: Settled = Settled {
    outcome: "won",
    payout: 80,
    pool: "paid",
};
const LOST: Settled = Settled {
    outcome: "lost",
    payout: 0,
    pool: "paid",
};
const DRAW: Settled = Settled {
    outcome: "refunded",
    payout: 40,
    pool: "refund_draw",
};

impl World {
    async fn bettor(&self) -> (Uuid, String) {
        let user = bets::funded_user(&self.pool, 100).await;
        (user, support::app::session_for(&self.pool, user).await)
    }

    async fn history(&self, token: &str, query: &str) -> http::Response {
        let cookie = support::oauth::session_cookie(token);
        http::send(
            self.app.addr,
            "GET",
            &format!("/api/bets/history{query}"),
            &[("cookie", cookie.as_str())],
        )
        .await
    }

    async fn page(&self, token: &str, query: &str) -> BetHistoryPage {
        let response = self.history(token, query).await;
        assert_eq!(response.status, 200, "{}", response.text());
        assert_eq!(response.header("cache-control"), Some("private, no-store"));
        assert_eq!(response.header("vary"), Some("Cookie"));
        serde_json::from_slice(&response.body).unwrap()
    }

    /// A scheduled fight in the next slot.
    async fn fight(&self) -> i32 {
        let slot = self.slot.get();
        self.slot.set(slot + 1);
        f::announce(
            &self.pool,
            T0 + slot * f::CADENCE,
            &f::golden_spec("striker"),
            &f::golden_spec("tank"),
        )
        .await
    }

    /// Bets of `user` on `fight` written straight: the guard of the window is off for them.
    async fn bets(&self, fight: i32, user: Uuid, side: &str, stakes: &[i64]) -> Vec<Uuid> {
        let mut tx = begin_privileged(&self.pool).await;
        sqlx::query("ALTER TABLE bets DISABLE TRIGGER USER")
            .execute(&mut *tx)
            .await
            .unwrap();
        let mut ids = vec![];
        for stake in stakes {
            let bet = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
                 VALUES ($1, $2, $3, $4, $5, true, $6)",
            )
            .bind(bet)
            .bind(fight)
            .bind(user)
            .bind(side)
            .bind(stake)
            .bind(Uuid::new_v4())
            .execute(&mut *tx)
            .await
            .unwrap();
            ids.push(bet);
        }
        sqlx::query("ALTER TABLE bets ENABLE TRIGGER USER")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        ids
    }

    /// The result of each bet, split in proportion to its stake.
    async fn results(&self, fight: i32, user: Uuid, bets: &[(Uuid, i64)], result: Settled) {
        let total: i64 = bets.iter().map(|bet| bet.1).sum();
        for (bet, stake) in bets {
            let payout = result.payout * stake / total;
            let net = payout - stake;
            sqlx::query(
                "INSERT INTO bet_settlements \
                 (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
                 VALUES ($1, $2, $3, $4, $5, $6, $6)",
            )
            .bind(bet)
            .bind(fight)
            .bind(user)
            .bind(result.outcome)
            .bind(payout)
            .bind(net)
            .execute(&self.pool)
            .await
            .unwrap();
        }
    }

    /// `scheduled` → `settled` (the legal edge of a cancelled fight) and its pool. A cancelled fight
    /// never has bets in production: this is only the shortest legal way to a settled row, and the
    /// history reads the result from the rows, never from how the fight ended.
    async fn settle(&self, fight: i32, pool_outcome: &str) {
        sqlx::query(
            "UPDATE fights SET state = 'settled', settled_at = now(), \
             cancel_reason = 'missed_window' WHERE id = $1",
        )
        .bind(fight)
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pool_settlements (fight_id, outcome, pool_a, pool_b, rake_bp, rake, residue) \
             VALUES ($1, $2, 0, 0, 0, 0, 0)",
        )
        .bind(fight)
        .bind(pool_outcome)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// A settled fight with the bets of `user` and their result.
    async fn settled(&self, user: Uuid, side: &str, stakes: &[i64], result: Settled) -> i32 {
        let fight = self.fight().await;
        let ids = self.bets(fight, user, side, stakes).await;
        let pairs: Vec<(Uuid, i64)> = ids.into_iter().zip(stakes.iter().copied()).collect();
        self.settle(fight, result.pool).await;
        self.results(fight, user, &pairs, result).await;
        fight
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_session_it_is_401() {
    let w = world().await;
    let response = http::send(w.app.addr, "GET", "/api/bets/history", &[]).await;
    assert_eq!(response.status, 401, "{}", response.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_bets_the_list_is_empty() {
    let w = world().await;
    let (_, token) = w.bettor().await;
    let page = w.page(&token, "").await;
    assert!(page.items.is_empty());
    assert_eq!(page.next_before, None);
}

/// Won, lost and refunded, each with its sums, its side and its reason; nothing of `ranked_profit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_result_carries_its_sums() {
    let w = world().await;
    let (user, token) = w.bettor().await;
    let won = w.settled(user, "a", &[25, 15], WON).await;
    let lost = w.settled(user, "b", &[30], LOST).await;
    let draw = w.settled(user, "a", &[40], DRAW).await;
    let response = w.history(&token, "").await;
    assert!(!response.text().contains("ranked"), "{}", response.text());
    let page = w.page(&token, "").await;
    let ids: Vec<u32> = page.items.iter().map(|item| item.fight_id).collect();
    let id = |fight: i32| u32::try_from(fight).unwrap();
    assert_eq!(ids, vec![id(draw), id(lost), id(won)]);

    let [draw, lost, won] = &page.items[..] else {
        panic!("three items");
    };
    assert_eq!((won.side, won.staked.get(), won.bets), (Side::A, 40, 2));
    assert_eq!((won.a.as_str(), won.b.as_str()), ("striker", "tank"));
    assert!(won.settled);
    let result = won.result.as_ref().unwrap();
    assert_eq!(result.outcome, BetOutcome::Won);
    assert_eq!((result.payout.get(), result.net.get()), (80, 40));
    assert_eq!(result.refund_reason, None);

    let result = lost.result.as_ref().unwrap();
    assert_eq!((lost.side, lost.staked.get(), lost.bets), (Side::B, 30, 1));
    assert_eq!(result.outcome, BetOutcome::Lost);
    assert_eq!((result.payout.get(), result.net.get()), (0, -30));

    let result = draw.result.as_ref().unwrap();
    assert_eq!(result.outcome, BetOutcome::Refunded);
    assert_eq!((result.payout.get(), result.net.get()), (40, 0));
    assert_eq!(result.refund_reason, Some(RefundReason::Draw));
}

/// A fight that is not settled has no result, even with rows of `bet_settlements` written ahead of
/// time: the state is the second barrier (019 "Riesgos").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_fight_has_no_result() {
    let w = world().await;
    let (user, token) = w.bettor().await;
    let fight = f::open_vector(&w.pool, T0, &Vector::of(KO)).await;
    let bet = bets::seed_bet(&w.pool, fight, user, "a", 25).await;
    let (closes_at, live_at) = f::hours(T0);
    f::close_by_sql(&w.pool, fight, closes_at).await;
    f::live_by_sql(&w.pool, fight, live_at).await;
    assert_eq!(f::row(&w.pool, fight).await.state, FightState::Live);
    w.results(fight, user, &[(bet, 25)], WON).await;
    let page = w.page(&token, "").await;
    let [item] = &page.items[..] else {
        panic!("one item");
    };
    assert!(!item.settled);
    assert_eq!(item.result, None);
    assert_eq!(item.staked.get(), 25);
    let text = w.history(&token, "").await.text();
    for fragment in ["winner", "won", "payout"] {
        assert!(!text.contains(fragment), "{fragment}: {text}");
    }
}

/// A settled fight with bets and no result is a broken invariant: 503, never `null` in silence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settled_fight_without_result_fails_closed() {
    let w = world().await;
    let (user, token) = w.bettor().await;
    let fight = w.fight().await;
    w.bets(fight, user, "a", &[25]).await;
    w.settle(fight, "paid").await;
    let response = w.history(&token, "").await;
    assert_eq!(response.status, 503, "{}", response.text());
    assert_eq!(response.text(), r#"{"error":"bet.unavailable"}"#);
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
}

/// Pages of `HISTORY_PAGE_SIZE`, the highest id first, with `next_before` until the last one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pages_walk_the_whole_history() {
    let w = world().await;
    let (user, token) = w.bettor().await;
    let mut fights = vec![];
    for _ in 0..=HISTORY_PAGE_SIZE {
        fights.push(w.settled(user, "a", &[25], LOST).await);
    }
    fights.reverse();
    let first = w.page(&token, "").await;
    assert_eq!(first.items.len(), HISTORY_PAGE_SIZE);
    let last_shown = first.items.last().unwrap().fight_id;
    assert_eq!(first.next_before, Some(last_shown));
    let second = w.page(&token, &format!("?before={last_shown}")).await;
    assert_eq!(second.next_before, None);
    let all: Vec<i32> = first
        .items
        .iter()
        .chain(&second.items)
        .map(|item| i32::try_from(item.fight_id).unwrap())
        .collect();
    assert_eq!(all, fights);
}

/// Only one's own bets, and a `before` is only a cursor: a fight of someone else, or none at all,
/// is a 200 with what is one's own below it, never a 404 that would tell who bet where.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_history_is_mine_only() {
    let w = world().await;
    let (alice, alice_token) = w.bettor().await;
    let (bob, bob_token) = w.bettor().await;
    let bobs = w.settled(bob, "b", &[25], LOST).await;
    let alices = w.settled(alice, "a", &[30], WON).await;
    let page = w.page(&bob_token, "").await;
    let ids: Vec<u32> = page.items.iter().map(|item| item.fight_id).collect();
    assert_eq!(ids, vec![u32::try_from(bobs).unwrap()]);
    let page = w.page(&bob_token, &format!("?before={}", alices + 1)).await;
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].staked.get(), 25);
    let page = w.page(&alice_token, &format!("?before={alices}")).await;
    assert!(page.items.is_empty());
    let page = w.page(&alice_token, "?before=999999").await;
    assert_eq!(page.items.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_before_that_is_not_an_id_is_404() {
    let w = world().await;
    let (_, token) = w.bettor().await;
    for bad in [
        "?before=",
        "?before=07",
        "?before=x",
        "?before=-1",
        "?before=2147483648",
        "?before=1&before=2",
    ] {
        let response = w.history(&token, bad).await;
        assert_eq!(response.status, 404, "{bad}: {}", response.text());
        assert_eq!(response.text(), r#"{"error":"fight.not_found"}"#);
        assert_eq!(response.header("cache-control"), Some("private, no-store"));
    }
}
