//! 021 phase 2 (TA-63): "buscar pelea" over HTTP. An attack is resolved by the engine with the
//! player ruleset in one transaction; its coins move only as a `pvp_reward` entry, scaled by the
//! step of the day, never for a defender, a pair over its limit or an account in probation; the
//! defender is drawn among the users of the windows of rating and level, skipping the locked ones,
//! and the character of the tournament is the last resort. The public detail re-simulates to the
//! same bytes, and carries nothing private.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use engine::{CharacterSpec, Ruleset, Seed, canonical_json};
use serde_json::Value;
use server::app::Settings;
use server::rate::{RateSettings, ServerClock};
use sqlx::{PgPool, Row};
use support::app::{ORIGIN, RunningApp};
use support::db::TestDb;
use support::http;
use uuid::Uuid;

const FIGHTS: &str = "/api/pvp/fights";

fn content_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/characters").to_owned()
}

struct App {
    app: RunningApp,
    pool: PgPool,
    clock: ServerClock,
}

impl App {
    fn addr(&self) -> SocketAddr {
        self.app.addr
    }

    /// `POST /api/pvp/fights` with `key`, a minute of the rate clock later: no quota in the way.
    async fn attack_with(&self, token: &str, key: &str) -> http::Response {
        self.clock.advance(Duration::from_secs(60));
        let cookie = support::oauth::session_cookie(token);
        http::send_body(
            self.addr(),
            "POST",
            FIGHTS,
            &[
                ("origin", ORIGIN),
                ("cookie", cookie.as_str()),
                ("content-type", "application/json"),
            ],
            &format!(r#"{{"client_key":"{key}"}}"#),
        )
        .await
    }

    async fn attack(&self, token: &str) -> http::Response {
        self.attack_with(token, &Uuid::new_v4().to_string()).await
    }

    async fn get(&self, path: &str, token: Option<&str>) -> http::Response {
        self.clock.advance(Duration::from_secs(60));
        let cookie = token.map(support::oauth::session_cookie);
        let mut headers = vec![("origin", ORIGIN)];
        if let Some(cookie) = &cookie {
            headers.push(("cookie", cookie.as_str()));
        }
        http::send(self.addr(), "GET", path, &headers).await
    }
}

async fn start_with(content: &str, extra: &[(&str, &str)]) -> (App, TestDb) {
    let db = TestDb::create().await;
    let clock = ServerClock::manual();
    let settings = Settings {
        rate: RateSettings {
            clock: clock.clone(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    let mut vars = vec![("CONTENT_DIR", content)];
    vars.extend_from_slice(extra);
    let (app, pool) = support::app::start_db(&db, &vars, &settings).await;
    (App { app, pool, clock }, db)
}

async fn start() -> (App, TestDb) {
    let dir = content_dir();
    start_with(&dir, &[]).await
}

/// A user with a session and the botarga of a new user.
async fn player(pool: &PgPool) -> (Uuid, String) {
    let (user, token) = support::app::user_with_session(pool).await;
    let catalog = server::botarga::BotargaCatalog::embedded().unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        server::botarga::store::create(&mut tx, user, catalog.starter())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    (user, token)
}

/// Out of the PvP probation: 8 days old, with 10 attacks against the tournament written by hand.
async fn veteran(pool: &PgPool, user: Uuid) {
    sqlx::query("UPDATE users SET created_at = now() - interval '8 days' WHERE id = $1")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO pvp_fights (attacker_id, defender_slug, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, season_id, created_at) \
         SELECT $1, 'rata-electrica', gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
         decode(repeat('00', 32), 'hex'), '\\x7b7d', 'a', 'ko', 0, 0, 0, 0, 1, 0, now() \
         FROM generate_series(1, 10)",
    )
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
}

/// A botarga that spends the minimum of every stat: it loses to a new one almost always.
async fn weaken(pool: &PgPool, user: Uuid) {
    sqlx::query(
        "UPDATE botargas SET hp = 20, attack = 20, defense = 20, speed = 20, special = 20 \
         WHERE user_id = $1",
    )
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
}

/// Bans the users of `filter` (a `WHERE` of this file) as the admin CLI would.
async fn ban_where(pool: &PgPool, filter: &str, users: &[Uuid]) {
    let mut tx = support::db::begin_privileged(pool).await;
    // `filter` is a literal of this file.
    let mut query = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE users SET banned_at = now() WHERE {filter}"
    )));
    for user in users {
        query = query.bind(*user);
    }
    query.execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

fn body(response: &http::Response) -> Value {
    serde_json::from_str(&response.text()).unwrap()
}

fn assert_private(response: &http::Response) {
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    assert_eq!(response.header("vary"), Some("Cookie"));
}

fn assert_code(response: &http::Response, status: u16, code: &str) {
    assert_eq!(response.status, status, "{}", response.text());
    assert_eq!(response.text(), format!(r#"{{"error":"{code}"}}"#));
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    // `table` is a literal of this file.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn balance(pool: &PgPool, user: Uuid) -> i64 {
    server::ledger::balance(pool, user).await.unwrap().0
}

/// The coins of every `pvp_reward` of `user`.
async fn rewarded(pool: &PgPool, user: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount), 0)::bigint FROM ledger_entries e \
         JOIN ledger_postings p ON p.entry_id = e.id JOIN ledger_accounts a ON a.id = p.account_id \
         WHERE e.kind = 'pvp_reward' AND a.user_id = $1",
    )
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn winner(created: &Value) -> Option<&str> {
    created["fight"]["winner"].as_str()
}

fn display_name(user: Uuid) -> String {
    format!("Test{}", user.simple())
}

/// The detail re-simulated by the engine from its own public fields gives exactly its log.
fn assert_replays(detail: &Value) {
    let spec = |field: &str| -> CharacterSpec {
        serde_json::from_str(detail[field].as_str().unwrap()).unwrap()
    };
    let hex = detail["seed"].as_str().unwrap();
    assert_eq!(hex.len(), 64);
    let mut seed = [0_u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    let version = u16::try_from(detail["ruleset_version"].as_u64().unwrap()).unwrap();
    assert_eq!(version, 2);
    let ruleset = Ruleset::for_version(version).unwrap();
    let log = engine::simulate(
        &Seed::from_bytes(seed),
        &spec("spec_a"),
        &spec("spec_b"),
        &ruleset,
    )
    .unwrap();
    let bytes = canonical_json(&log).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        detail["log"].as_str().unwrap()
    );
    let (won, reason) = log.outcome().unwrap();
    assert_eq!(detail["winner"], serde_json::to_value(won).unwrap());
    assert_eq!(detail["end_reason"], serde_json::to_value(reason).unwrap());
}

/// What no public body may carry (021 "Lectura").
fn assert_public(detail: &Value, users: &[Uuid], keys: &[String]) {
    let mut fields: Vec<&str> = detail
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        [
            "a",
            "b",
            "created_at",
            "end_reason",
            "id",
            "log",
            "rating_delta",
            "ruleset_version",
            "seed",
            "spec_a",
            "spec_b",
            "winner",
        ]
    );
    let text = detail.to_string();
    // The test users are named after their id in its simple form: the hyphenated one is the id.
    for user in users {
        assert!(!text.contains(&user.to_string()));
    }
    for key in keys {
        assert!(!text.contains(key.as_str()), "{key}");
    }
    for private in ["daily_index", "coins", "xp", "client_key", "user_id"] {
        assert!(!text.contains(private), "{private}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_attack_between_two_users_is_resolved_paid_by_entry_and_public() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token_a) = player(pool).await;
    let (b, token_b) = player(pool).await;
    veteran(pool, a).await;
    veteran(pool, b).await;
    weaken(pool, b).await;
    let balance_b = balance(pool, b).await;

    // The pair counts three times today: enough for the strong side to win at least once.
    let mut keys = Vec::new();
    let mut won = 0;
    for _ in 0..3 {
        let key = Uuid::new_v4().to_string();
        let created = app.attack_with(&token_a, &key).await;
        assert_eq!(created.status, 201, "{}", created.text());
        assert_private(&created);
        let created = body(&created);
        let fight = &created["fight"];
        assert_eq!(
            fight["a"],
            serde_json::json!({"kind": "player", "display_name": display_name(a)})
        );
        assert_eq!(
            fight["b"],
            serde_json::json!({"kind": "player", "display_name": display_name(b)})
        );
        // TA-64 moves the rating; until then nothing does.
        assert_eq!(fight["rating_delta"], 0);
        let coins = if winner(&created) == Some("a") {
            won += 1;
            15
        } else {
            0
        };
        assert_eq!(
            created["reward"],
            serde_json::json!({"rating_delta": 0, "xp": 0, "coins": coins, "factor_pct": 100})
        );
        assert_replays(fight);
        keys.push(key);
    }
    assert!(won > 0, "the weak botarga won three times in a row");

    // Only the attacker, only by entry, one per fight that paid.
    assert_eq!(rewarded(pool, a).await, 15 * won);
    assert_eq!(balance(pool, a).await, 15 * won);
    assert_eq!(balance(pool, b).await, balance_b);
    assert_eq!(
        count(pool, "ledger_entries WHERE kind = 'pvp_reward'").await,
        won
    );
    assert_eq!(support::bets::total(pool).await, 0);
    // The counter of the day is the attacker's alone, and the ratings stay.
    let daily: Vec<(Uuid, i16)> = sqlx::query("SELECT user_id, attacks FROM pvp_daily")
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    assert_eq!(daily, vec![(a, 3)]);
    let ratings: Vec<i32> = sqlx::query_scalar("SELECT rating FROM botargas ORDER BY rating")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(ratings, [1000, 1000]);
    // One event per attack, with the hash of the address.
    let events = sqlx::query(
        "SELECT user_id, octet_length(ip_hash) FROM auth_events WHERE kind = 'pvp_attack'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|row| row.get::<Uuid, _>(0) == a));
    assert!(events.iter().all(|row| row.get::<i32, _>(1) == 32));

    // The detail is public, and exactly what the verification needs.
    let id: i32 = sqlx::query_scalar(
        "SELECT max(id) FROM pvp_fights WHERE attacker_id = $1 AND defender_id = $2",
    )
    .bind(a)
    .bind(b)
    .fetch_one(pool)
    .await
    .unwrap();
    let detail = app.get(&format!("{FIGHTS}/{id}"), None).await;
    assert_eq!(detail.status, 200, "{}", detail.text());
    assert_eq!(detail.header("cache-control"), Some("no-store"));
    let detail = body(&detail);
    assert_public(&detail, &[a, b], &keys);
    assert_replays(&detail);

    // The lists of both sides, as each one lived it.
    let mine = app.get(FIGHTS, Some(&token_a)).await;
    assert_eq!(mine.status, 200, "{}", mine.text());
    assert_private(&mine);
    let mine = body(&mine);
    let fights = mine["fights"].as_array().unwrap();
    // The three real ones and the ten of `veteran`.
    assert_eq!(fights.len(), 13);
    assert!(fights.iter().all(|f| f["role"] == "attacker"));
    assert_eq!(fights[0]["id"], id);
    assert_eq!(mine["next_before"], Value::Null);
    let theirs = body(&app.get(FIGHTS, Some(&token_b)).await);
    let defenses: Vec<&Value> = theirs["fights"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["role"] == "defender")
        .collect();
    assert_eq!(defenses.len(), 3);
    for defense in defenses {
        assert_eq!(
            defense["opponent"],
            serde_json::json!({"kind": "player", "display_name": display_name(a)})
        );
        assert_eq!(defense["reward"]["coins"], 0);
        assert!(!defense.to_string().contains(&a.to_string()));
    }
    // And the botarga shows the attacks of today.
    let botarga = body(&app.get("/api/botarga", Some(&token_a)).await);
    assert_eq!(
        botarga["progress"]["daily"],
        serde_json::json!({"attacks": 3, "factor_pct": 100})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_client_key_is_the_same_fight_even_at_once() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    let (b, _) = player(pool).await;
    veteran(pool, a).await;
    veteran(pool, b).await;
    let key = Uuid::new_v4().to_string();
    let first = app.attack_with(&token, &key).await;
    assert_eq!(first.status, 201, "{}", first.text());
    let again = app.attack_with(&token, &key).await;
    assert_eq!(again.status, 200, "{}", again.text());
    assert_private(&again);
    let (first, again) = (body(&first), body(&again));
    assert_eq!(first["fight"], again["fight"]);
    assert_eq!(first["reward"], again["reward"]);
    // The progress is read back: the same attacks of the day, since nothing new was written.
    assert_eq!(first["progress"], again["progress"]);
    assert_eq!(
        count(pool, "pvp_fights WHERE defender_id IS NOT NULL").await,
        1
    );
    assert_eq!(count(pool, "pvp_daily").await, 1);

    // Two at once with a new key: one fight, one counter, at most one entry.
    let key = Uuid::new_v4().to_string();
    let rewards_before = count(pool, "ledger_entries WHERE kind = 'pvp_reward'").await;
    let (one, two) = tokio::join!(app.attack_with(&token, &key), app.attack_with(&token, &key));
    let mut statuses = [one.status.as_u16(), two.status.as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 201], "{} {}", one.text(), two.text());
    assert_eq!(body(&one)["fight"], body(&two)["fight"]);
    assert_eq!(
        count(pool, "pvp_fights WHERE defender_id IS NOT NULL").await,
        2
    );
    let attacks: i16 = sqlx::query_scalar("SELECT attacks FROM pvp_daily WHERE user_id = $1")
        .bind(a)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(attacks, 2);
    assert!(count(pool, "ledger_entries WHERE kind = 'pvp_reward'").await - rewards_before <= 1);
    assert_eq!(support::bets::total(pool).await, 0);
}

/// Writes `attacks` as the counter of today of `user`.
async fn set_attacks(pool: &PgPool, user: Uuid, attacks: i16) {
    sqlx::query(
        "INSERT INTO pvp_daily (user_id, day, attacks) VALUES ($1, (now() AT TIME ZONE 'UTC')::date, $2) \
         ON CONFLICT (user_id, day) DO UPDATE SET attacks = $2",
    )
    .bind(user)
    .bind(attacks)
    .execute(pool)
    .await
    .unwrap();
}

/// Attacks until the attacker wins, at most `tries` times, and returns that answer.
async fn a_victory(app: &App, token: &str, tries: usize) -> Option<Value> {
    for _ in 0..tries {
        let created = app.attack(token).await;
        assert_eq!(created.status, 201, "{}", created.text());
        let created = body(&created);
        if winner(&created) == Some("a") {
            return Some(created);
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_steps_of_the_day_scale_the_coins_and_the_21st_pays_nothing() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    veteran(pool, a).await;
    // A new weak defender for each step, so the limit of the pair never gets in the way.
    for attacks in [10, 13, 16, 20] {
        let (b, _) = player(pool).await;
        veteran(pool, b).await;
        weaken(pool, b).await;
        // The defenders of the earlier steps are left out.
        ban_where(pool, "id <> $1 AND id <> $2", &[a, b]).await;
        set_attacks(pool, a, attacks).await;
        let before = balance(pool, a).await;
        let created = a_victory(&app, &token, 3).await.expect("no victory");
        // A defeat before the victory is one more step: read the index the fight was written with.
        let id = i32::try_from(created["fight"]["id"].as_i64().unwrap()).unwrap();
        let index: i16 = sqlx::query_scalar("SELECT daily_index FROM pvp_fights WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(index > attacks, "{index}");
        let (factor, coins) = match index {
            11..=13 => (75, 11),
            14..=16 => (50, 7),
            17..=20 => (25, 3),
            _ => (0, 0),
        };
        assert_eq!(created["reward"]["factor_pct"], factor, "{index}");
        assert_eq!(created["reward"]["coins"], coins, "{index}");
        assert_eq!(balance(pool, a).await - before, coins, "{index}");
        // From the 21st on, the fight is resolved and stored all the same.
        if attacks == 20 {
            assert_eq!(created["reward"]["factor_pct"], 0);
        }
    }
    assert_eq!(support::bets::total(pool).await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_fourth_fight_of_a_pair_and_a_side_in_probation_pay_nothing() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    let (b, _) = player(pool).await;
    veteran(pool, a).await;
    veteran(pool, b).await;
    weaken(pool, b).await;
    // Three fights of the pair today, one of them the other way round.
    for (attacker, defender) in [(a, b), (b, a), (a, b)] {
        sqlx::query(
            "INSERT INTO pvp_fights (attacker_id, defender_id, client_key, ruleset_version, \
             spec_a, spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, \
             xp_defender, coins_attacker, daily_index, season_id, created_at) \
             VALUES ($1, $2, gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
             decode(repeat('00', 32), 'hex'), '\\x7b7d', NULL, 'draw', 0, 0, 0, 0, 1, 0, now())",
        )
        .bind(attacker)
        .bind(defender)
        .execute(pool)
        .await
        .unwrap();
    }
    let created = a_victory(&app, &token, 5).await.expect("no victory");
    assert_eq!(created["reward"]["coins"], 0);
    assert_eq!(created["reward"]["factor_pct"], 100);
    assert_eq!(rewarded(pool, a).await, 0);

    // A defender in probation (a new account): the fight does not pay either.
    let (c, _) = player(pool).await;
    weaken(pool, c).await;
    ban_where(pool, "id = $1", &[b]).await;
    let created = a_victory(&app, &token, 5).await.expect("no victory");
    assert_eq!(
        created["fight"]["b"]["display_name"],
        display_name(c).as_str()
    );
    assert_eq!(created["reward"]["coins"], 0);

    // An attacker in probation: 10 attacks but 2 days old.
    sqlx::query("UPDATE users SET created_at = now() - interval '2 days' WHERE id = $1")
        .bind(a)
        .execute(pool)
        .await
        .unwrap();
    let (d, _) = player(pool).await;
    veteran(pool, d).await;
    weaken(pool, d).await;
    ban_where(pool, "id = $1", &[c]).await;
    let created = a_victory(&app, &token, 5).await.expect("no victory");
    assert_eq!(
        created["fight"]["b"]["display_name"],
        display_name(d).as_str()
    );
    assert_eq!(created["reward"]["coins"], 0);
    assert_eq!(rewarded(pool, a).await, 0);
    assert_eq!(
        count(pool, "ledger_entries WHERE kind = 'pvp_reward'").await,
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_other_users_the_rival_is_a_character_of_the_tournament() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    veteran(pool, a).await;
    let roster: Vec<String> = std::fs::read_dir(content_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    for _ in 0..3 {
        let created = app.attack(&token).await;
        assert_eq!(created.status, 201, "{}", created.text());
        let created = body(&created);
        let b = &created["fight"]["b"];
        assert_eq!(b["kind"], "tournament");
        assert!(roster.iter().any(|slug| b["slug"] == slug.as_str()), "{b}");
        // XP only (TA-62), never rating nor coins.
        assert_eq!(created["reward"]["coins"], 0);
        assert_eq!(created["reward"]["rating_delta"], 0);
        assert_replays(&created["fight"]);
        // Scaled to the level of the attacker: the budget and the slots of level 1.
        let spec: CharacterSpec =
            serde_json::from_str(created["fight"]["spec_b"].as_str().unwrap()).unwrap();
        let s = spec.stats;
        let sum: u16 = [s.hp, s.attack, s.defense, s.speed, s.special]
            .iter()
            .map(|&v| u16::from(v))
            .sum();
        assert!(sum <= 200, "{sum}");
        assert_eq!(spec.abilities.len(), 1);
    }
    assert_eq!(
        count(pool, "pvp_fights WHERE defender_slug IS NOT NULL").await,
        13
    );
    assert_eq!(count(pool, "ledger_entries").await, 0);
    let attacks: i16 = sqlx::query_scalar("SELECT attacks FROM pvp_daily WHERE user_id = $1")
        .bind(a)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(attacks, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_windows_leave_out_the_far_the_banned_and_the_locked() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (_a, token) = player(pool).await;
    let (b, _) = player(pool).await;
    let kind = |created: &Value| created["fight"]["b"]["kind"].as_str().unwrap().to_owned();
    // Four steps of 100: 401 points away is out of every window.
    sqlx::query("UPDATE botargas SET rating = 1401 WHERE user_id = $1")
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(kind(&body(&app.attack(&token).await)), "tournament");
    sqlx::query("UPDATE botargas SET rating = 1400 WHERE user_id = $1")
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(kind(&body(&app.attack(&token).await)), "player");
    // The gap of level 1 is 3.
    sqlx::query("UPDATE botargas SET rating = 1000, level = 5 WHERE user_id = $1")
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(kind(&body(&app.attack(&token).await)), "tournament");
    sqlx::query("UPDATE botargas SET level = 4 WHERE user_id = $1")
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(kind(&body(&app.attack(&token).await)), "player");
    // A row someone else holds is skipped, without waiting for it.
    let mut held = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM botargas WHERE user_id = $1 FOR UPDATE")
        .bind(b)
        .execute(&mut *held)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    assert_eq!(kind(&body(&app.attack(&token).await)), "tournament");
    assert!(started.elapsed() < Duration::from_millis(900));
    held.rollback().await.unwrap();
    // A banned user is never a defender.
    ban_where(pool, "id = $1", &[b]).await;
    assert_eq!(kind(&body(&app.attack(&token).await)), "tournament");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_the_catalog_no_longer_has_is_refused_or_skipped() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    let (b, _) = player(pool).await;
    let gone = |user: Uuid| {
        sqlx::query("UPDATE botargas SET abilities = ARRAY['retired_move'] WHERE user_id = $1")
            .bind(user)
            .execute(pool)
    };
    // The defender's: it is skipped, and the tournament steps in.
    gone(b).await.unwrap();
    let created = body(&app.attack(&token).await);
    assert_eq!(created["fight"]["b"]["kind"], "tournament");
    // The attacker's: a code, never a 500, and nothing is written.
    gone(a).await.unwrap();
    let before = count(pool, "pvp_fights").await;
    assert_code(&app.attack(&token).await, 409, "pvp.build_invalid");
    assert_eq!(count(pool, "pvp_fights").await, before);
    assert_eq!(
        count(pool, "auth_events WHERE kind = 'pvp_attack'").await,
        before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_any_rival_the_search_says_so_and_writes_nothing() {
    let (app, _db) = start_with("/nonexistent/content/characters", &[]).await;
    let pool = &app.pool;
    let (_, token) = player(pool).await;
    let response = app.attack(&token).await;
    assert_code(&response, 503, "pvp.no_rival");
    assert_private(&response);
    for table in [
        "pvp_fights",
        "pvp_daily",
        "auth_events WHERE kind = 'pvp_attack'",
    ] {
        assert_eq!(count(pool, table).await, 0, "{table}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn win_coins_zero_turns_the_coins_off() {
    let dir = content_dir();
    let (app, _db) = start_with(&dir, &[("PVP_WIN_COINS", "0")]).await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    let (b, _) = player(pool).await;
    veteran(pool, a).await;
    veteran(pool, b).await;
    weaken(pool, b).await;
    let created = a_victory(&app, &token, 3).await.expect("no victory");
    assert_eq!(created["reward"]["coins"], 0);
    assert_eq!(count(pool, "ledger_entries").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_requests_answer_a_code() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (_, token) = player(pool).await;
    let cookie = support::oauth::session_cookie(&token);
    let post = |body: &'static str| {
        let cookie = cookie.clone();
        let app = &app;
        async move {
            app.clock.advance(Duration::from_secs(60));
            http::send_body(
                app.addr(),
                "POST",
                FIGHTS,
                &[
                    ("origin", ORIGIN),
                    ("cookie", cookie.as_str()),
                    ("content-type", "application/json"),
                ],
                body,
            )
            .await
        }
    };
    for bad in [
        "",
        "{}",
        r#"{"client_key":"key"}"#,
        r#"{"client_key":"0B0E8E5E-8A43-4C3C-9B55-0E6E3E1B2A10"}"#,
        r#"{"client_key":"0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10","defender":"x"}"#,
    ] {
        let response = post(bad).await;
        assert_code(&response, 422, "pvp.invalid_request");
        assert_private(&response);
    }
    assert_eq!(count(pool, "pvp_fights").await, 0);
    // Without a botarga, the code of the botarga.
    let (_, other) = support::app::user_with_session(pool).await;
    assert_code(&app.attack(&other).await, 404, "botarga.not_created");
    // Without a session.
    let anonymous = http::send_body(
        app.addr(),
        "POST",
        FIGHTS,
        &[("origin", ORIGIN), ("content-type", "application/json")],
        r#"{"client_key":"0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10"}"#,
    )
    .await;
    assert_eq!(anonymous.status, 401);
    assert_eq!(app.get(FIGHTS, None).await.status, 401);
    // The reads.
    for path in [
        format!("{FIGHTS}?before=abc"),
        format!("{FIGHTS}?before=01"),
    ] {
        assert_code(
            &app.get(&path, Some(&token)).await,
            422,
            "pvp.invalid_request",
        );
    }
    for path in [
        "/api/pvp/fights/0",
        "/api/pvp/fights/abc",
        "/api/pvp/fights/1",
    ] {
        let response = app.get(path, None).await;
        assert_code(&response, 404, "pvp.not_found");
        assert_eq!(response.header("cache-control"), Some("no-store"));
    }
    let empty = body(&app.get(FIGHTS, Some(&token)).await);
    assert_eq!(
        empty,
        serde_json::json!({"fights": [], "next_before": null})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_list_pages_by_twenty() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    // 25 fights: `veteran` writes 10 at a time.
    for _ in 0..2 {
        veteran(pool, a).await;
    }
    for _ in 0..5 {
        assert_eq!(app.attack(&token).await.status, 201);
    }
    let first = body(&app.get(FIGHTS, Some(&token)).await);
    let ids: Vec<i64> = first["fights"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids.len(), 20);
    assert!(ids.windows(2).all(|pair| pair[0] > pair[1]));
    let next = first["next_before"].as_i64().unwrap();
    assert_eq!(next, *ids.last().unwrap());
    let second = body(
        &app.get(&format!("{FIGHTS}?before={next}"), Some(&token))
            .await,
    );
    assert_eq!(second["fights"].as_array().unwrap().len(), 5);
    assert_eq!(second["next_before"], Value::Null);
}

/// The database is the last barrier: one `pvp_reward` per fight, of exactly its coins and to its
/// attacker; the issuance pays no other new kind; and a fight is never changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_database_refuses_a_second_or_a_wrong_reward() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, _) = player(pool).await;
    let (b, _) = player(pool).await;
    let fight: i32 = sqlx::query_scalar(
        "INSERT INTO pvp_fights (attacker_id, defender_id, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, season_id, created_at) \
         VALUES ($1, $2, gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', decode(repeat('00', 32), 'hex'), \
         '\\x7b7d', 'a', 'ko', 0, 0, 0, 15, 1, 0, now()) RETURNING id",
    )
    .bind(a)
    .bind(b)
    .fetch_one(pool)
    .await
    .unwrap();
    let code = |error: sqlx::Error| {
        error
            .as_database_error()
            .and_then(|e| e.code())
            .map(|c| c.into_owned())
    };
    let reward = |user: Uuid, amount: i64| async move {
        let mut tx = pool.begin().await?;
        server::ledger::pvp_reward(&mut tx, fight, user, server::ledger::Coins(amount)).await?;
        tx.commit().await
    };
    // A wrong amount, and a reward to the defender.
    assert_eq!(
        code(reward(a, 14).await.unwrap_err()).as_deref(),
        Some("TA008")
    );
    assert_eq!(
        code(reward(b, 15).await.unwrap_err()).as_deref(),
        Some("23503")
    );
    // The right one, once.
    reward(a, 15).await.unwrap();
    assert_eq!(
        code(reward(a, 15).await.unwrap_err()).as_deref(),
        Some("23505")
    );
    assert_eq!(balance(pool, a).await, 15);
    // Only a `pvp_reward` points to a fight.
    let error = sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, pvp_fight_id) VALUES ($1, 'grant', $2, $3)",
    )
    .bind(Uuid::new_v4())
    .bind(a)
    .bind(fight)
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(code(error).as_deref(), Some("23514"));
    // A fight does not change nor go away.
    for statement in [
        "UPDATE pvp_fights SET coins_attacker = 0",
        "DELETE FROM pvp_fights",
    ] {
        let error = sqlx::query(statement).execute(pool).await.unwrap_err();
        assert_eq!(code(error).as_deref(), Some("TA011"), "{statement}");
    }
    // A fight against the tournament cannot pay.
    let error = sqlx::query(
        "INSERT INTO pvp_fights (attacker_id, defender_slug, client_key, ruleset_version, spec_a, \
         spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, \
         coins_attacker, daily_index, season_id, created_at) \
         VALUES ($1, 'rata-electrica', gen_random_uuid(), 2, '\\x7b7d', '\\x7b7d', \
         decode(repeat('00', 32), 'hex'), '\\x7b7d', 'a', 'ko', 0, 0, 0, 15, 1, 0, now())",
    )
    .bind(a)
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(code(error).as_deref(), Some("23514"));
    assert_eq!(support::bets::total(pool).await, 0);
}

/// Owner, 2026-11-11: at most 50 attacks a UTC day, since each one stores its fight for good. The
/// 50th is resolved; the 51st answers a code and writes nothing; a retry of an earlier key still
/// gives its fight.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_51st_attack_of_the_day_is_refused_and_writes_nothing() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (a, token) = player(pool).await;
    set_attacks(pool, a, 49).await;
    let key = Uuid::new_v4().to_string();
    let last = app.attack_with(&token, &key).await;
    assert_eq!(last.status, 201, "{}", last.text());
    assert_eq!(body(&last)["progress"]["daily"]["attacks"], 50);
    let before = count(pool, "pvp_fights").await;
    let refused = app.attack(&token).await;
    assert_code(&refused, 429, "pvp.daily_limit");
    assert_private(&refused);
    assert_eq!(count(pool, "pvp_fights").await, before);
    assert_eq!(
        count(pool, "auth_events WHERE kind = 'pvp_attack'").await,
        before
    );
    let attacks: i16 = sqlx::query_scalar("SELECT attacks FROM pvp_daily WHERE user_id = $1")
        .bind(a)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(attacks, 50);
    assert_eq!(app.attack_with(&token, &key).await.status, 200);
    // And the database refuses a 51st by itself.
    let error = sqlx::query("UPDATE pvp_daily SET attacks = 51 WHERE user_id = $1")
        .bind(a)
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514")
    );
}
