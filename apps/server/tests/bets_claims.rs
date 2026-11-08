//! 012 RF29 to RF32: the daily grant, the bailout and the wallet. Tests 7, 8 and 26 call the claims
//! with their transaction, without the limiter; the rest goes through HTTP. Every test starts the
//! app, so all run multi-thread (see `TestDb`'s drop).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use protocol::{BailoutErrorCode, BonusErrorCode, ErrorCode, FightFinal, Side, Wallet};
use server::app::{AppState, Settings};
use server::bets::BetsHook;
use server::bets::claims::{self, Claim};
use server::fights::hook::{SettlementHook, SettlingFight};
use server::ledger::{self, Coins};
use sqlx::PgPool;
use support::app::{ORIGIN, RunningApp};
use support::bets;
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector};
use support::http;
use uuid::Uuid;

/// `DAILY_GRANT` of the test config: `B = 100`, `R = 50`, `m = 25`.
const B: i64 = 100;
const R: i64 = 50;

async fn app(db: &TestDb) -> (RunningApp, PgPool, AppState) {
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let pool = db.migrated_pool().await;
    let state = AppState::from_config(
        &config,
        &Settings::default(),
        pool.clone(),
        Arc::new(support::app::AlwaysUp),
    )
    .unwrap();
    (support::app::start(state.clone()).await, pool, state)
}

async fn post(app: &RunningApp, path: &str, token: &str) -> http::Response {
    let cookie = support::oauth::session_cookie(token);
    http::send(
        app.addr,
        "POST",
        path,
        &[("origin", ORIGIN), ("cookie", cookie.as_str())],
    )
    .await
}

async fn get(app: &RunningApp, path: &str, token: &str) -> http::Response {
    let cookie = support::oauth::session_cookie(token);
    http::send(app.addr, "GET", path, &[("cookie", cookie.as_str())]).await
}

fn wallet(response: &http::Response) -> Wallet {
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    assert_eq!(response.header("vary"), Some("Cookie"));
    serde_json::from_slice(&response.body).unwrap()
}

fn refusal(response: &http::Response, status: u16, code: &str) {
    assert_eq!(response.status, status, "{}", response.text());
    assert_eq!(response.text(), format!(r#"{{"error":"{code}"}}"#));
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
}

async fn events(pool: &PgPool, user: Uuid, kind: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM auth_events WHERE user_id = $1 AND kind = $2 \
         AND ledger_entry_id IS NOT NULL",
    )
    .bind(user)
    .bind(kind)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A user with a session and no grant yet; or with today's grant of `coins`.
async fn session_user(pool: &PgPool, grant: Option<i64>) -> (Uuid, String) {
    let user = match grant {
        None => bets::user(pool).await,
        Some(coins) => bets::funded_user(pool, coins).await,
    };
    (user, support::app::session_for(pool, user).await)
}

/// RF29: the first claim gives `B` with its `auth_events`; the second, 409. The wallet says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bonus_is_claimed_once_a_day() {
    let db = TestDb::create().await;
    let (app, pool, _) = app(&db).await;
    let (user, token) = session_user(&pool, None).await;
    let before = wallet(&get(&app, "/api/wallet", &token).await);
    assert!(before.bonus_available);
    let after = wallet(&post(&app, "/api/bonus/claim", &token).await);
    assert_eq!(after.balance.get(), B);
    assert!(!after.bonus_available);
    assert_eq!(events(&pool, user, "grant").await, 1);
    refusal(
        &post(&app, "/api/bonus/claim", &token).await,
        409,
        BonusErrorCode::AlreadyClaimed.as_str(),
    );
    assert_eq!(ledger::balance(&pool, user).await.unwrap(), Coins(B));
}

/// RF30, through HTTP: grant first, then broke, then once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bailout_follows_its_order() {
    let db = TestDb::create().await;
    let (app, pool, _) = app(&db).await;
    let (_, no_grant) = session_user(&pool, None).await;
    refusal(
        &post(&app, "/api/bailout/claim", &no_grant).await,
        409,
        BailoutErrorCode::GrantAvailable.as_str(),
    );
    let (_, rich) = session_user(&pool, Some(25)).await;
    refusal(
        &post(&app, "/api/bailout/claim", &rich).await,
        409,
        BailoutErrorCode::NotEligible.as_str(),
    );
    let (user, broke) = session_user(&pool, Some(24)).await;
    assert_eq!(
        wallet(&get(&app, "/api/wallet", &broke).await).bailout,
        protocol::BailoutState::Available
    );
    let after = wallet(&post(&app, "/api/bailout/claim", &broke).await);
    assert_eq!(after.balance.get(), 24 + R);
    assert_eq!(after.bailout, protocol::BailoutState::NotEligible);
    assert_eq!(events(&pool, user, "bailout").await, 1);
    // Spent again, it is already claimed today: the user loses all of it in a fight the runner
    // settles, so nothing is open.
    let vector = Vector::of(KO);
    let loser = match vector.log.events.last() {
        Some(protocol::FightEvent::End {
            winner: Some(Side::A),
            ..
        }) => "b",
        Some(protocol::FightEvent::End {
            winner: Some(Side::B),
            ..
        }) => "a",
        other => panic!("the vector of a KO has a winner: {other:?}"),
    };
    let winner = if loser == "a" { "b" } else { "a" };
    let id = f::open_vector(&pool, T0, &vector).await;
    let (settings, clock) = f::settings_at(T0);
    let settings = server::fights::FightSettings {
        hook: Arc::new(BetsHook),
        ..settings
    };
    let (_shared, _runner) = f::start(&pool, &settings);
    bets::seed_bet(&pool, id, user, loser, 74).await;
    let other = bets::funded_user(&pool, 1000).await;
    bets::seed_bet(&pool, id, other, winner, 10).await;
    f::drive_to_settled(&pool, &clock, id).await;
    assert_eq!(ledger::balance(&pool, user).await.unwrap(), Coins(0));
    refusal(
        &post(&app, "/api/bailout/claim", &broke).await,
        409,
        BailoutErrorCode::AlreadyClaimed.as_str(),
    );
}

/// RF32: the shape of a new account: probation, its cap, the amounts and the reset.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_wallet_of_a_new_account() {
    let db = TestDb::create().await;
    let (app, pool, _) = app(&db).await;
    let (_, token) = session_user(&pool, Some(B)).await;
    let wallet = wallet(&get(&app, "/api/wallet", &token).await);
    assert_eq!(wallet.balance.get(), B);
    assert_eq!(wallet.open_stakes.get(), 0);
    assert_eq!(
        (
            wallet.bonus_amount.get(),
            wallet.bailout_amount.get(),
            wallet.min_bet.get()
        ),
        (B, R, 25)
    );
    // In probation: the cap is `1 B`.
    assert_eq!(wallet.fight_cap.get(), B);
    let probation = wallet.probation.unwrap();
    assert_eq!((probation.days_left, probation.fights_left), (7, 10));
    let resets = wallet.day_resets_at.as_ms().unwrap();
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    assert!(resets > now && resets <= now + 86_400_000, "{resets}");
    assert_eq!(resets % 86_400_000, 0);
}

/// RF29 to RF32: without a session, 401, and nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_session_nothing_is_claimed() {
    let db = TestDb::create().await;
    let (app, pool, _) = app(&db).await;
    for (method, path) in [
        ("POST", "/api/bonus/claim"),
        ("POST", "/api/bailout/claim"),
        ("GET", "/api/wallet"),
    ] {
        let response = http::send(app.addr, method, path, &[("origin", ORIGIN)]).await;
        assert_eq!(response.status, 401, "{path}");
    }
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entries, 0);
}

/// Plan-b D6: `bonus` and `bailout` share one bucket per user (5/min).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bonus_and_bailout_share_their_quota() {
    let db = TestDb::create().await;
    let (app, pool, _) = app(&db).await;
    let (_, token) = session_user(&pool, Some(B)).await;
    for _ in 0..5 {
        assert_eq!(post(&app, "/api/bonus/claim", &token).await.status, 409);
    }
    let limited = post(&app, "/api/bailout/claim", &token).await;
    assert_eq!(limited.status, 429, "{}", limited.text());
    assert_eq!(limited.header("cache-control"), Some("private, no-store"));
}

/// Test 7: 20 claims at once give exactly one `grant`, and exactly one `bailout`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_claims_give_exactly_one_entry() {
    let db = TestDb::create().await;
    let (_app, pool, state) = app(&db).await;
    let fresh = bets::user(&pool).await;
    let broke = bets::funded_user(&pool, 10).await;
    for (user, bailout) in [(fresh, false), (broke, true)] {
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (pool, state) = (pool.clone(), state.clone());
            tasks.push(tokio::spawn(async move {
                let mut tx = pool.begin().await.unwrap();
                let claim = if bailout {
                    claims::claim_bailout(&mut tx, &state, user, &[0; 32]).await
                } else {
                    claims::claim_bonus(&mut tx, &state, user, &[0; 32]).await
                };
                // A lock that waited past `lock_timeout` is a failure, never a second entry.
                if let Ok(Claim::Done(_)) = claim {
                    tx.commit().await.unwrap();
                    true
                } else {
                    false
                }
            }));
        }
        let mut done = 0;
        for task in tasks {
            done += usize::from(task.await.unwrap());
        }
        assert_eq!(done, 1, "bailout: {bailout}");
        let kind = if bailout { "bailout" } else { "grant" };
        let entries: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ledger_entries WHERE user_id = $1 AND kind = $2",
        )
        .bind(user)
        .bind(kind)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(entries, 1, "{kind}");
    }
}

async fn bailout_of(pool: &PgPool, state: &AppState, user: Uuid) -> Claim {
    let mut tx = pool.begin().await.unwrap();
    let claim = claims::claim_bailout(&mut tx, state, user, &[0; 32])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    claim
}

/// Test 8: an open bet that leaves the balance at 0 is not being broke. Test 26: without today's
/// grant, the grant comes first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_bet_is_not_being_broke_and_the_grant_comes_first() {
    let db = TestDb::create().await;
    let (_app, pool, state) = app(&db).await;
    let id = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    let user = bets::funded_user(&pool, 100).await;
    bets::seed_bet(&pool, id, user, "a", 100).await;
    assert_eq!(ledger::balance(&pool, user).await.unwrap(), Coins(0));
    assert_eq!(
        bailout_of(&pool, &state, user).await,
        Claim::Refused(ErrorCode::Bailout(BailoutErrorCode::NotEligible))
    );
    let new = bets::user(&pool).await;
    assert_eq!(
        bailout_of(&pool, &state, new).await,
        Claim::Refused(ErrorCode::Bailout(BailoutErrorCode::GrantAvailable))
    );
}

/// RF30, bailout against a settlement, on both sides of its `COMMIT`: never granted with a refund on
/// its way. The claim waits on the settlement's key lock of the user and then sees it all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bailout_never_passes_a_refund_on_its_way() {
    let db = TestDb::create().await;
    let (_app, pool, state) = app(&db).await;
    let id = f::open_vector(&pool, T0, &Vector::of(KO)).await;
    let user = bets::funded_user(&pool, 25).await;
    bets::seed_bet(&pool, id, user, "a", 25).await;
    // The refund, uncommitted.
    let mut settling = pool.begin().await.unwrap();
    BetsHook
        .settle(
            &mut settling,
            &SettlingFight {
                id,
                final_result: FightFinal::Voided {
                    reason: protocol::VoidReason::EngineError,
                },
                closes_at: T0,
            },
        )
        .await
        .unwrap();
    let claim = {
        let (pool, state) = (pool.clone(), state.clone());
        tokio::spawn(async move { bailout_of(&pool, &state, user).await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    settling.commit().await.unwrap();
    assert_eq!(
        claim.await.unwrap(),
        Claim::Refused(ErrorCode::Bailout(BailoutErrorCode::NotEligible))
    );
    // After the `COMMIT`: the refund is in the balance.
    assert_eq!(
        bailout_of(&pool, &state, user).await,
        Claim::Refused(ErrorCode::Bailout(BailoutErrorCode::NotEligible))
    );
}
