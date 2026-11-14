//! 020, phase B: `server season farm-report|exclude|close` (ADR-0008 D8, test 28 but the `/64`, which
//! is of 008). Fights and their results are sown straight in the database, in September 2026, so the
//! season has ended; the season of January 2027 (`f::T0`) has not. The app is not started: no
//! reconciliation sums a fight behind the back of a test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use base64::Engine as _;
use server::admin::season::{
    ExcludeReason, SEASON_DEADLINE, SeasonAction, SeasonCommand, SeasonFailure, execute_season,
};
use server::seasons;
use sqlx::{PgPool, Row};
use support::bin;
use support::db::{TestDb, begin_privileged, privileged};
use support::fights::{self as f, wait_for};
use uuid::Uuid;

/// 2026-09-10T00:00:00Z, a multiple of the cadence.
const SEPTEMBER: i64 = 1_788_998_400_000;
const SEASON: i32 = 202_609;
/// The season of `f::T0`, which has not ended.
const FUTURE: i32 = 202_701;

struct World {
    db: TestDb,
    pool: PgPool,
    admin: Uuid,
    slot: AtomicI64,
}

async fn world() -> World {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = support::bets::user(&pool).await;
    privileged(
        &pool,
        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1").bind(admin),
    )
    .await;
    World {
        db,
        pool,
        admin,
        slot: AtomicI64::new(0),
    }
}

/// What a command can write: a refusal leaves all of it as it was.
#[derive(Debug, PartialEq)]
struct Written {
    audit: Vec<(String, String)>,
    tallies: i64,
    closed: Vec<(i32, bool)>,
    stats: Vec<(i32, Uuid, i64, i32, bool)>,
}

impl World {
    /// A user old enough to be out of probation at the end of September.
    async fn user(&self) -> Uuid {
        let id = support::bets::user(&self.pool).await;
        sqlx::query("UPDATE users SET created_at = '2026-08-01' WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
        id
    }

    async fn fight_opening_at(&self, opens_at: i64) -> i32 {
        f::announce(
            &self.pool,
            opens_at,
            &f::golden_spec("striker"),
            &f::golden_spec("tank"),
        )
        .await
    }

    /// The next fight of September, still scheduled.
    async fn fight(&self) -> i32 {
        let k = self.slot.fetch_add(1, Ordering::SeqCst);
        self.fight_opening_at(SEPTEMBER + k * f::CADENCE).await
    }

    /// One bet and its result, written straight (the window of the fight is long gone).
    #[allow(clippy::too_many_arguments)]
    async fn bet(
        &self,
        fight: i32,
        user: Uuid,
        side: &str,
        stake: i64,
        outcome: &str,
        net: i64,
        ranked: i64,
    ) -> Uuid {
        let bet = Uuid::new_v4();
        let mut tx = begin_privileged(&self.pool).await;
        sqlx::query("ALTER TABLE bets DISABLE TRIGGER USER")
            .execute(&mut *tx)
            .await
            .unwrap();
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
        sqlx::query("ALTER TABLE bets ENABLE TRIGGER USER")
            .execute(&mut *tx)
            .await
            .unwrap();
        let payout = if outcome == "lost" { 0 } else { stake + net };
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
        bet
    }

    /// A won bet, its gain all ranked.
    async fn won(&self, fight: i32, user: Uuid, stake: i64, gain: i64) -> Uuid {
        self.bet(fight, user, "a", stake, "won", gain, gain).await
    }

    async fn lost(&self, fight: i32, user: Uuid, stake: i64) -> Uuid {
        self.bet(fight, user, "b", stake, "lost", -stake, -stake)
            .await
    }

    /// An event of `auth_events` with `hash`, at `at` (SQL), of the bet if there is one.
    async fn event(&self, user: Uuid, hash: &[u8], at: &str, bet: Option<Uuid>) {
        sqlx::query(
            "INSERT INTO auth_events (user_id, kind, provider, ip_hash, bet_id, created_at) \
             VALUES ($1, $2, 'dev', $3, $4, $5::timestamptz)",
        )
        .bind(user)
        .bind(if bet.is_some() { "bet" } else { "login" })
        .bind(hash)
        .bind(bet)
        .bind(at)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// `scheduled` → `settled`, the legal edge of a cancelled fight: the season reads only results.
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

    async fn tally(&self, fight: i32) -> bool {
        let mut tx = self.pool.begin().await.unwrap();
        let summed = seasons::tally(&mut tx, fight).await.unwrap();
        tx.commit().await.unwrap();
        summed
    }

    /// Settled and summed, as the hook leaves it.
    async fn done(&self, fight: i32) {
        self.settle(fight).await;
        self.tally(fight).await;
    }

    /// `count` fights of September where `user` wins nothing: what a user needs to be on the board.
    async fn fillers(&self, users: &[Uuid], count: usize) {
        for _ in 0..count {
            let fight = self.fight().await;
            for user in users {
                self.won(fight, *user, 10, 0).await;
            }
            self.done(fight).await;
        }
    }

    async fn run(
        &self,
        by: Uuid,
        season: i32,
        action: SeasonAction,
    ) -> Result<String, SeasonFailure> {
        let command = SeasonCommand { season, by, action };
        execute_season(&self.db.options(), &command, SEASON_DEADLINE).await
    }

    async fn report(&self) -> serde_json::Value {
        let text = self
            .run(self.admin, SEASON, SeasonAction::FarmReport)
            .await
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    async fn close(&self, accept_partial: bool) -> Result<String, SeasonFailure> {
        self.run(self.admin, SEASON, SeasonAction::Close { accept_partial })
            .await
    }

    async fn exclude(&self, season: i32, user: Uuid) -> Result<String, SeasonFailure> {
        let action = SeasonAction::Exclude {
            user,
            reason: ExcludeReason::CounterpartConcentration,
        };
        self.run(self.admin, season, action).await
    }

    async fn written(&self) -> Written {
        let audit = sqlx::query("SELECT action, payload::text FROM admin_audit ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        let tallies = sqlx::query_scalar("SELECT count(*) FROM season_tallies")
            .fetch_one(&self.pool)
            .await
            .unwrap();
        let closed = sqlx::query("SELECT id, closed_at IS NOT NULL FROM seasons ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        let stats = sqlx::query(
            "SELECT season_id, user_id, ranked_profit, settled_fights, excluded_at IS NOT NULL \
             FROM season_stats ORDER BY season_id, user_id",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)))
        .collect();
        Written {
            audit,
            tallies,
            closed,
            stats,
        }
    }

    /// The rows of one action of `admin_audit`, with their payload.
    async fn audit(&self, action: &str) -> Vec<(String, Option<Uuid>, String, serde_json::Value)> {
        sqlx::query(
            "SELECT actor, admin_id, target, payload FROM admin_audit WHERE action = $1 ORDER BY id",
        )
        .bind(action)
        .fetch_all(&self.pool)
        .await
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
        .collect()
    }

    /// A review written straight, at `at` (SQL): the trigger takes it, as it is signed by an admin
    /// after the end of the season.
    async fn review_at(&self, at: &str) {
        self.review(
            "INSERT INTO admin_audit (actor, admin_id, action, target, payload, at) \
             VALUES ('cli_attributed', $1, 'season_review', $2, $3::jsonb, $4::timestamptz)",
            at,
        )
        .await;
    }

    /// A review of `hours` ago.
    async fn review_ago(&self, hours: i32) {
        self.review(
            "INSERT INTO admin_audit (actor, admin_id, action, target, payload, at) \
             VALUES ('cli_attributed', $1, 'season_review', $2, $3::jsonb, \
                     now() - make_interval(hours => $4::text::int))",
            &hours.to_string(),
        )
        .await;
    }

    async fn review(&self, sql: &'static str, at: &str) {
        sqlx::query(sql)
            .bind(self.admin)
            .bind(SEASON.to_string())
            .bind(serde_json::json!({ "coverage": "full" }).to_string())
            .bind(at)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// An event at the start of the window of `farm-report`, a day before the season: `auth_events`
    /// covers all of it.
    async fn cover(&self) {
        let someone = self.user().await;
        self.event(someone, &[0; 32], "2026-08-31T00:00:00Z", None)
            .await;
    }
}

#[allow(non_snake_case)]
fn REFUSED(code: &'static str) -> Result<String, SeasonFailure> {
    Err(SeasonFailure::Refused(code))
}

/// `--by` that does not exist, is not an admin, or is a banned admin: every command refuses first,
/// and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_an_admin_who_is_not_banned() {
    let w = world().await;
    w.cover().await;
    let user = w.user().await;
    w.fillers(&[user], 1).await;
    let banned = w.user().await;
    privileged(
        &w.pool,
        sqlx::query("UPDATE users SET role = 'admin', banned_at = now() WHERE id = $1")
            .bind(banned),
    )
    .await;
    let before = w.written().await;
    for by in [Uuid::new_v4(), user, banned] {
        for action in [
            SeasonAction::FarmReport,
            SeasonAction::Close {
                accept_partial: true,
            },
            SeasonAction::Exclude {
                user,
                reason: ExcludeReason::FarmSignals,
            },
        ] {
            assert_eq!(w.run(by, SEASON, action).await, REFUSED("season.not_admin"));
        }
    }
    assert_eq!(w.written().await, before);
}

/// Test 28: `farm-report` refuses without writing anything on a season without a row, before
/// `ends_at`, and with a pending fight: unsettled, or settled without its mark.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn farm_report_refuses_what_it_cannot_review() {
    let w = world().await;
    w.cover().await;
    let ana = w.user().await;
    w.fillers(&[ana], 1).await;
    let future = w.fight_opening_at(f::T0).await;
    w.won(future, ana, 10, 5).await;
    w.done(future).await;

    let before = w.written().await;
    for (season, code) in [(202_001, "season.not_found"), (FUTURE, "season.not_ended")] {
        assert_eq!(
            w.run(w.admin, season, SeasonAction::FarmReport).await,
            REFUSED(code)
        );
    }
    let pending = w.fight().await;
    w.won(pending, ana, 10, 5).await;
    let before_pending = w.written().await;
    assert_eq!(
        w.run(w.admin, SEASON, SeasonAction::FarmReport).await,
        REFUSED("season.pending_fights")
    );
    // Settled, but its sum is missing: still pending.
    w.settle(pending).await;
    assert_eq!(
        w.run(w.admin, SEASON, SeasonAction::FarmReport).await,
        REFUSED("season.pending_fights")
    );
    assert_eq!(w.written().await, before_pending);
    assert_eq!(before.audit, before_pending.audit);
    assert!(w.audit("season_review").await.is_empty());

    assert!(w.tally(pending).await);
    w.report().await;
    assert_eq!(w.audit("season_review").await.len(), 1);
}

/// Test 28: one row of `season_review` per run, signed by its admin, with the counts, the users marked
/// and the coverage, and no `UNIQUE` per season.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_review_per_run() {
    let w = world().await;
    w.cover().await;
    let ana = w.user().await;
    w.fillers(&[ana], 2).await;
    let first = w.report().await;
    let second = w.report().await;
    assert_eq!(first, second);
    let rows = w.audit("season_review").await;
    assert_eq!(rows.len(), 2);
    for (actor, admin, target, payload) in rows {
        assert_eq!(actor, "cli_attributed");
        assert_eq!(admin, Some(w.admin));
        assert_eq!(target, SEASON.to_string());
        assert_eq!(
            payload,
            serde_json::json!({
                "coverage": "full",
                "fights": 2,
                "shared_ip_opposite_sides": 0,
                "reviewed": 0,
                "flagged": [],
            })
        );
    }
}

/// D8: the farm of a main and its alt behind one `ip_hash` shows in both lists, with an opaque label;
/// an honest winner with many small counterparts is not marked. No byte of a sown `ip_hash` reaches
/// the output, the payload or the logs of the binary.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_report_shows_the_farm_and_never_an_ip_hash() {
    let w = world().await;
    let main = w.user().await;
    let alt = w.user().await;
    let other = w.user().await;
    let honest = w.user().await;
    let farm_hash: [u8; 32] = std::array::from_fn(|i| u8::try_from(i).unwrap() * 7 + 3);
    let lone_hash: [u8; 32] = std::array::from_fn(|i| 255 - u8::try_from(i).unwrap());
    // The first event of the season: it covers all of it, and its hash is the first label.
    w.event(main, &farm_hash, "2026-08-31T00:00:00Z", None)
        .await;
    w.event(alt, &farm_hash, "2026-09-02T12:00:00Z", None).await;
    w.event(honest, &lone_hash, "2026-09-02T13:00:00Z", None)
        .await;
    w.fillers(&[main, honest], 10).await;

    // The farm: `main` wins 300, given by `alt` (300 of the 400 lost) and `other` (100).
    let farm = w.fight().await;
    let main_bet = w.won(farm, main, 100, 300).await;
    let alt_bet = w.lost(farm, alt, 300).await;
    w.lost(farm, other, 100).await;
    w.event(main, &farm_hash, "2026-09-20T10:00:00Z", Some(main_bet))
        .await;
    w.event(alt, &farm_hash, "2026-09-20T10:01:00Z", Some(alt_bet))
        .await;
    w.done(farm).await;

    // The honest one: 100 from ten losers of 10 each, so its top five gave exactly 50 %.
    let fair = w.fight().await;
    w.won(fair, honest, 100, 100).await;
    for _ in 0..10 {
        let loser = w.user().await;
        w.lost(fair, loser, 10).await;
    }
    w.done(fair).await;

    let report = w.report().await;
    assert_eq!(report["coverage"], "full");
    assert_eq!(report["fights"], 12);
    let mut pair = [main.to_string(), alt.to_string()];
    pair.sort();
    assert_eq!(
        report["shared_ip_opposite_sides"],
        serde_json::json!([{ "fight_id": farm, "ip_group": 1, "users": pair }])
    );
    let top = report["counterpart_concentration"]["top"]
        .as_array()
        .unwrap();
    assert_eq!(report["counterpart_concentration"]["threshold_bp"], 5000);
    assert_eq!(top.len(), 2);
    assert_eq!(
        top[0],
        serde_json::json!({
            "rank": 1,
            "user_id": main.to_string(),
            "ranked_profit": 300,
            "concentration_bp": 10000,
            "shares_ip_group": true,
            "flagged": true,
            "counterparts": [
                { "user_id": alt.to_string(), "given": 225, "ip_group": 1 },
                { "user_id": other.to_string(), "given": 75, "ip_group": null },
            ],
        })
    );
    assert_eq!(top[1]["user_id"], honest.to_string());
    assert_eq!(top[1]["concentration_bp"], 5000);
    assert_eq!(top[1]["shares_ip_group"], false);
    assert_eq!(top[1]["flagged"], false);
    assert_eq!(top[1]["counterparts"].as_array().unwrap().len(), 5);

    let (_, _, _, payload) = w.audit("season_review").await.remove(0);
    assert_eq!(payload["flagged"], serde_json::json!([main.to_string()]));
    assert_eq!(payload["shared_ip_opposite_sides"], 1);
    assert_eq!(payload["reviewed"], 2);

    // The binary, at every level of log: the report is the line that starts with `{"season"`.
    bin::warm_up().await;
    let url = w.db.url();
    let by = w.admin.to_string();
    let season = SEASON.to_string();
    let args = [
        "season",
        "farm-report",
        season.as_str(),
        "--by",
        by.as_str(),
    ];
    for level in ["info", "trace", "debug,sqlx=trace"] {
        let done = bin::spawn(
            &args,
            &[("DATABASE_URL", url.as_str()), ("RUST_LOG", level)],
        )
        .finish(Duration::from_secs(30))
        .await;
        assert!(done.status.success(), "{level}: {:?}", done.stderr);
        let line = done
            .stdout
            .iter()
            .find(|line| line.starts_with("{\"season\""))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(line).unwrap(),
            report
        );
        let all = [done.stdout.join("\n"), done.stderr.join("\n")].join("\n");
        assert_no_hash(&all, &[farm_hash, lone_hash], level);
    }
    let payloads = sqlx::query_scalar::<_, String>("SELECT payload::text FROM admin_audit")
        .fetch_all(&w.pool)
        .await
        .unwrap()
        .join("\n");
    assert_eq!(w.audit("season_review").await.len(), 4);
    assert_no_hash(&payloads, &[farm_hash, lone_hash], "payload");
    assert_no_hash(&report.to_string(), &[farm_hash, lone_hash], "report");
}

/// No form of any of `hashes` in `text`: hex in both cases, base64 in both alphabets, a Postgres
/// `bytea` literal, and the bytes themselves.
fn assert_no_hash(text: &str, hashes: &[[u8; 32]], case: &str) {
    for hash in hashes {
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        for form in [
            hex.clone(),
            hex.to_uppercase(),
            format!("\\x{hex}"),
            base64::engine::general_purpose::STANDARD.encode(hash),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash),
            // A prefix is enough to give a hash away.
            hex[..16].to_owned(),
        ] {
            assert!(!text.contains(&form), "{case}: {form}");
        }
        assert!(
            !text.as_bytes().windows(hash.len()).any(|w| w == hash),
            "{case}: raw bytes"
        );
    }
}

/// D8: `coverage` is `partial` when the oldest `auth_events` is after `starts_at`, and `close` takes a
/// partial review only with `--accept-partial`, which stays in its audit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_partial_review_closes_only_when_accepted() {
    let w = world().await;
    let ana = w.user().await;
    w.event(ana, &[9; 32], "2026-09-03T00:00:00Z", None).await;
    w.fillers(&[ana], 1).await;
    let report = w.report().await;
    assert_eq!(report["coverage"], "partial");
    assert_eq!(w.audit("season_review").await[0].3["coverage"], "partial");

    let before = w.written().await;
    assert_eq!(w.close(false).await, REFUSED("season.partial_review"));
    assert_eq!(w.written().await, before);
    assert_eq!(w.close(true).await, Ok("season.close_applied".to_owned()));
    let closes = w.audit("season_close").await;
    assert_eq!(closes.len(), 1);
    let (actor, admin, target, payload) = &closes[0];
    assert_eq!(
        (actor.as_str(), *admin, target.as_str()),
        ("cli_attributed", Some(w.admin), "202609")
    );
    assert_eq!(payload["coverage"], "partial");
    assert_eq!(payload["accept_partial"], true);
    assert_eq!(payload["tallied"], 0);
}

/// The coverage starts where the window of the groups does, a day before the season: an event one
/// second later leaves the bets of the last minutes of August without their `ip_hash`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_coverage_starts_a_day_before_the_season() {
    let w = world().await;
    let ana = w.user().await;
    w.event(ana, &[9; 32], "2026-08-31T00:00:01Z", None).await;
    w.fillers(&[ana], 1).await;
    assert_eq!(w.report().await["coverage"], "partial");
    w.event(ana, &[9; 32], "2026-08-31T00:00:00Z", None).await;
    assert_eq!(w.report().await["coverage"], "full");
}

/// Without any `auth_events`, nothing is covered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_events_is_a_partial_coverage() {
    let w = world().await;
    let ana = w.user().await;
    w.fillers(&[ana], 1).await;
    assert_eq!(w.report().await["coverage"], "partial");
}

/// Test 28: `close` refuses without a review, with one before `ends_at`, with one older than 48 h,
/// and with a pending fight; with one of the last 48 h after `ends_at` it closes, once. Every refusal
/// writes nothing, the sums it made under its lock included.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_needs_a_fresh_review_after_the_end() {
    let w = world().await;
    w.cover().await;
    let ana = w.user().await;
    w.fillers(&[ana], 2).await;

    let before = w.written().await;
    assert_eq!(w.close(false).await, REFUSED("season.no_review"));
    assert_eq!(w.written().await, before);
    w.review_at("2026-09-30T23:00:00Z").await;
    w.review_ago(49).await;
    let before = w.written().await;
    assert_eq!(w.close(false).await, REFUSED("season.no_review"));
    assert_eq!(w.written().await, before);

    // A settled fight without its mark is summed under the lock, then the rest is checked: an
    // unsettled one refuses, and the sum goes with the refusal.
    let unmarked = w.fight().await;
    w.won(unmarked, ana, 10, 7).await;
    w.settle(unmarked).await;
    let unsettled = w.fight().await;
    w.won(unsettled, ana, 10, 7).await;
    w.review_ago(1).await;
    let before = w.written().await;
    assert_eq!(w.close(false).await, REFUSED("season.pending_fights"));
    assert_eq!(w.written().await, before);

    w.settle(unsettled).await;
    assert_eq!(w.close(false).await, Ok("season.close_applied".to_owned()));
    let (_, _, _, payload) = w.audit("season_close").await.remove(0);
    assert_eq!(payload["tallied"], 2);
    assert_eq!(payload["accept_partial"], false);
    assert_eq!(payload["coverage"], "full");
    let after = w.written().await;
    assert_eq!(after.tallies, before.tallies + 2);
    assert!(after.closed.contains(&(SEASON, true)));
    assert_eq!(
        after.stats,
        vec![(SEASON, ana, 14, 4, false)],
        "the two fights summed by close"
    );

    let before = w.written().await;
    assert_eq!(w.close(true).await, REFUSED("season.already_closed"));
    w.report_after_close().await;
    assert_eq!(w.written().await.closed, before.closed);
}

impl World {
    /// A review still runs on a closed season (it only reads), and adds its row.
    async fn report_after_close(&self) {
        let rows = self.audit("season_review").await.len();
        self.report().await;
        assert_eq!(self.audit("season_review").await.len(), rows + 1);
    }
}

/// `close` refuses a season without a row and one that has not ended, before any review.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_a_missing_or_running_season() {
    let w = world().await;
    let ana = w.user().await;
    let future = w.fight_opening_at(f::T0).await;
    w.won(future, ana, 10, 5).await;
    w.done(future).await;
    let before = w.written().await;
    for (season, code) in [(202_001, "season.not_found"), (FUTURE, "season.not_ended")] {
        let action = SeasonAction::Close {
            accept_partial: true,
        };
        assert_eq!(w.run(w.admin, season, action).await, REFUSED(code));
    }
    assert_eq!(w.written().await, before);
}

/// `exclude` writes `excluded_*` with the CLI actor and its audit, and refuses a user without stats,
/// a second exclusion and a closed season. The board leaves the user out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exclude_takes_a_user_out_of_an_open_season() {
    let w = world().await;
    w.cover().await;
    let ana = w.user().await;
    let bob = w.user().await;
    w.fillers(&[ana, bob], 10).await;
    let mut tx = w.pool.begin().await.unwrap();
    assert_eq!(
        seasons::board::ranked(&mut tx, SEASON, 20)
            .await
            .unwrap()
            .len(),
        2
    );
    drop(tx);

    let before = w.written().await;
    for (season, user, code) in [
        (SEASON, Uuid::new_v4(), "season.no_stats"),
        (202_001, ana, "season.not_found"),
    ] {
        assert_eq!(w.exclude(season, user).await, REFUSED(code));
    }
    assert_eq!(w.written().await, before);

    assert_eq!(
        w.exclude(SEASON, ana).await,
        Ok("season.exclude_applied".to_owned())
    );
    let row = sqlx::query(
        "SELECT excluded_by, excluded_reason, excluded_at IS NOT NULL FROM season_stats \
         WHERE season_id = $1 AND user_id = $2",
    )
    .bind(SEASON)
    .bind(ana)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<Option<Uuid>, _>(0), Some(w.admin));
    assert_eq!(
        row.get::<Option<String>, _>(1).as_deref(),
        Some("counterpart_concentration")
    );
    assert!(row.get::<bool, _>(2));
    let audit = w.audit("season_exclude").await;
    assert_eq!(audit.len(), 1);
    assert_eq!(
        audit[0].3,
        serde_json::json!({ "user_id": ana.to_string(), "reason": "counterpart_concentration" })
    );
    let mut tx = w.pool.begin().await.unwrap();
    let ranked = seasons::board::ranked(&mut tx, SEASON, 20).await.unwrap();
    drop(tx);
    assert_eq!(ranked.iter().map(|r| r.user_id).collect::<Vec<_>>(), [bob]);

    let before = w.written().await;
    assert_eq!(
        w.exclude(SEASON, ana).await,
        REFUSED("season.already_excluded")
    );
    assert_eq!(w.written().await, before);

    w.report().await;
    w.close(false).await.unwrap();
    let before = w.written().await;
    assert_eq!(w.exclude(SEASON, bob).await, REFUSED("season.closed"));
    assert_eq!(w.written().await, before);
}

/// The sum of a fight on a row that `exclude` wrote: the guard lets it through without the CLI actor,
/// and the exclusion stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tally_passes_over_an_excluded_row() {
    let w = world().await;
    let ana = w.user().await;
    w.fillers(&[ana], 1).await;
    w.exclude(SEASON, ana).await.unwrap();
    let fight = w.fight().await;
    w.won(fight, ana, 10, 40).await;
    w.settle(fight).await;
    assert!(w.tally(fight).await);
    let row = sqlx::query(
        "SELECT ranked_profit, settled_fights, excluded_reason FROM season_stats \
         WHERE season_id = $1 AND user_id = $2",
    )
    .bind(SEASON)
    .bind(ana)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>(0), 40);
    assert_eq!(row.get::<i32, _>(1), 2);
    assert_eq!(
        row.get::<Option<String>, _>(2).as_deref(),
        Some("counterpart_concentration")
    );
}

/// The second barrier: a `season_review` that is not signed by an admin from the CLI, of a season
/// that does not exist or has not ended, never enters `admin_audit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guard_takes_only_a_signed_review_after_the_end() {
    let w = world().await;
    let ana = w.user().await;
    w.fillers(&[ana], 1).await;
    let future = w.fight_opening_at(f::T0).await;
    w.done(future).await;
    let insert = |actor: &'static str, admin: Option<Uuid>, target: String| {
        let pool = w.pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
                 VALUES ($1, $2, 'season_review', $3, '{}')",
            )
            .bind(actor)
            .bind(admin)
            .bind(target)
            .execute(&pool)
            .await
        }
    };
    for (actor, admin, target) in [
        ("cli", None, SEASON.to_string()),
        ("cli_attributed", Some(w.admin), "202001".to_owned()),
        ("cli_attributed", Some(w.admin), FUTURE.to_string()),
        ("cli_attributed", Some(w.admin), "2026-09".to_owned()),
        ("cli_attributed", Some(w.admin), "x202609".to_owned()),
        ("cli_attributed", Some(w.admin), String::new()),
    ] {
        let error = insert(actor, admin, target.clone()).await.unwrap_err();
        assert_eq!(
            server::db::error_kind(&error),
            "season_review_guard",
            "{actor} {target}"
        );
    }
    // Signed by someone who is not an admin, or by a banned admin.
    let banned = w.user().await;
    privileged(
        &w.pool,
        sqlx::query("UPDATE users SET role = 'admin', banned_at = now() WHERE id = $1")
            .bind(banned),
    )
    .await;
    for by in [ana, banned] {
        let error = insert("cli_attributed", Some(by), SEASON.to_string())
            .await
            .unwrap_err();
        assert_eq!(server::db::error_kind(&error), "season_review_guard");
    }
    insert("cli_attributed", Some(w.admin), SEASON.to_string())
        .await
        .unwrap();
    // The other actions of a season are not reviews: the guard leaves them alone.
    sqlx::query(
        "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
         VALUES ('cli_attributed', $1, 'season_exclude', $2, '{}')",
    )
    .bind(w.admin)
    .bind(FUTURE.to_string())
    .execute(&w.pool)
    .await
    .unwrap();
}

/// Locks crossed for real: a sum holds the season `FOR KEY SHARE` and the stats of its users while
/// `close` waits on the season, and goes on to sum another fight that `close` would sum too. Then the
/// other way: with the season held `FOR UPDATE`, a sum waits before writing its mark, so the holder
/// sums that same fight without waiting on it. No deadlock in either order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_and_a_sum_never_deadlock() {
    let w = world().await;
    w.cover().await;
    let ana = w.user().await;
    let bob = w.user().await;
    w.fillers(&[ana, bob], 1).await;
    let first = w.fight().await;
    w.won(first, bob, 10, 3).await;
    w.settle(first).await;
    let second = w.fight().await;
    w.won(second, ana, 10, 5).await;
    w.won(second, bob, 10, 5).await;
    w.settle(second).await;
    w.review_ago(1).await;

    // A sum first, `close` behind it.
    let mut sum = w.pool.begin().await.unwrap();
    assert!(seasons::tally(&mut sum, second).await.unwrap());
    let options = w.db.options();
    let by = w.admin;
    let close = tokio::spawn(async move {
        let command = SeasonCommand {
            season: SEASON,
            by,
            action: SeasonAction::Close {
                accept_partial: false,
            },
        };
        execute_season(&options, &command, SEASON_DEADLINE).await
    });
    wait_for_lock(&w.pool, "FOR UPDATE").await;
    assert!(seasons::tally(&mut sum, first).await.unwrap());
    sum.commit().await.unwrap();
    assert_eq!(close.await.unwrap(), Ok("season.close_applied".to_owned()));
    let (_, _, _, payload) = w.audit("season_close").await.remove(0);
    assert_eq!(payload["tallied"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sum_waits_for_the_season_before_its_mark() {
    let w = world().await;
    let ana = w.user().await;
    w.fillers(&[ana], 1).await;
    let fight = w.fight().await;
    w.won(fight, ana, 10, 5).await;
    w.settle(fight).await;

    let mut holder = w.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM seasons WHERE id = $1 FOR UPDATE")
        .bind(SEASON)
        .execute(&mut *holder)
        .await
        .unwrap();
    let pool = w.pool.clone();
    let waiting = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        let summed = seasons::tally(&mut tx, fight).await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(summed)
    });
    wait_for_lock(&w.pool, "FOR KEY SHARE").await;
    // Its mark is not written yet: the holder sums the fight at once, without waiting on it.
    sqlx::query("SET LOCAL lock_timeout = '2s'")
        .execute(&mut *holder)
        .await
        .unwrap();
    assert!(seasons::tally(&mut holder, fight).await.unwrap());
    holder.commit().await.unwrap();
    assert!(!waiting.await.unwrap().unwrap());
    assert_eq!(w.written().await.stats, vec![(SEASON, ana, 5, 2, false)]);
}

/// Many sums of fights with users in common, from both sides of `close`, while it sums the rest in
/// another order: no `40P01`, and every fight counted once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_among_many_sums() {
    let w = Arc::new(world().await);
    w.cover().await;
    let ana = w.user().await;
    let bob = w.user().await;
    w.fillers(&[ana, bob], 1).await;
    let mut fights = Vec::new();
    for i in 0..60 {
        let fight = w.fight().await;
        // `close` takes them in order of id: `bob` alone, then `ana` alone, and so on.
        match i % 3 {
            0 => {
                w.won(fight, bob, 10, 1).await;
            }
            1 => {
                w.won(fight, ana, 10, 1).await;
            }
            _ => {
                w.won(fight, ana, 10, 1).await;
                w.won(fight, bob, 10, 1).await;
            }
        }
        w.settle(fight).await;
        fights.push(fight);
    }
    w.review_ago(1).await;
    let mut sums = Vec::new();
    for chunk in fights.chunks(15) {
        let pool = w.pool.clone();
        let mut chunk = chunk.to_vec();
        chunk.reverse();
        sums.push(tokio::spawn(async move {
            for fight in chunk {
                let mut tx = pool.begin().await.unwrap();
                seasons::tally(&mut tx, fight).await?;
                tx.commit().await?;
            }
            Ok::<_, sqlx::Error>(())
        }));
    }
    let closed = w.close(false).await;
    for sum in sums {
        sum.await.unwrap().unwrap();
    }
    // Either `close` came after every sum, or it summed some itself; a pending fight refuses only
    // if it was summed by neither, which a sum in flight cannot leave.
    assert_eq!(closed, Ok("season.close_applied".to_owned()));
    let stats = w.written().await.stats;
    let mut expected = vec![(SEASON, ana, 40, 41, false), (SEASON, bob, 40, 41, false)];
    expected.sort_by_key(|row| row.1);
    assert_eq!(stats, expected);
}

/// Waits until a query that contains `what` waits on a lock.
async fn wait_for_lock(pool: &PgPool, what: &str) {
    let pattern = format!("%{what}%");
    wait_for("a query waiting on a lock", || {
        let pool = pool.clone();
        let pattern = pattern.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE wait_event_type = 'Lock' AND query LIKE $1",
            )
            .bind(pattern)
            .fetch_one(&pool)
            .await
            .unwrap()
                > 0
        }
    })
    .await;
}

/// Test 28: no other way to write a `season_review`. Outside this file, `src/admin/season.rs` and the
/// migration of phase B, nothing of the server, its crates or the web names the action.
#[test]
fn only_the_subcommand_names_a_season_review() {
    let root = f::repo_root();
    let token = ["season", "_review"].concat();
    let allowed: Vec<PathBuf> = [
        "apps/server/src/admin/season.rs",
        "apps/server/tests/season_cli.rs",
        "apps/server/migrations/20261110000000_season_review.sql",
        // 021 phase 4 widens `admin_audit_action_ck` with the PvP actions: it lists the action
        // again in the same check, and writes nothing.
        "apps/server/migrations/20261114000000_pvp_ranking.sql",
    ]
    .iter()
    .map(|p| root.join(p))
    .collect();
    let mut seen = 0;
    for dir in [
        "apps/server/src",
        "apps/server/tests",
        "apps/server/migrations",
        "crates",
        "apps/web/src",
        "packages",
    ] {
        for path in files(&root.join(dir)) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            seen += 1;
            if names(&text, &token) {
                assert!(allowed.contains(&path), "{} names {token}", path.display());
            }
        }
    }
    assert!(seen > 100, "{seen}");
    // Never vacuous: the code of the subcommand (not its tests) writes it, and the migration guards it.
    let code = std::fs::read_to_string(&allowed[0]).unwrap();
    let code = code.split("#[cfg(test)]").next().unwrap();
    assert!(code.contains(&format!("\"{token}\"")));
    let migration = std::fs::read_to_string(&allowed[2]).unwrap();
    assert!(migration.contains(&format!("NEW.action = '{token}'")));
}

/// Whether `text` has `token` as a whole word: `season_review_guard` is another name.
fn names(text: &str, token: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.split(|c: char| !word(c)).any(|w| w == token)
}

/// Every file under `dir`, without the directories of builds and dependencies.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if path.is_dir() {
                if !["node_modules", "target", "dist", ".svelte-kit", "build"]
                    .contains(&name.as_ref())
                {
                    stack.push(path);
                }
            } else {
                out.push(path);
            }
        }
    }
    out
}
