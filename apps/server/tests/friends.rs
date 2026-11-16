//! 023 phase 1 (TA-86): friends by invite and their challenges over HTTP. An invite that cannot be
//! accepted is always the same 404; removing a friend blocks them from the remover's invite; a
//! challenge is resolved at once and private to its two sides, and moves no rating, XP, coins,
//! `pvp_daily` nor `pvp_fights`. Phase 2 (TA-87): a tournament of 4 or 8 is resolved whole, private
//! to its players, and moves nothing else either.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use engine::{CharacterSpec, Ruleset, Seed, canonical_json};
use serde_json::Value;
use server::app::Settings;
use server::rate::{RateSettings, ServerClock};
use sqlx::PgPool;
use support::app::{ORIGIN, RunningApp};
use support::db::TestDb;
use support::http;
use uuid::Uuid;

struct App {
    app: RunningApp,
    pool: PgPool,
    clock: ServerClock,
}

impl App {
    fn addr(&self) -> SocketAddr {
        self.app.addr
    }

    /// A request a minute of the rate clock later: no quota in the way.
    async fn send(&self, method: &str, path: &str, token: Option<&str>, body: &str) -> Value {
        let response = self.raw(method, path, token, body).await;
        let text = response.text();
        let mut value: Value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap()
        };
        if let Value::Object(map) = &mut value {
            map.insert("_status".to_owned(), response.status.as_u16().into());
        } else {
            value = serde_json::json!({ "_status": response.status.as_u16(), "_body": value });
        }
        value
    }

    async fn raw(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &str,
    ) -> http::Response {
        self.clock.advance(Duration::from_secs(60));
        let cookie = token.map(support::oauth::session_cookie);
        let mut headers = vec![("origin", ORIGIN), ("content-type", "application/json")];
        if let Some(cookie) = &cookie {
            headers.push(("cookie", cookie.as_str()));
        }
        http::send_body(self.addr(), method, path, &headers, body).await
    }

    async fn friends(&self, token: &str) -> Value {
        self.send("GET", "/api/friends", Some(token), "").await
    }

    async fn accept(&self, token: &str, code: &str) -> Value {
        let body = format!(r#"{{"code":"{code}"}}"#);
        self.send("POST", "/api/friends/accept", Some(token), &body)
            .await
    }

    async fn challenge(&self, token: &str, name: &str, key: &str) -> Value {
        let body = format!(r#"{{"display_name":"{name}","client_key":"{key}"}}"#);
        self.send("POST", "/api/friends/challenge", Some(token), &body)
            .await
    }
}

async fn start() -> (App, TestDb) {
    let db = TestDb::create().await;
    let clock = ServerClock::manual();
    let settings = Settings {
        rate: RateSettings {
            clock: clock.clone(),
            ..RateSettings::default()
        },
        ..Settings::default()
    };
    let content = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/characters");
    let (app, pool) = support::app::start_db(&db, &[("CONTENT_DIR", content)], &settings).await;
    (App { app, pool, clock }, db)
}

/// A user named `name` (with the key the server computes) and the botarga of a new user.
async fn player(pool: &PgPool, name: &str) -> (Uuid, String) {
    let (user, token) = support::app::user_with_session(pool).await;
    sqlx::query("UPDATE users SET display_name = $2, display_name_key = $3 WHERE id = $1")
        .bind(user)
        .bind(name)
        .bind(server::profile::name::key_of(name))
        .execute(pool)
        .await
        .unwrap();
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

fn status(value: &Value) -> u64 {
    value["_status"].as_u64().unwrap()
}

fn assert_code(value: &Value, expected: u64, code: &str) {
    assert_eq!(status(value), expected, "{value}");
    assert_eq!(value["error"], code, "{value}");
}

fn invite(view: &Value) -> String {
    view["invite"].as_str().unwrap().to_owned()
}

fn names(view: &Value) -> Vec<&str> {
    view["friends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["display_name"].as_str().unwrap())
        .collect()
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    // `table` is a literal of this file.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn ban(pool: &PgPool, user: Uuid) {
    let mut tx = support::db::begin_privileged(pool).await;
    sqlx::query("UPDATE users SET banned_at = now() WHERE id = $1")
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// `a` and `b` become friends through the invite of `a`.
async fn befriend(app: &App, a: &str, b: &str) {
    let code = invite(&app.friends(a).await);
    assert_eq!(status(&app.accept(b, &code).await), 200);
}

#[tokio::test]
async fn an_invite_makes_friends_and_every_refusal_is_the_same_404() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (_, ana) = player(pool, "Ana").await;
    let (_, beto) = player(pool, "Beto").await;
    let (carla_id, carla) = player(pool, "Carla").await;
    let (_, nobody) = support::app::user_with_session(pool).await;

    let view = app.friends(&ana).await;
    assert_eq!(status(&view), 200);
    assert!(names(&view).is_empty());
    let code = invite(&view);
    assert_eq!(code.len(), 22);
    let raw = app.raw("GET", "/api/friends", Some(&ana), "").await;
    assert_eq!(raw.header("cache-control"), Some("private, no-store"));
    assert_eq!(raw.header("vary"), Some("Cookie"));

    // Without a botarga there is no list and no accepting, whatever the code.
    assert_code(&app.friends(&nobody).await, 404, "botarga.not_created");
    assert_code(
        &app.accept(&nobody, &code).await,
        404,
        "botarga.not_created",
    );
    // Without a session.
    let anonymous = app
        .raw(
            "POST",
            "/api/friends/accept",
            None,
            &format!(r#"{{"code":"{code}"}}"#),
        )
        .await;
    assert_eq!(anonymous.status, 401);

    let body = format!(r#"{{"code":"{code}"}}"#);
    let preview = app
        .send("POST", "/api/friends/invite/preview", Some(&beto), &body)
        .await;
    assert_eq!(status(&preview), 200);
    assert_eq!(preview["display_name"], "Ana");
    assert_eq!(count(pool, "friendships").await, 0);

    let accepted = app.accept(&beto, &code).await;
    assert_eq!(status(&accepted), 200, "{accepted}");
    assert_eq!(names(&accepted), ["Ana"]);
    assert_eq!(names(&app.friends(&ana).await), ["Beto"]);
    assert_eq!(accepted["friends"][0]["ready"], true);
    assert_eq!(accepted["friends"][0]["level"], 1);
    assert_eq!(
        count(pool, "auth_events WHERE kind = 'friend_accept'").await,
        1
    );
    assert_code(
        &app.accept(&beto, &code).await,
        409,
        "friends.already_friends",
    );

    // The own code, a code of no one, a malformed one, a rotated one, and a banned owner: one 404.
    let not_found = |v: &Value| assert_code(v, 404, "friends.invite_not_found");
    not_found(&app.accept(&ana, &code).await);
    not_found(&app.accept(&carla, "AAAAAAAAAAAAAAAAAAAAAA").await);
    not_found(&app.accept(&carla, "short").await);
    not_found(&app.accept(&carla, "AAAAAAAAAAAAAAAAAAAAA=").await);
    let rotated = app
        .send("POST", "/api/friends/invite/rotate", Some(&ana), "")
        .await;
    assert_eq!(status(&rotated), 200);
    let new_code = invite(&rotated);
    assert_ne!(new_code, code);
    not_found(&app.accept(&carla, &code).await);
    let carla_code = invite(&app.friends(&carla).await);
    ban(pool, carla_id).await;
    not_found(&app.accept(&ana, &carla_code).await);
    let preview = app
        .send(
            "POST",
            "/api/friends/invite/preview",
            Some(&beto),
            &format!(r#"{{"code":"{carla_code}"}}"#),
        )
        .await;
    not_found(&preview);
    // A body that is not a request.
    assert_code(
        &app.send("POST", "/api/friends/accept", Some(&beto), "{}")
            .await,
        422,
        "friends.invalid_request",
    );
}

#[tokio::test]
async fn removing_a_friend_blocks_their_accept_until_the_remover_accepts_back() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (_, ana) = player(pool, "Ana").await;
    let (_, beto) = player(pool, "Beto").await;
    befriend(&app, &ana, &beto).await;

    let removed = app
        .send(
            "POST",
            "/api/friends/remove",
            Some(&ana),
            r#"{"display_name":"beto"}"#,
        )
        .await;
    assert_eq!(status(&removed), 200, "{removed}");
    assert!(names(&removed).is_empty());
    assert!(names(&app.friends(&beto).await).is_empty());
    assert_code(
        &app.send(
            "POST",
            "/api/friends/remove",
            Some(&ana),
            r#"{"display_name":"Beto"}"#,
        )
        .await,
        404,
        "friends.not_friend",
    );

    // Beto can no longer accept the invite of Ana, even a rotated one: the same 404.
    let code = invite(&app.friends(&ana).await);
    assert_code(
        &app.accept(&beto, &code).await,
        404,
        "friends.invite_not_found",
    );
    let preview = app
        .send(
            "POST",
            "/api/friends/invite/preview",
            Some(&beto),
            &format!(r#"{{"code":"{code}"}}"#),
        )
        .await;
    assert_code(&preview, 404, "friends.invite_not_found");
    let rotated = invite(
        &app.send("POST", "/api/friends/invite/rotate", Some(&ana), "")
            .await,
    );
    assert_code(
        &app.accept(&beto, &rotated).await,
        404,
        "friends.invite_not_found",
    );

    // Ana accepting the invite of Beto is the way back, and lifts her block.
    let beto_code = invite(&app.friends(&beto).await);
    assert_eq!(status(&app.accept(&ana, &beto_code).await), 200);
    assert_eq!(count(pool, "friend_blocks").await, 0);
}

/// The detail re-simulated by the engine from its own public fields gives exactly its log.
fn assert_replays(detail: &Value) {
    let spec = |field: &str| -> CharacterSpec {
        serde_json::from_str(detail[field].as_str().unwrap()).unwrap()
    };
    let hex = detail["seed"].as_str().unwrap();
    let mut seed = [0_u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    let ruleset = Ruleset::for_version(2).unwrap();
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
}

#[tokio::test]
async fn a_challenge_is_private_idempotent_and_moves_nothing_else() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (ana_id, ana) = player(pool, "Ana").await;
    let (_, beto) = player(pool, "Beto").await;
    let (_, carla) = player(pool, "Carla").await;
    let key = Uuid::new_v4().to_string();
    assert_code(
        &app.challenge(&ana, "Beto", &key).await,
        404,
        "friends.not_friend",
    );
    befriend(&app, &ana, &beto).await;

    let botargas_before: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(b)::text FROM botargas b ORDER BY user_id")
            .fetch_all(pool)
            .await
            .unwrap();
    let unlocks_before = count(pool, "botarga_unlocks").await;
    let created = app.challenge(&ana, "beto", &key).await;
    assert_eq!(status(&created), 201, "{created}");
    let fight = &created["fight"];
    assert_eq!(fight["a"]["display_name"], "Ana");
    assert_eq!(fight["b"]["display_name"], "Beto");
    assert_eq!(fight["rating_delta"], 0);
    assert_replays(fight);
    let again = app.challenge(&ana, "Beto", &key).await;
    assert_eq!(status(&again), 200);
    assert_eq!(again["fight"], created["fight"]);

    // Nothing of the ranked PvP, the progress nor the ledger.
    let botargas_after: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(b)::text FROM botargas b ORDER BY user_id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(botargas_before, botargas_after);
    assert_eq!(count(pool, "pvp_fights").await, 0);
    assert_eq!(count(pool, "pvp_daily").await, 0);
    assert_eq!(count(pool, "botarga_draws").await, 0);
    assert_eq!(count(pool, "botarga_unlocks").await, unlocks_before);
    assert_eq!(count(pool, "ledger_entries").await, 0);
    assert_eq!(count(pool, "friend_fights").await, 1);
    assert_eq!(
        count(pool, "auth_events WHERE kind = 'friend_challenge'").await,
        1
    );
    let challenges: i16 =
        sqlx::query_scalar("SELECT challenges FROM friend_daily WHERE user_id = $1")
            .bind(ana_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(challenges, 1);

    // Both sides read it, and in their lists; anyone else gets a 404.
    let id = fight["id"].as_i64().unwrap();
    let path = format!("/api/friends/fights/{id}");
    for token in [&ana, &beto] {
        let detail = app.send("GET", &path, Some(token), "").await;
        assert_eq!(status(&detail), 200);
        assert_eq!(detail["fight"], created["fight"]);
        let list = app
            .send("GET", "/api/friends/fights", Some(token), "")
            .await;
        assert_eq!(list["fights"][0]["id"], id);
    }
    let list = app
        .send("GET", "/api/friends/fights", Some(&beto), "")
        .await;
    assert_eq!(list["fights"][0]["role"], "defender");
    assert_eq!(list["fights"][0]["opponent"], "Ana");
    assert_code(
        &app.send("GET", &path, Some(&carla), "").await,
        404,
        "friends.not_found",
    );
    assert_code(
        &app.send("GET", "/api/friends/fights/x1", Some(&ana), "")
            .await,
        404,
        "friends.not_found",
    );
    assert_eq!(app.raw("GET", &path, None, "").await.status, 401);
    let pvp = app
        .send("GET", &format!("/api/pvp/fights/{id}"), Some(&ana), "")
        .await;
    assert_ne!(pvp["a"]["display_name"], "Ana");

    // The database refuses to change a fight between friends.
    let error = sqlx::query("UPDATE friend_fights SET winner = NULL")
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(server::db::error_kind(&error), "friend_fight_guard");
}

#[tokio::test]
async fn the_51st_challenge_of_the_day_is_refused_and_writes_nothing() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (ana_id, ana) = player(pool, "Ana").await;
    let (_, beto) = player(pool, "Beto").await;
    befriend(&app, &ana, &beto).await;
    sqlx::query(
        "INSERT INTO friend_daily (user_id, day, challenges) \
         VALUES ($1, (now() AT TIME ZONE 'UTC')::date, 50)",
    )
    .bind(ana_id)
    .execute(pool)
    .await
    .unwrap();
    let refused = app
        .challenge(&ana, "Beto", &Uuid::new_v4().to_string())
        .await;
    assert_code(&refused, 429, "friends.daily_limit");
    assert_eq!(count(pool, "friend_fights").await, 0);
    // The friend is not limited by the challenges of the other.
    let created = app
        .challenge(&beto, "Ana", &Uuid::new_v4().to_string())
        .await;
    assert_eq!(status(&created), 201);
}

#[tokio::test]
async fn crossed_challenges_at_once_both_finish() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (_, ana) = player(pool, "Ana").await;
    let (beto_id, beto) = player(pool, "Beto").await;
    befriend(&app, &ana, &beto).await;
    let (k1, k2) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
    let (one, two) = tokio::join!(
        app.challenge(&ana, "Beto", &k1),
        app.challenge(&beto, "Ana", &k2),
    );
    assert_eq!(status(&one), 201, "{one}");
    assert_eq!(status(&two), 201, "{two}");
    assert_eq!(count(pool, "friend_fights").await, 2);

    // A banned friend is no longer in the way of a challenge: the same 404 as a stranger.
    ban(pool, beto_id).await;
    let after = app
        .challenge(&ana, "Beto", &Uuid::new_v4().to_string())
        .await;
    assert_code(&after, 404, "friends.not_friend");
    let view = app.friends(&ana).await;
    assert_eq!(view["friends"][0]["ready"], false);
}

impl App {
    async fn tournament(&self, token: &str, size: u8, names: &[&str], key: &str) -> Value {
        let body = serde_json::json!({
            "size": size,
            "display_names": names,
            "client_key": key,
        })
        .to_string();
        self.send("POST", "/api/friends/tournaments", Some(token), &body)
            .await
    }
}

const NAMES: [&str; 9] = [
    "Ana", "Beto", "Carla", "Dani", "Eva", "Fede", "Gala", "Hugo", "Iris",
];

/// The owner Ana and eight other players, all friends of Ana and of nobody else.
async fn roster(app: &App) -> Vec<(Uuid, String)> {
    let mut players = Vec::new();
    for name in NAMES {
        players.push(player(&app.pool, name).await);
    }
    for (_, token) in &players[1..] {
        befriend(app, &players[0].1, token).await;
    }
    players
}

/// The bracket is whole: `size - 1` matches, round by round, each later match between the sides
/// that advanced from the two before it, and the champion the one who advanced from the final.
fn assert_bracket(t: &Value, size: usize, owner: &str, others: &[&str]) {
    assert_eq!(t["size"], size);
    assert_eq!(t["owner"], owner);
    let players: Vec<&str> = t["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    let mut sorted = players.clone();
    sorted.sort_unstable();
    let mut expected: Vec<&str> = others.iter().copied().chain([owner]).collect();
    expected.sort_unstable();
    assert_eq!(sorted, expected);
    let matches = t["matches"].as_array().unwrap();
    assert_eq!(matches.len(), size - 1);
    let advancer = |m: &Value| -> String {
        let side = m["advances"].as_str().unwrap();
        m[side].as_str().unwrap().to_owned()
    };
    let mut alive: Vec<String> = players.iter().map(|p| (*p).to_owned()).collect();
    let mut at = 0;
    let mut round = 1;
    while alive.len() > 1 {
        let mut next = Vec::new();
        for (slot, pair) in alive.chunks(2).enumerate() {
            let m = &matches[at];
            at += 1;
            assert_eq!(m["round"], round, "{t}");
            assert_eq!(m["slot"], slot, "{t}");
            assert_eq!(m["a"], pair[0].as_str(), "{t}");
            assert_eq!(m["b"], pair[1].as_str(), "{t}");
            next.push(advancer(m));
        }
        alive = next;
        round += 1;
    }
    assert_eq!(t["champion"], alive[0].as_str());
}

#[tokio::test]
async fn a_tournament_of_4_and_one_of_8_are_whole_private_and_move_nothing_else() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let players = roster(&app).await;
    let ana = &players[0].1;
    let outsider = player(pool, "Zoe").await.1;
    let botargas_before: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(b)::text FROM botargas b ORDER BY user_id")
            .fetch_all(pool)
            .await
            .unwrap();
    let unlocks_before = count(pool, "botarga_unlocks").await;

    let key4 = Uuid::new_v4().to_string();
    let four = app.tournament(ana, 4, &NAMES[1..4], &key4).await;
    assert_eq!(status(&four), 201, "{four}");
    assert_bracket(&four, 4, "Ana", &NAMES[1..4]);
    let again = app.tournament(ana, 4, &NAMES[1..4], &key4).await;
    assert_eq!(status(&again), 200);
    assert_eq!(again["id"], four["id"]);
    assert_eq!(again["matches"], four["matches"]);

    let eight = app
        .tournament(ana, 8, &NAMES[1..8], &Uuid::new_v4().to_string())
        .await;
    assert_eq!(status(&eight), 201, "{eight}");
    assert_bracket(&eight, 8, "Ana", &NAMES[1..8]);

    // Every match replays, and every player of the tournament reads every match of it.
    for m in four["matches"].as_array().unwrap() {
        let path = format!("/api/friends/fights/{}", m["fight_id"]);
        for token in [ana, &players[1].1, &players[3].1] {
            let detail = app.send("GET", &path, Some(token), "").await;
            assert_eq!(status(&detail), 200, "{detail}");
            assert_eq!(detail["fight"]["rating_delta"], 0);
            assert_replays(&detail["fight"]);
        }
        // Iris plays in no tournament of 4; Zoe in none at all.
        for token in [&players[8].1, &outsider] {
            assert_code(
                &app.send("GET", &path, Some(token), "").await,
                404,
                "friends.not_found",
            );
        }
    }

    // The tournament itself: its players read it; anybody else gets a 404.
    let path = format!("/api/friends/tournaments/{}", four["id"]);
    let read = app.send("GET", &path, Some(&players[2].1), "").await;
    assert_eq!(status(&read), 200);
    assert_eq!(read["matches"], four["matches"]);
    for token in [&players[8].1, &outsider] {
        assert_code(
            &app.send("GET", &path, Some(token), "").await,
            404,
            "friends.not_found",
        );
    }
    assert_eq!(app.raw("GET", &path, None, "").await.status, 401);
    let list = app
        .send("GET", "/api/friends/tournaments", Some(ana), "")
        .await;
    let ids: Vec<&Value> = list["tournaments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| &t["id"])
        .collect();
    assert_eq!(ids, [&eight["id"], &four["id"]]);
    assert_eq!(list["tournaments"][0]["champion"], eight["champion"]);
    let list = app
        .send("GET", "/api/friends/tournaments", Some(&players[8].1), "")
        .await;
    assert_eq!(list["tournaments"].as_array().unwrap().len(), 0);
    // The list of fights has only challenges: how many matches one played would tell how far one
    // went in a bracket.
    let fights = app.send("GET", "/api/friends/fights", Some(ana), "").await;
    assert_eq!(fights["fights"].as_array().unwrap().len(), 0);

    // Nothing of the ranked PvP, the progress nor the ledger.
    let botargas_after: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(b)::text FROM botargas b ORDER BY user_id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(botargas_before, botargas_after);
    assert_eq!(count(pool, "botarga_unlocks").await, unlocks_before);
    for table in ["pvp_fights", "pvp_daily", "botarga_draws", "ledger_entries"] {
        assert_eq!(count(pool, table).await, 0, "{table}");
    }
    assert_eq!(count(pool, "friend_fights").await, 3 + 7);
    assert_eq!(
        count(pool, "auth_events WHERE kind = 'friend_tournament'").await,
        2
    );
    let tournaments: i16 =
        sqlx::query_scalar("SELECT tournaments FROM friend_daily WHERE user_id = $1")
            .bind(players[0].0)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(tournaments, 2);

    // The database refuses to change a tournament, and a draw that advances the wrong side.
    let error = sqlx::query("DELETE FROM friend_tournament_slots")
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(server::db::error_kind(&error), "friend_fight_guard");
    let draw = |advances: &'static str| {
        let mut seed = vec![0_u8; 32];
        seed[0] = 0b0000_0011;
        sqlx::query(
            "INSERT INTO friend_fights (kind, attacker_id, defender_id, ruleset_version, \
             spec_a, spec_b, seed, log, winner, end_reason, tournament_id, round, slot, \
             advances) \
             VALUES ('bracket', $1, $2, 2, '\\x00', '\\x00', $3, '\\x00', NULL, 'draw', $4, 1, \
                     3, $5)",
        )
        .bind(players[0].0)
        .bind(players[1].0)
        .bind(seed)
        .bind(i32::try_from(four["id"].as_i64().unwrap()).unwrap())
        .bind(advances)
        .execute(pool)
    };
    assert!(draw("a").await.is_err());
    assert!(draw("b").await.is_ok());
}

#[tokio::test]
async fn a_tournament_that_cannot_be_is_one_422_and_the_11th_of_the_day_is_refused() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let players = roster(&app).await;
    let ana = &players[0].1;
    let stranger = player(pool, "Zoe").await;
    ban(pool, players[8].0).await;
    let key = || Uuid::new_v4().to_string();
    for (size, names) in [
        (5_u8, &["Beto", "Carla", "Dani", "Eva"][..]),
        (4, &["Beto", "Carla"][..]),
        (4, &["Beto", "Carla", "Dani", "Eva"][..]),
        (4, &["Beto", "beto", "Dani"][..]),
        (4, &["Beto", "Carla", "Zoe"][..]),
        (4, &["Beto", "Carla", "Iris"][..]),
        (4, &["Beto", "Carla", "Ana"][..]),
        (4, &["Beto", "Carla", "  "][..]),
    ] {
        let refused = app.tournament(ana, size, names, &key()).await;
        assert_code(&refused, 422, "friends.tournament_invalid");
    }
    // A stranger is no player of anybody's tournament, and has no friends to make one with.
    let refused = app.tournament(&stranger.1, 4, &NAMES[1..4], &key()).await;
    assert_code(&refused, 422, "friends.tournament_invalid");
    let bad_key = app.tournament(ana, 4, &NAMES[1..4], "not-a-key").await;
    assert_code(&bad_key, 422, "friends.invalid_request");
    assert_eq!(count(pool, "friend_tournaments").await, 0);
    assert_eq!(count(pool, "friend_fights").await, 0);
    assert_eq!(count(pool, "friend_daily").await, 0);

    sqlx::query(
        "INSERT INTO friend_daily (user_id, day, tournaments) \
         VALUES ($1, (now() AT TIME ZONE 'UTC')::date, 10)",
    )
    .bind(players[0].0)
    .execute(pool)
    .await
    .unwrap();
    let refused = app.tournament(ana, 4, &NAMES[1..4], &key()).await;
    assert_code(&refused, 429, "friends.daily_limit");
    assert_eq!(count(pool, "friend_tournaments").await, 0);
}

#[tokio::test]
async fn crossed_tournaments_at_once_both_finish() {
    let (app, _db) = start().await;
    let players = roster(&app).await;
    // Beto befriends Carla and Dani too, so both can hold a tournament of the same four.
    for i in [2, 3] {
        befriend(&app, &players[1].1, &players[i].1).await;
    }
    let (k1, k2) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
    let (one, two) = tokio::join!(
        app.tournament(&players[0].1, 4, &["Beto", "Carla", "Dani"], &k1),
        app.tournament(&players[1].1, 4, &["Ana", "Carla", "Dani"], &k2),
    );
    assert_eq!(status(&one), 201, "{one}");
    assert_eq!(status(&two), 201, "{two}");
    assert_eq!(count(&app.pool, "friend_fights").await, 6);
}

#[tokio::test]
async fn preview_gives_the_one_404_for_an_own_an_invented_and_a_rotated_code() {
    let (app, _db) = start().await;
    let (_, ana) = player(&app.pool, "Ana").await;
    let (_, beto) = player(&app.pool, "Beto").await;
    let preview = |token: String, code: String| {
        let app = &app;
        async move {
            let body = format!(r#"{{"code":"{code}"}}"#);
            app.send("POST", "/api/friends/invite/preview", Some(&token), &body)
                .await
        }
    };
    let old = invite(&app.friends(&ana).await);
    let shown = preview(beto.clone(), old.clone()).await;
    assert_eq!(status(&shown), 200, "{shown}");
    assert_eq!(shown["display_name"], "Ana");
    // Its own, one nobody has, and one rotated away: the same 404 as `accept`.
    assert_code(
        &preview(ana.clone(), old.clone()).await,
        404,
        "friends.invite_not_found",
    );
    assert_code(
        &preview(beto.clone(), "B".repeat(22)).await,
        404,
        "friends.invite_not_found",
    );
    let rotated = app
        .send("POST", "/api/friends/invite/rotate", Some(&ana), "")
        .await;
    assert_eq!(status(&rotated), 200);
    assert_code(
        &preview(beto.clone(), old.clone()).await,
        404,
        "friends.invite_not_found",
    );
    assert_code(
        &app.accept(&beto, &old).await,
        404,
        "friends.invite_not_found",
    );
    assert_eq!(
        preview(beto.clone(), invite(&rotated)).await["display_name"],
        "Ana"
    );
}

#[tokio::test]
async fn ten_challenges_keep_a_user_in_the_pvp_probation() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (ana_id, ana) = player(pool, "Ana").await;
    let (_, beto) = player(pool, "Beto").await;
    befriend(&app, &ana, &beto).await;
    // Old enough: only the ten attacks are missing (021 "Carencia PvP").
    sqlx::query("UPDATE users SET created_at = now() - interval '30 days' WHERE id = $1")
        .bind(ana_id)
        .execute(pool)
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    assert!(
        server::pvp::matchmaking::probation(&mut conn, ana_id)
            .await
            .unwrap()
    );
    for _ in 0..10 {
        let created = app
            .challenge(&ana, "Beto", &Uuid::new_v4().to_string())
            .await;
        assert_eq!(status(&created), 201, "{created}");
    }
    assert_eq!(count(pool, "friend_fights").await, 10);
    assert!(
        server::pvp::matchmaking::probation(&mut conn, ana_id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn two_accepts_at_once_keep_the_cap_of_200() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (host_id, host) = player(pool, "Host").await;
    // 199 friends already: one more fits, two do not.
    let catalog = server::botarga::BotargaCatalog::embedded().unwrap();
    for _ in 0..199 {
        let (user, _) = support::app::user_with_session(pool).await;
        let mut tx = pool.begin().await.unwrap();
        server::botarga::store::create(&mut tx, user, catalog.starter())
            .await
            .unwrap();
        let (lo, hi) = (host_id.min(user), host_id.max(user));
        sqlx::query("INSERT INTO friendships (user_lo, user_hi) VALUES ($1, $2)")
            .bind(lo)
            .bind(hi)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let (_, one) = player(pool, "Uno").await;
    let (_, two) = player(pool, "Dos").await;
    let code = invite(&app.friends(&host).await);
    // Both accepts must meet at the lock of the host, not run one after the other: the host's row
    // is held until both wait on it.
    let mut hold = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(host_id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let release = async {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(900);
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting == 2 {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "{waiting} waiting");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        hold.commit().await.unwrap();
    };
    let (a, b, ()) = tokio::join!(app.accept(&one, &code), app.accept(&two, &code), release);
    let mut statuses = [status(&a), status(&b)];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 422], "{a} {b}");
    let refused = if status(&a) == 422 { &a } else { &b };
    assert_code(refused, 422, "friends.limit");
    let friends: i64 =
        sqlx::query_scalar("SELECT count(*) FROM friendships WHERE user_lo = $1 OR user_hi = $1")
            .bind(host_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(friends, 200);
    assert_eq!(
        app.friends(&host).await["friends"]
            .as_array()
            .unwrap()
            .len(),
        200
    );
}

#[tokio::test]
async fn the_list_gives_each_friend_its_own_level_and_readiness() {
    let (app, _db) = start().await;
    let pool = &app.pool;
    let (_, ana) = player(pool, "Ana").await;
    let (beto_id, beto) = player(pool, "Beto").await;
    let (carla_id, carla) = player(pool, "Carla").await;
    befriend(&app, &ana, &beto).await;
    befriend(&app, &ana, &carla).await;
    sqlx::query("UPDATE botargas SET level = 4 WHERE user_id = $1")
        .bind(carla_id)
        .execute(pool)
        .await
        .unwrap();
    ban(pool, beto_id).await;
    let view = app.friends(&ana).await;
    let by_name = |name: &str| {
        view["friends"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["display_name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(by_name("Beto")["level"], 1);
    assert_eq!(by_name("Beto")["ready"], false);
    assert_eq!(by_name("Carla")["level"], 4);
    assert_eq!(by_name("Carla")["ready"], true);
}
