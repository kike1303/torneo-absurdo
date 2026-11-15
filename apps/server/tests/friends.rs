//! 023 phase 1 (TA-86): friends by invite and their challenges over HTTP. An invite that cannot be
//! accepted is always the same 404; removing a friend blocks them from the remover's invite; a
//! challenge is resolved at once and private to its two sides, and moves no rating, XP, coins,
//! `pvp_daily` nor `pvp_fights`.
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
