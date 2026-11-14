//! 021 phase 4 (TA-64): the PvP ranking. The leaderboard of the current season is read live (rated
//! fights of the month, out of bans and exclusions); the close of a season writes the snapshot of
//! its ratings and the soft reset, exact even when it comes late, and only once; the tables of the
//! ranking only grow, each from its own writer; and `server pvp farm-report|exclude` review and
//! exclude without printing a byte of an `ip_hash`. Fights are sown straight in the database, the
//! past ones with the guard of `pvp_fights` off (it stamps the month of `now()`). The jobs are off:
//! no close runs behind the back of a test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use base64::Engine as _;
use serde_json::Value;
use server::admin::pvp::{PvpAction, PvpCommand, execute_pvp};
use server::admin::season::{ExcludeReason, SEASON_DEADLINE, SeasonFailure};
use server::app::Settings;
use server::jobs::Jobs;
use server::pvp::rewards::soft_reset;
use server::pvp::season;
use server::rate::{RateSettings, ServerClock};
use sqlx::{PgPool, Row};
use support::app::{ORIGIN, RunningApp};
use support::db::{TestDb, begin_privileged, privileged};
use support::http;
use uuid::Uuid;

const SEPTEMBER: i32 = 202_609;
const OCTOBER: i32 = 202_610;

fn content_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/characters").to_owned()
}

async fn app(db: &TestDb) -> (RunningApp, PgPool, ServerClock) {
    let clock = ServerClock::manual();
    let settings = Settings {
        rate: RateSettings {
            clock: clock.clone(),
            ..RateSettings::default()
        },
        jobs: Jobs::Off,
        ..Settings::default()
    };
    let dir = content_dir();
    let (app, pool) = support::app::start_db(db, &[("CONTENT_DIR", dir.as_str())], &settings).await;
    (app, pool, clock)
}

async fn get(app: &RunningApp, clock: &ServerClock, path: &str) -> http::Response {
    clock.advance(Duration::from_secs(60));
    http::send(app.addr, "GET", path, &[("origin", ORIGIN)]).await
}

fn body(response: &http::Response) -> Value {
    serde_json::from_str(&response.text()).unwrap()
}

/// A user with the botarga of a new user, named `Test<id>`.
async fn player(pool: &PgPool) -> Uuid {
    let user = support::bets::user(pool).await;
    let catalog = server::botarga::BotargaCatalog::embedded().unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        server::botarga::store::create(&mut tx, user, catalog.starter())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    user
}

fn display_name(user: Uuid) -> String {
    format!("Test{}", user.simple())
}

async fn set_rating(pool: &PgPool, user: Uuid, rating: i32) {
    sqlx::query("UPDATE botargas SET rating = $2 WHERE user_id = $1")
        .bind(user)
        .bind(rating)
        .execute(pool)
        .await
        .unwrap();
}

async fn rating(pool: &PgPool, user: Uuid) -> i32 {
    sqlx::query_scalar("SELECT rating FROM botargas WHERE user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `n` fights of `attacker` against `defender`, won by `winner` (`'a'`, `'b'` or a draw), each
/// moving the attacker `delta` and the defender `-delta`. In the current month with `season: None`; in the 15th of
/// `season` otherwise, with the guard of the table off for that transaction.
#[allow(clippy::too_many_arguments)]
async fn sow(
    pool: &PgPool,
    season: Option<i32>,
    attacker: Uuid,
    defender: Uuid,
    winner: Option<&str>,
    delta: i16,
    rated: bool,
    n: i32,
) {
    let mut tx = pool.begin().await.unwrap();
    if season.is_some() {
        sqlx::query("ALTER TABLE pvp_fights DISABLE TRIGGER pvp_fights_guard")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO pvp_fights (attacker_id, defender_id, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, rated, defender_rating_delta, season_id, created_at) \
         SELECT $1, $2, gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
         decode(repeat('00', 32), 'hex'), '\\x7b7d', $3, \
         CASE WHEN $3::text IS NULL THEN 'draw' ELSE 'ko' END, $4, 0, 0, 0, 1, $5, -$4, \
         COALESCE($6, 0), COALESCE(make_timestamptz($6 / 100, $6 % 100, 15, 12, 0, 0, 'UTC'), now()) \
         FROM generate_series(1, $7)",
    )
    .bind(attacker)
    .bind(defender)
    .bind(winner)
    .bind(delta)
    .bind(rated)
    .bind(season)
    .bind(n)
    .execute(&mut *tx)
    .await
    .unwrap();
    if season.is_some() {
        sqlx::query("ALTER TABLE pvp_fights ENABLE TRIGGER pvp_fights_guard")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
}

async fn admin(pool: &PgPool) -> Uuid {
    let admin = support::bets::user(pool).await;
    privileged(
        pool,
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
    )
    .await;
    admin
}

async fn pvp_cli(
    db: &TestDb,
    season: i32,
    by: Uuid,
    action: PvpAction,
) -> Result<String, SeasonFailure> {
    execute_pvp(
        &db.options(),
        &PvpCommand { season, by, action },
        SEASON_DEADLINE,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_live_leaderboard_ranks_the_rated_fights_of_the_month() {
    let db = TestDb::create().await;
    let (app, pool, clock) = app(&db).await;
    let pool = &pool;
    let [a, b, c, d, e, f] = [
        player(pool).await,
        player(pool).await,
        player(pool).await,
        player(pool).await,
        player(pool).await,
        player(pool).await,
    ];
    for (user, r) in [(a, 1300), (b, 1100), (c, 1200), (d, 1500), (e, 1400)] {
        set_rating(pool, user, r).await;
    }
    sow(pool, None, a, f, Some("a"), 10, true, 10).await;
    sow(pool, None, a, f, None, 0, true, 1).await;
    // What is not rated does not count.
    sow(pool, None, a, f, Some("a"), 0, false, 5).await;
    sow(pool, None, b, f, Some("b"), -10, true, 10).await;
    // Nine are one short.
    sow(pool, None, c, f, Some("a"), 10, true, 9).await;
    sow(pool, None, d, f, Some("a"), 10, true, 10).await;
    sow(pool, None, e, f, Some("a"), 10, true, 10).await;
    privileged(
        pool,
        sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1").bind(d),
    )
    .await;
    let by = admin(pool).await;
    let current = season::current(&mut pool.acquire().await.unwrap())
        .await
        .unwrap();
    let current_id = i32::try_from(current.id).unwrap();
    let excluded = pvp_cli(
        &db,
        current_id,
        by,
        PvpAction::Exclude {
            user: e,
            reason: ExcludeReason::FarmSignals,
        },
    )
    .await;
    assert_eq!(excluded.as_deref(), Ok("pvp.exclude_applied"));

    let entry = |rank: u16, user: Uuid, rating: i32, n: u32, w: u32, l: u32| {
        serde_json::json!({
            "rank": rank, "display_name": display_name(user), "rating": rating,
            "rated_fights": n, "wins": w, "losses": l,
        })
    };
    let expected = serde_json::json!([
        entry(1, a, 1300, 11, 10, 0),
        entry(2, b, 1100, 10, 0, 10),
        entry(3, f, 1000, 50, 10, 39),
    ]);
    for path in [
        "/api/pvp/leaderboard".to_owned(),
        format!("/api/pvp/leaderboard?season={current_id}"),
    ] {
        let response = get(&app, &clock, &path).await;
        assert_eq!(response.status, 200, "{}", response.text());
        assert_eq!(response.header("cache-control"), Some("no-store"));
        let board = body(&response);
        assert_eq!(board["entries"], expected);
        assert_eq!(board["min_fights"], 10);
        assert_eq!(board["season"]["id"], current_id);
        assert_eq!(board["season"]["closed"], false);
        // Never an id.
        for user in [a, b, c, d, e, f] {
            assert!(!response.text().contains(&user.to_string()));
        }
    }
    // The live board is served from memory for a while: a tenth fight of `c` does not show yet.
    sow(pool, None, c, f, Some("a"), 10, true, 1).await;
    let board = body(&get(&app, &clock, "/api/pvp/leaderboard").await);
    assert_eq!(board["entries"], expected);
    // A month that is neither the current one nor closed, and what is not a month.
    for season in ["202001", "202613", "abc", "202611&season=202611"] {
        let response = get(
            &app,
            &clock,
            &format!("/api/pvp/leaderboard?season={season}"),
        )
        .await;
        assert_eq!(response.status, 404, "{season}");
        assert_eq!(response.text(), r#"{"error":"season.not_found"}"#);
    }
    let seasons = body(&get(&app, &clock, "/api/pvp/seasons").await);
    assert_eq!(seasons.as_array().unwrap().len(), 1);
    assert_eq!(seasons[0]["id"], current_id);
    assert_eq!(seasons[0]["closed"], false);
}

/// The world of the close: September and October have ended, with rated fights, and the current
/// month has one more. The ratings of `botargas` carry all of them.
struct Closing {
    a: Uuid,
    b: Uuid,
    c: Uuid,
}

async fn closing(pool: &PgPool) -> Closing {
    let (a, b, c) = (player(pool).await, player(pool).await, player(pool).await);
    // September: `a` beats `b` twelve times (+10 each) and `c` loses once against `a` (−5).
    sow(pool, Some(SEPTEMBER), a, b, Some("a"), 10, true, 12).await;
    sow(pool, Some(SEPTEMBER), c, a, Some("b"), -5, true, 1).await;
    // A fight that is not rated moves nothing and counts nowhere.
    sow(pool, Some(SEPTEMBER), c, a, Some("a"), 0, false, 3).await;
    // October: `b` beats `a` once (+20). This month: `a` beats `c` (+8).
    sow(pool, Some(OCTOBER), b, a, Some("a"), 20, true, 1).await;
    sow(pool, None, a, c, Some("a"), 8, true, 1).await;
    set_rating(pool, a, 1000 + 120 + 5 - 20 + 8).await;
    set_rating(pool, b, 1000 - 120 + 20).await;
    set_rating(pool, c, 1000 - 5 - 8).await;
    Closing { a, b, c }
}

async fn snapshot(pool: &PgPool, season: i32) -> Vec<(Uuid, i32, i32, i32, i32)> {
    sqlx::query(
        "SELECT user_id, rating, rated_fights, wins, losses FROM pvp_season_ratings \
         WHERE season_id = $1 ORDER BY rating DESC",
    )
    .bind(season)
    .fetch_all(pool)
    .await
    .unwrap()
    .iter()
    .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)))
    .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_late_close_snapshots_each_month_and_resets_once() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let pool = &pool;
    let Closing { a, b, c } = closing(pool).await;

    assert_eq!(season::close_due(pool).await.unwrap(), 2);
    // September, before its reset: each rating minus what October and this month moved.
    assert_eq!(
        snapshot(pool, SEPTEMBER).await,
        [(a, 1125, 13, 13, 0), (c, 995, 1, 0, 1), (b, 880, 12, 0, 12),]
    );
    // October, from the September reset with October on top: only `a` and `b` played it.
    let a_sep = soft_reset(1125) - 20 + 8;
    let b_sep = soft_reset(880) + 20;
    let c_sep = soft_reset(995) - 8;
    assert_eq!(
        snapshot(pool, OCTOBER).await,
        [(a, a_sep - 8, 1, 0, 1), (b, b_sep, 1, 1, 0)]
    );
    // And now, both resets with this month on top.
    assert_eq!(rating(pool, a).await, soft_reset(a_sep - 8) + 8);
    assert_eq!(rating(pool, b).await, soft_reset(b_sep));
    assert_eq!(rating(pool, c).await, soft_reset(c_sep + 8) - 8);
    assert_eq!(
        (
            rating(pool, a).await,
            rating(pool, b).await,
            rating(pool, c).await
        ),
        (1029, 980, 991)
    );

    // Once: a second pass finds nothing, and changes nothing.
    assert_eq!(season::close_due(pool).await.unwrap(), 0);
    assert_eq!(rating(pool, a).await, 1029);
    let rows: Vec<(i32, bool)> =
        sqlx::query("SELECT id, closed_at >= ends_at FROM pvp_seasons ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();
    assert_eq!(rows, [(SEPTEMBER, true), (OCTOBER, true)]);
    // The current month is never closed by the job.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pvp_seasons WHERE ends_at > now()")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );

    // The board of a closed month is its snapshot, with the minimum of fights.
    let (app, _, clock) = app(&db).await;
    let board = body(
        &get(
            &app,
            &clock,
            &format!("/api/pvp/leaderboard?season={SEPTEMBER}"),
        )
        .await,
    );
    assert_eq!(board["season"]["closed"], true);
    let names: Vec<&str> = board["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["display_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, [display_name(a), display_name(b)]);
    assert_eq!(board["entries"][0]["rating"], 1125);
    let seasons = body(&get(&app, &clock, "/api/pvp/seasons").await);
    let ids: Vec<i64> = seasons
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids[1..], [i64::from(OCTOBER), i64::from(SEPTEMBER)]);
    assert!(ids[0] > i64::from(OCTOBER));
}

#[tokio::test]
async fn the_tables_of_the_ranking_only_grow_from_their_writer() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let pool = &pool;
    let Closing { a, b, .. } = closing(pool).await;
    season::close_due(pool).await.unwrap();
    let refused = |sql: &'static str| async move {
        let error = sqlx::query(sql).execute(pool).await.unwrap_err();
        sqlstate(&error)
    };
    let ta013 = Some("TA013".to_owned());
    // A closed season and its snapshot never change, and nothing new joins them later.
    assert_eq!(
        refused("UPDATE pvp_seasons SET closed_at = now()").await,
        ta013
    );
    assert_eq!(refused("DELETE FROM pvp_seasons").await, ta013);
    assert_eq!(refused("TRUNCATE pvp_seasons CASCADE").await, ta013);
    assert_eq!(
        refused("UPDATE pvp_season_ratings SET rating = 4000").await,
        ta013
    );
    assert_eq!(refused("DELETE FROM pvp_season_ratings").await, ta013);
    let late = sqlx::query(
        "INSERT INTO pvp_season_ratings (season_id, user_id, rating, rated_fights, wins, losses) \
         VALUES ($1, $2, 1000, 1, 0, 0)",
    )
    .bind(SEPTEMBER)
    .bind(Uuid::nil())
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&late), ta013);
    // A month that has not ended is not closed.
    assert_eq!(
        refused(
            "INSERT INTO pvp_seasons (id, starts_at, ends_at) \
             SELECT 209912, '2099-12-01T00:00:00Z', '2100-01-01T00:00:00Z'"
        )
        .await,
        ta013
    );
    // An exclusion only from the CLI.
    let mut tx = pool.begin().await.unwrap();
    let error = sqlx::query(
        "INSERT INTO pvp_exclusions (season_id, user_id, excluded_by, reason) \
         VALUES ($1, $2, $3, 'farm_signals')",
    )
    .bind(SEPTEMBER)
    .bind(a)
    .bind(b)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&error), ta013);
    drop(tx);
    let mut tx = begin_privileged(pool).await;
    sqlx::query(
        "INSERT INTO pvp_exclusions (season_id, user_id, excluded_by, reason) \
         VALUES ($1, $2, $3, 'farm_signals')",
    )
    .bind(SEPTEMBER)
    .bind(a)
    .bind(b)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(refused("DELETE FROM pvp_exclusions").await, ta013);
    // A fight that is not rated moves nothing, and the tournament is never rated.
    let mut tx = pool.begin().await.unwrap();
    let error = sqlx::query(
        "INSERT INTO pvp_fights (attacker_id, defender_id, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, rated, season_id, created_at) \
         VALUES ($1, $2, gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
         decode(repeat('00', 32), 'hex'), '\\x7b7d', 'a', 'ko', 5, 0, 0, 0, 1, false, 0, now())",
    )
    .bind(a)
    .bind(b)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("23514"));
    drop(tx);
    let error = sqlx::query(
        "INSERT INTO pvp_fights (attacker_id, defender_slug, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, rated, season_id, created_at) \
         VALUES ($1, 'rata-electrica', gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
         decode(repeat('00', 32), 'hex'), '\\x7b7d', 'a', 'ko', 0, 0, 0, 0, 1, true, 0, now())",
    )
    .bind(a)
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("23514"));
}

/// Every way the bytes of `hash` could be printed.
fn encodings(hash: &[u8; 32]) -> Vec<String> {
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    vec![
        hex.clone(),
        hex.to_uppercase(),
        base64::engine::general_purpose::STANDARD.encode(hash),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash),
        format!("{hash:?}"),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_report_shows_the_farm_of_a_closed_season_and_never_an_ip_hash() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let pool = &pool;
    let Closing { a, b, c } = closing(pool).await;
    let by = admin(pool).await;
    let plain = support::bets::user(pool).await;
    let farm_hash: [u8; 32] = std::array::from_fn(|i| u8::try_from(i).unwrap() * 7 + 3);
    for (user, at) in [(a, "2026-08-31T00:00:00Z"), (b, "2026-09-03T00:00:00Z")] {
        sqlx::query(
            "INSERT INTO auth_events (user_id, kind, provider, ip_hash, created_at) \
             VALUES ($1, 'login', 'dev', $2, $3::timestamptz)",
        )
        .bind(user)
        .bind(farm_hash.as_slice())
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }
    let audits = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_audit")
            .fetch_one(pool)
            .await
            .unwrap()
    };

    // Before the close there is nothing to review, and only an admin reviews.
    let report = pvp_cli(&db, SEPTEMBER, by, PvpAction::FarmReport).await;
    assert_eq!(report, Err(SeasonFailure::Refused("pvp.season_not_closed")));
    season::close_due(pool).await.unwrap();
    let report = pvp_cli(&db, SEPTEMBER, plain, PvpAction::FarmReport).await;
    assert_eq!(report, Err(SeasonFailure::Refused("pvp.not_admin")));
    assert_eq!(audits().await, 0);

    let text = pvp_cli(&db, SEPTEMBER, by, PvpAction::FarmReport)
        .await
        .unwrap();
    for encoding in encodings(&farm_hash) {
        assert!(!text.contains(&encoding), "{encoding}");
    }
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["season"], SEPTEMBER);
    assert_eq!(report["coverage"], "full");
    assert_eq!(report["fights"], 16);
    let mut pair = [a.to_string(), b.to_string()];
    pair.sort();
    assert_eq!(
        report["repeated_pairs"],
        serde_json::json!([{ "users": pair, "fights": 12, "attacker_share_bp": 10000 }])
    );
    assert_eq!(
        report["punching_bags"],
        serde_json::json!([{ "user_id": b.to_string(), "defenses": 12, "losses": 12 }])
    );
    let top = report["counterpart_concentration"]["top"]
        .as_array()
        .unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(
        top[0],
        serde_json::json!({
            "rank": 1,
            "user_id": a.to_string(),
            "rating": 1125,
            "gained": 125,
            "concentration_bp": 10000,
            "shares_ip_group": true,
            "flagged": true,
            "counterparts": [
                { "user_id": b.to_string(), "gained": 120, "ip_group": 1 },
                { "user_id": c.to_string(), "gained": 5, "ip_group": null },
            ],
        })
    );
    assert_eq!(top[1]["gained"], 0);
    assert_eq!(top[1]["concentration_bp"], Value::Null);
    let payload: Value = sqlx::query_scalar(
        "SELECT payload FROM admin_audit WHERE action = 'pvp_review' AND actor = 'cli_attributed' \
         AND admin_id = $1 AND target = $2",
    )
    .bind(by)
    .bind(SEPTEMBER.to_string())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(payload["flagged"], serde_json::json!([a.to_string()]));
    assert_eq!(payload["repeated_pairs"], 1);
    for encoding in encodings(&farm_hash) {
        assert!(!payload.to_string().contains(&encoding), "{encoding}");
    }

    // The exclusion: once, from a season that exists, of a user with a botarga.
    let exclude = |season: i32, user: Uuid| {
        pvp_cli(
            &db,
            season,
            by,
            PvpAction::Exclude {
                user,
                reason: ExcludeReason::CounterpartConcentration,
            },
        )
    };
    assert_eq!(
        exclude(SEPTEMBER, a).await.as_deref(),
        Ok("pvp.exclude_applied")
    );
    let before = audits().await;
    assert_eq!(
        exclude(SEPTEMBER, a).await,
        Err(SeasonFailure::Refused("pvp.already_excluded"))
    );
    assert_eq!(
        exclude(209_912, a).await,
        Err(SeasonFailure::Refused("pvp.season_not_found"))
    );
    // A month that ended without a rated fight has no leaderboard either.
    assert_eq!(
        exclude(202_001, a).await,
        Err(SeasonFailure::Refused("pvp.season_not_found"))
    );
    assert_eq!(
        exclude(SEPTEMBER, plain).await,
        Err(SeasonFailure::Refused("pvp.no_botarga"))
    );
    assert_eq!(audits().await, before);
    let mut conn = pool.acquire().await.unwrap();
    let september = season::closed(&mut conn, SEPTEMBER).await.unwrap().unwrap();
    let names: Vec<String> = season::entries(&mut conn, &september)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.display_name)
        .collect();
    assert_eq!(names, [display_name(b)]);
}

/// A fight that holds its two botargas while the close waits for them: the close sees it once it
/// commits, so the snapshot of September does not count it and the reset keeps it on top.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_close_waits_for_a_fight_and_two_closes_take_turns() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let Closing { a, b, .. } = closing(&pool).await;
    let mut fight = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM botargas WHERE user_id = ANY($1) ORDER BY user_id FOR UPDATE")
        .bind(vec![a, b])
        .execute(&mut *fight)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO pvp_fights (attacker_id, defender_id, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, rated, defender_rating_delta, season_id, created_at) \
         VALUES ($1, $2, gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
         decode(repeat('00', 32), 'hex'), '\\x7b7d', 'a', 'ko', 10, 0, 0, 0, 1, true, -10, 0, now())",
    )
    .bind(a)
    .bind(b)
    .execute(&mut *fight)
    .await
    .unwrap();
    for (user, delta) in [(a, 10), (b, -10)] {
        sqlx::query("UPDATE botargas SET rating = rating + $2 WHERE user_id = $1")
            .bind(user)
            .bind(delta)
            .execute(&mut *fight)
            .await
            .unwrap();
    }
    // Two closes at once: one waits for the fight, the other for the advisory lock.
    let first = tokio::spawn({
        let pool = pool.clone();
        async move { season::close_due(&pool).await.unwrap() }
    });
    let second = tokio::spawn({
        let pool = pool.clone();
        async move { season::close_due(&pool).await.unwrap() }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!first.is_finished() && !second.is_finished());
    fight.commit().await.unwrap();
    assert_eq!(first.await.unwrap() + second.await.unwrap(), 2);
    assert_eq!(
        snapshot(&pool, SEPTEMBER).await.len(),
        3,
        "each month is closed once"
    );
    assert_eq!(snapshot(&pool, SEPTEMBER).await[0], (a, 1125, 13, 13, 0));
    assert_eq!(
        (rating(&pool, a).await, rating(&pool, b).await),
        (1039, 970)
    );
}

/// A fight rated in a month that is already closed is refused: the one that waited for the close of
/// its month retries in the new one. One that is not rated is not.
#[tokio::test]
async fn a_rated_fight_never_lands_in_a_closed_month() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let (a, b) = (player(&pool).await, player(&pool).await);
    let mut tx = pool.begin().await.unwrap();
    // The current month closed, as the job would leave it once it ends.
    sqlx::query("ALTER TABLE pvp_seasons DISABLE TRIGGER pvp_seasons_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO pvp_seasons (id, starts_at, ends_at, closed_at) \
         SELECT (extract(year FROM m) * 100 + extract(month FROM m))::int, m AT TIME ZONE 'UTC', \
                (m + interval '1 month') AT TIME ZONE 'UTC', \
                (m + interval '1 month') AT TIME ZONE 'UTC' \
         FROM (SELECT date_trunc('month', now() AT TIME ZONE 'UTC') AS m) AS month",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let insert = |rated: bool, delta: i16| {
        sqlx::query(
            "INSERT INTO pvp_fights (attacker_id, defender_id, client_key, ruleset_version, \
             spec_a, spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, \
             xp_defender, coins_attacker, daily_index, rated, defender_rating_delta, season_id, \
             created_at) \
             VALUES ($1, $2, gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
             decode(repeat('00', 32), 'hex'), '\\x7b7d', 'a', 'ko', $4, 0, 0, 0, 1, $3, -$4, 0, \
             now())",
        )
        .bind(a)
        .bind(b)
        .bind(rated)
        .bind(delta)
    };
    insert(false, 0).execute(&mut *tx).await.unwrap();
    sqlx::query("SAVEPOINT rated")
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = insert(true, 10).execute(&mut *tx).await.unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("TA013"));
}
