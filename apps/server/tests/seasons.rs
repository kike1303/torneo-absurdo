//! 020, phase A: seasons, `season_stats` and the leaderboard (ADR-0008 D8, tests 25 and 27 on the
//! aggregate). Fights and their results are sown straight in the database, in months of the past, so
//! probation is decided at the end of each season. Every test starts the app or a database, so all
//! run multi-thread (see `TestDb`'s drop).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use protocol::Leaderboard;
use server::app::Settings;
use server::seasons::{self, SEASON_MIN_FIGHTS};
use sqlx::{PgPool, Row};
use support::app::RunningApp;
use support::db::{TestDb, begin_privileged};
use support::fights::{self as f, KO, Vector};
use support::http;
use uuid::Uuid;

/// 2026-09-10T00:00:00Z: fights of September 2026 open from here, one cadence apart.
const SEPTEMBER: i64 = 1_788_998_400_000;
const SEASON_SEPT: i32 = 202_609;
const SEASON_OCT: i32 = 202_610;

struct World {
    _db: TestDb,
    app: RunningApp,
    pool: PgPool,
    slot: std::sync::atomic::AtomicI64,
}

async fn world() -> World {
    let db = TestDb::create().await;
    let (app, pool) = start_quiet(&db, |_| async {}).await;
    World {
        _db: db,
        app,
        pool,
        slot: std::sync::atomic::AtomicI64::new(0),
    }
}

/// The app once the startup pass of its job has ended (TA-81): that pass reconciles the seasons,
/// and on a slow runner it could otherwise mark a fight after the test settles it and before the
/// test sums it. `before_wait` runs between the start and the wait (the regression test below).
async fn start_quiet<F, Fut>(db: &TestDb, before_wait: F) -> (RunningApp, PgPool)
where
    F: FnOnce(PgPool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let started = Arc::new(tokio::sync::Notify::new());
    let settings = Settings {
        jobs_started: server::jobs::StartupPassSignal(Some(started.clone())),
        ..Settings::default()
    };
    let (app, pool, _) = f::start_app(db, &[], &settings).await;
    before_wait(pool.clone()).await;
    tokio::time::timeout(std::time::Duration::from_secs(30), started.notified())
        .await
        .expect("the startup pass of the job never ended");
    (app, pool)
}

impl World {
    /// A user created on `created_at` (an SQL timestamp).
    async fn user(&self, created_at: &str) -> Uuid {
        let id = support::bets::user(&self.pool).await;
        sqlx::query("UPDATE users SET created_at = $2::timestamptz WHERE id = $1")
            .bind(id)
            .bind(created_at)
            .execute(&self.pool)
            .await
            .unwrap();
        id
    }

    /// A fight whose window closes at `closes_at` (ms), still scheduled: its results are sown first
    /// and [`World::settle`] comes after, as in production, where both are one transaction. The
    /// reconciliation of the app's job could otherwise mark it before its results exist.
    async fn fight_closing_at(&self, closes_at: i64) -> i32 {
        f::announce(
            &self.pool,
            closes_at - f::WINDOW,
            &f::golden_spec("striker"),
            &f::golden_spec("tank"),
        )
        .await
    }

    /// `scheduled` → `settled`, the legal edge of a cancelled fight: the season only reads the
    /// results, never how the fight ended.
    async fn settle(&self, fight: i32) {
        sqlx::query(
            "UPDATE fights SET state = 'settled', settled_at = now(), \
             cancel_reason = 'missed_window' WHERE id = $1",
        )
        .bind(fight)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// The next fight of September.
    async fn september_fight(&self) -> i32 {
        let k = self.slot.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.fight_closing_at(SEPTEMBER + k * f::CADENCE + f::WINDOW)
            .await
    }

    /// One bet of `user` on `fight` and its result, written straight.
    async fn result(&self, fight: i32, user: Uuid, outcome: &str, net: i64, ranked: i64) {
        let bet = Uuid::new_v4();
        let mut tx = begin_privileged(&self.pool).await;
        sqlx::query("ALTER TABLE bets DISABLE TRIGGER USER")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
             VALUES ($1, $2, $3, 'a', 100, true, $4)",
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
        let payout = if outcome == "lost" { 0 } else { 100 + net };
        sqlx::query(
            "INSERT INTO bet_settlements \
             (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(bet)
        .bind(fight)
        .bind(user)
        .bind(outcome)
        .bind(payout)
        .bind(net)
        .bind(ranked)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    /// `count` September fights where `user` won `ranked` each.
    async fn wins(&self, user: Uuid, count: i32, ranked: i64) {
        for _ in 0..count {
            let fight = self.september_fight().await;
            self.result(fight, user, "won", ranked, ranked).await;
            self.settle(fight).await;
        }
    }

    async fn stats(&self, season: i32, user: Uuid) -> Option<(i64, i32)> {
        sqlx::query(
            "SELECT ranked_profit, settled_fights FROM season_stats \
             WHERE season_id = $1 AND user_id = $2",
        )
        .bind(season)
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .map(|row| (row.get(0), row.get(1)))
    }

    async fn board(&self, query: &str) -> http::Response {
        http::get(self.app.addr, &format!("/api/leaderboard{query}")).await
    }

    async fn names(&self, season: i32) -> Vec<String> {
        let response = self.board(&format!("?season={season}")).await;
        assert_eq!(response.status, 200, "{}", response.text());
        let board: Leaderboard = serde_json::from_slice(&response.body).unwrap();
        board
            .entries
            .into_iter()
            .map(|entry| entry.display_name)
            .collect()
    }

    async fn name(&self, user: Uuid) -> String {
        sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

/// Test 25 on the aggregate: `season_stats` is the sum of the results that are not refunds, by the
/// season of their fight, and neither a second tally nor a reconciliation sums twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_aggregate_is_the_sum_of_the_results_once() {
    let w = world().await;
    let ana = w.user("2026-08-01").await;
    let bob = w.user("2026-08-01").await;
    let one = w.september_fight().await;
    w.result(one, ana, "won", 40, 30).await;
    w.result(one, bob, "lost", -100, -100).await;
    let two = w.september_fight().await;
    w.result(two, ana, "refunded", 0, 0).await;
    let three = w.september_fight().await;
    w.result(three, ana, "lost", -100, -100).await;
    w.result(three, ana, "lost", -100, -100).await;
    for fight in [one, two, three] {
        w.settle(fight).await;
    }

    let mut tx = w.pool.begin().await.unwrap();
    assert!(seasons::tally(&mut tx, one).await.unwrap());
    assert!(!seasons::tally(&mut tx, one).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(seasons::reconcile(&w.pool).await.unwrap(), 2);
    assert_eq!(seasons::reconcile(&w.pool).await.unwrap(), 0);

    // Two bets on one fight are one fight; a refund is none.
    assert_eq!(w.stats(SEASON_SEPT, ana).await, Some((30 - 200, 2)));
    assert_eq!(w.stats(SEASON_SEPT, bob).await, Some((-100, 1)));
    let mismatches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ( \
           SELECT t.season_id, b.user_id, sum(b.ranked_profit) AS p, count(DISTINCT b.fight_id) AS n \
           FROM bet_settlements b JOIN season_tallies t ON t.fight_id = b.fight_id \
           WHERE outcome <> 'refunded' GROUP BY 1, 2) AS want \
         FULL JOIN season_stats s ON s.season_id = want.season_id AND s.user_id = want.user_id \
         WHERE want.p IS DISTINCT FROM s.ranked_profit OR want.n IS DISTINCT FROM s.settled_fights",
    )
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(mismatches, 0);
}

/// The season of a fight is the month of its `closes_at` in UTC, whatever the `TimeZone` of the
/// session; a fight that closes before `ends_at` counts there even if it settles later (test 27).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_season_of_a_fight_is_its_utc_month() {
    let w = world().await;
    let ana = w.user("2026-08-01").await;
    // 2026-10-01T00:30:00Z is still September 30 in Mexico City.
    let october = w.fight_closing_at(1_790_814_600_000).await;
    // 2026-09-30T23:50:00Z.
    let september = w.fight_closing_at(1_790_812_200_000).await;
    w.result(october, ana, "won", 10, 10).await;
    w.result(september, ana, "won", 20, 20).await;
    w.settle(october).await;
    w.settle(september).await;
    let mut tx = w.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL TIME ZONE 'America/Mexico_City'")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(seasons::tally(&mut tx, october).await.unwrap());
    assert!(seasons::tally(&mut tx, september).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(w.stats(SEASON_OCT, ana).await, Some((10, 1)));
    assert_eq!(w.stats(SEASON_SEPT, ana).await, Some((20, 1)));
    let bounds: (String, String) = sqlx::query_as(
        "SELECT to_char(starts_at AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI'), \
                to_char(ends_at AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI') \
         FROM seasons WHERE id = $1",
    )
    .bind(SEASON_OCT)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(
        bounds,
        ("2026-10-01 00:00".to_owned(), "2026-11-01 00:00".to_owned())
    );
}

/// RF31 of 010: a closed season never makes the hook fail; it still gets the sum.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_season_does_not_fail_the_tally() {
    let w = world().await;
    let ana = w.user("2026-08-01").await;
    let first = w.september_fight().await;
    w.result(first, ana, "won", 5, 5).await;
    w.settle(first).await;
    seasons::reconcile(&w.pool).await.unwrap();
    let mut tx = begin_privileged(&w.pool).await;
    sqlx::query("UPDATE seasons SET closed_at = now() WHERE id = $1")
        .bind(SEASON_SEPT)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let late = w.september_fight().await;
    w.result(late, ana, "won", 7, 7).await;
    w.settle(late).await;
    let mut tx = w.pool.begin().await.unwrap();
    assert!(seasons::tally(&mut tx, late).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(w.stats(SEASON_SEPT, ana).await, Some((12, 2)));
}

/// The guards: only the CLI closes a season or excludes a user, a close is never rewritten, and the
/// upsert of the tally, which leaves `excluded_*` NULL, passes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_cli_closes_or_excludes() {
    let w = world().await;
    let ana = w.user("2026-08-01").await;
    let fight = w.september_fight().await;
    w.result(fight, ana, "won", 5, 5).await;
    w.settle(fight).await;
    seasons::reconcile(&w.pool).await.unwrap();
    let refused = |result: Result<sqlx::postgres::PgQueryResult, sqlx::Error>| {
        let error = result.unwrap_err();
        assert_eq!(server::db::error_kind(&error), "season_guard");
    };
    refused(
        sqlx::query("UPDATE seasons SET closed_at = now() WHERE id = $1")
            .bind(SEASON_SEPT)
            .execute(&w.pool)
            .await,
    );
    refused(
        sqlx::query(
            "UPDATE season_stats SET excluded_at = now(), excluded_by = $2, \
             excluded_reason = 'farm_signals' WHERE season_id = $1",
        )
        .bind(SEASON_SEPT)
        .bind(ana)
        .execute(&w.pool)
        .await,
    );
    refused(
        sqlx::query("DELETE FROM season_stats")
            .execute(&w.pool)
            .await,
    );
    let mut tx = begin_privileged(&w.pool).await;
    sqlx::query("UPDATE seasons SET closed_at = now() WHERE id = $1")
        .bind(SEASON_SEPT)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = begin_privileged(&w.pool).await;
    let again =
        sqlx::query("UPDATE seasons SET closed_at = now() + interval '1 day' WHERE id = $1")
            .bind(SEASON_SEPT)
            .execute(&mut *tx)
            .await;
    refused(again);
}

/// Two settlements with users in common, at the same time, never deadlock (020, security review 8).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_tallies_with_users_in_common_do_not_deadlock() {
    let w = Arc::new(world().await);
    let users = [
        w.user("2026-08-01").await,
        w.user("2026-08-01").await,
        w.user("2026-08-01").await,
    ];
    let one = w.september_fight().await;
    let two = w.september_fight().await;
    for user in users {
        w.result(one, user, "won", 1, 1).await;
        w.result(two, user, "lost", -100, -100).await;
    }
    w.settle(one).await;
    w.settle(two).await;
    let mut first = w.pool.begin().await.unwrap();
    seasons::tally(&mut first, one).await.unwrap();
    let second = {
        let w = w.clone();
        tokio::spawn(async move {
            let mut tx = w.pool.begin().await.unwrap();
            seasons::tally(&mut tx, two).await.unwrap();
            tx.commit().await.unwrap();
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    first.commit().await.unwrap();
    second.await.unwrap();
    for user in users {
        assert_eq!(w.stats(SEASON_SEPT, user).await, Some((-99, 2)));
    }
}

/// Test 27 and the owner's minimum: out of probation at the end of the season, with ten fights in
/// it, neither banned nor excluded; in order, with a deterministic tie-break and no user id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_leaderboard_shows_only_the_eligible_in_order() {
    let w = world().await;
    let top = w.user("2026-08-01").await;
    let second = w.user("2026-08-01").await;
    let nine = w.user("2026-08-01").await;
    let banned = w.user("2026-08-01").await;
    let excluded = w.user("2026-08-01").await;
    // Six days old when the season ends: still in probation then, whatever it is today.
    let young = w.user("2026-09-25").await;
    w.wins(top, SEASON_MIN_FIGHTS, 30).await;
    w.wins(second, SEASON_MIN_FIGHTS + 2, 10).await;
    w.wins(nine, SEASON_MIN_FIGHTS - 1, 100).await;
    w.wins(banned, SEASON_MIN_FIGHTS, 100).await;
    w.wins(excluded, SEASON_MIN_FIGHTS, 100).await;
    w.wins(young, SEASON_MIN_FIGHTS, 100).await;
    seasons::reconcile(&w.pool).await.unwrap();
    let mut tx = begin_privileged(&w.pool).await;
    sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1")
        .bind(banned)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE season_stats SET excluded_at = now(), excluded_by = $2, \
         excluded_reason = 'farm_signals' WHERE season_id = $3 AND user_id = $1",
    )
    .bind(excluded)
    .bind(top)
    .bind(SEASON_SEPT)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        w.names(SEASON_SEPT).await,
        vec![w.name(top).await, w.name(second).await]
    );
    let response = w.board(&format!("?season={SEASON_SEPT}")).await;
    assert_eq!(response.header("cache-control"), Some("no-store"));
    let text = response.text();
    for user in [top, second, nine, banned, excluded, young] {
        assert!(!text.contains(&user.to_string()), "{text}");
    }
    let board: Leaderboard = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(board.season.id, 202_609);
    assert!(!board.season.closed);
    assert_eq!(board.min_fights, 10);
    let first = &board.entries[0];
    assert_eq!(
        (first.rank, first.ranked_profit.get(), first.settled_fights),
        (1, 300, 10)
    );
}

/// Test 27: what an account earns in probation counts in full once it leaves it before the end of
/// the season, including the fights it won while still in probation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_probation_earned_appears_on_leaving_it() {
    let w = world().await;
    // Its first fights are on September 10: it is 4 days old, in probation. On October 1 it is 15.
    let late = w.user("2026-09-06").await;
    w.wins(late, SEASON_MIN_FIGHTS, 25).await;
    seasons::reconcile(&w.pool).await.unwrap();
    let response = w.board(&format!("?season={SEASON_SEPT}")).await;
    let board: Leaderboard = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(board.entries.len(), 1);
    assert_eq!(board.entries[0].display_name, w.name(late).await);
    assert_eq!(board.entries[0].ranked_profit.get(), 250);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seasons_are_listed_and_a_bad_one_is_404() {
    let w = world().await;
    let ana = w.user("2026-08-01").await;
    let fight = w.september_fight().await;
    w.result(fight, ana, "won", 5, 5).await;
    w.settle(fight).await;
    seasons::reconcile(&w.pool).await.unwrap();
    let listed = http::get(w.app.addr, "/api/seasons").await;
    assert_eq!(listed.status, 200, "{}", listed.text());
    assert_eq!(listed.header("cache-control"), Some("no-store"));
    let seasons: Vec<protocol::SeasonView> = serde_json::from_slice(&listed.body).unwrap();
    assert_eq!(
        seasons.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![202_609]
    );
    // Without `season`, this moment's season, even without a row: 200 and empty.
    let now = w.board("").await;
    assert_eq!(now.status, 200, "{}", now.text());
    let board: Leaderboard = serde_json::from_slice(&now.body).unwrap();
    assert!(board.entries.is_empty());
    for bad in [
        "?season=",
        "?season=202613",
        "?season=20261",
        "?season=x",
        "?season=202601&season=202602",
        "?season=203001",
    ] {
        let response = w.board(bad).await;
        assert_eq!(response.status, 404, "{bad}: {}", response.text());
        assert_eq!(response.text(), r#"{"error":"season.not_found"}"#);
    }
}

/// The real hook sums in the season: a fight settled by the runner, with a bet on each side.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_settlement_hook_tallies_the_fight() {
    let db = TestDb::create().await;
    let (fights, clock) = f::settings_at(f::T0);
    let settings = Settings {
        fights: server::fights::FightSettings {
            hook: Arc::new(server::bets::BetsHook),
            ..fights
        },
        ..Settings::default()
    };
    let (_app, pool, shared) = f::start_app(&db, &[], &settings).await;
    let fight = f::open_vector(&pool, f::T0, &Vector::of(KO)).await;
    let ana = support::bets::funded_user(&pool, 100).await;
    let bob = support::bets::funded_user(&pool, 100).await;
    support::bets::seed_bet(&pool, fight, ana, "a", 40).await;
    support::bets::seed_bet(&pool, fight, bob, "b", 40).await;
    let _runner = f::spawn_runner(
        &pool,
        &shared,
        f::config_without_roster(),
        settings.fights.clone(),
    );
    f::drive_to_settled(&pool, &clock, fight).await;
    let marked: i64 = sqlx::query_scalar("SELECT count(*) FROM season_tallies WHERE fight_id = $1")
        .bind(fight)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(marked, 1);
    let total: (i64, i64) = sqlx::query_as(
        "SELECT sum(ranked_profit)::bigint, sum(settled_fights)::bigint FROM season_stats",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // A winner and a loser of 40 each, outside probation or not: the loser counts -40 in full.
    assert_eq!(total.1, 2);
    assert!(total.0 <= 0, "{total:?}");
    assert_eq!(seasons::reconcile(&pool).await.unwrap(), 0);
}

/// TA-81: a startup pass of the job that ends late (here held back by a lock on `auth_events`, the
/// first table it touches) still ends before the world sows its fights, so the test's own tally is
/// the one that sums them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_startup_pass_ends_before_the_fights_are_sown() {
    let db = TestDb::create().await;
    let held = db.migrated_pool().await;
    let mut hold = held.begin().await.unwrap();
    sqlx::query("LOCK TABLE auth_events IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *hold)
        .await
        .unwrap();
    let (app, pool) = start_quiet(&db, |_| async move {
        // Let the pass reach the lock before it is released.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        hold.commit().await.unwrap();
    })
    .await;
    let w = World {
        _db: db,
        app,
        pool,
        slot: std::sync::atomic::AtomicI64::new(0),
    };
    let ana = w.user("2026-08-01").await;
    let fight = w.september_fight().await;
    w.result(fight, ana, "won", 10, 10).await;
    w.settle(fight).await;
    let mut tx = w.pool.begin().await.unwrap();
    assert!(seasons::tally(&mut tx, fight).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(w.stats(SEASON_SEPT, ana).await, Some((10, 1)));
}
