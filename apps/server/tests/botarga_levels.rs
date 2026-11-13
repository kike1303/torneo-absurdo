//! 021 phase 3 (TA-62): levels, XP and the draws of new attacks. The XP of a fight crosses levels in
//! the transaction of the fight, and every fifth level writes its draw with the options already
//! taken among what the user does not own; a reload reads the same ones; `draw/choose` unlocks one
//! option once, and the database refuses any other change to a draw.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashSet;
use std::net::SocketAddr;

use serde_json::{Value, json};
use server::botarga::{BotargaCatalog, store, xp_to_next};
use sqlx::{PgPool, Row};
use support::app::ORIGIN;
use support::db::TestDb;
use support::http;
use support::oauth;
use uuid::Uuid;

const BOTARGA: &str = "/api/botarga";
const CHOOSE: &str = "/api/botarga/draw/choose";

async fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: &str,
    body: &str,
) -> http::Response {
    let cookie = oauth::session_cookie(token);
    http::send_body(
        addr,
        method,
        path,
        &[
            ("origin", ORIGIN),
            ("cookie", cookie.as_str()),
            ("content-type", "application/json"),
        ],
        body,
    )
    .await
}

async fn view(addr: SocketAddr, token: &str) -> Value {
    let response = send(addr, "GET", BOTARGA, token, "").await;
    assert_eq!(response.status, 200, "{}", response.text());
    serde_json::from_str(&response.text()).unwrap()
}

async fn choose(addr: SocketAddr, token: &str, draw_id: &Value, item: &str) -> http::Response {
    let body = json!({"draw_id": draw_id, "item_id": item}).to_string();
    send(addr, "POST", CHOOSE, token, &body).await
}

fn assert_code(response: &http::Response, status: u16, code: &str) {
    assert_eq!(response.status, status, "{}", response.text());
    assert_eq!(response.text(), format!(r#"{{"error":"{code}"}}"#));
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
}

async fn user_with_botarga(addr: SocketAddr, pool: &PgPool) -> (Uuid, String) {
    let (user, token) = support::app::user_with_session(pool).await;
    assert_eq!(send(addr, "POST", BOTARGA, &token, "").await.status, 200);
    (user, token)
}

/// The XP from `from` to the start of `to`.
fn xp_between(from: u8, to: u8) -> u32 {
    (from..to).map(xp_to_next).sum()
}

async fn grant(pool: &PgPool, user: Uuid, xp: u32) {
    let catalog = BotargaCatalog::embedded().unwrap();
    let mut tx = pool.begin().await.unwrap();
    store::grant_xp(&mut tx, &catalog, user, xp).await.unwrap();
    tx.commit().await.unwrap();
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn xp_crosses_levels_and_draws_attacks_the_user_does_not_own() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    let catalog = BotargaCatalog::embedded().unwrap();

    grant(&pool, user, 100).await;
    let v = view(app.addr, &token).await;
    assert_eq!(v["progress"]["level"], 1);
    assert_eq!(v["progress"]["xp"], 100);
    assert_eq!(v["draws"], json!([]));

    // To level 5 and 7 more.
    grant(&pool, user, xp_between(1, 5) - 100 + 7).await;
    let v = view(app.addr, &token).await;
    assert_eq!(v["progress"]["level"], 5);
    assert_eq!(v["progress"]["xp"], 7);
    assert_eq!(v["progress"]["xp_to_next"], 200);
    // The second slot opens, and the budget of level 5 (204) is not spent yet.
    assert_eq!(v["progress"]["slots_open"], 2);
    assert_eq!(v["progress"]["budget_free"], 4);
    let draws = v["draws"].as_array().unwrap();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0]["level"], 5);
    assert_eq!(draws[0]["kind"], "ability");
    let options = strings(&draws[0]["options"]);
    assert_eq!(options.len(), 3);
    assert_eq!(options.iter().collect::<HashSet<_>>().len(), 3);
    let drawable = catalog.drawable_abilities(5);
    for option in &options {
        assert_ne!(option, "foam_headbutt", "an owned attack is never offered");
        assert!(drawable.contains(&option.as_str()), "{option}: tier 1 only");
    }
    // A reload reads the same draw: nothing draws again.
    assert_eq!(view(app.addr, &token).await["draws"], v["draws"]);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM botarga_draws")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_choice_unlocks_and_equips_once() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    grant(&pool, user, xp_between(1, 5)).await;
    let v = view(app.addr, &token).await;
    let draw = &v["draws"][0];
    let options = strings(&draw["options"]);
    let item = options[0].as_str();

    let response = choose(app.addr, &token, &draw["id"], item).await;
    assert_eq!(response.status, 200, "{}", response.text());
    let chosen: Value = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(chosen["draws"], json!([]));
    assert!(strings(&chosen["unlocks"]["abilities"]).contains(&item.to_owned()));
    // Level 5 opened a second slot: the attack is equipped after the starter.
    assert_eq!(
        strings(&chosen["build"]["abilities"]),
        vec!["foam_headbutt".to_owned(), item.to_owned()]
    );

    // The same option again: the botarga as it is, nothing new.
    let again = choose(app.addr, &token, &draw["id"], item).await;
    assert_eq!(again.status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&again.text()).unwrap(),
        chosen
    );
    // Another option of the same draw, after the choice: 409.
    assert_code(
        &choose(app.addr, &token, &draw["id"], &options[1]).await,
        409,
        "botarga.draw_already_chosen",
    );
    let source: String = sqlx::query_scalar(
        "SELECT source FROM botarga_unlocks WHERE user_id = $1 AND item_id = $2",
    )
    .bind(user)
    .bind(item)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(source, "draw");
    let unlocked: i64 =
        sqlx::query_scalar("SELECT count(*) FROM botarga_unlocks WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unlocked, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_choice_outside_the_draw_is_refused_with_its_code() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    let (other, other_token) = user_with_botarga(app.addr, &pool).await;
    grant(&pool, user, xp_between(1, 5)).await;
    grant(&pool, other, xp_between(1, 5)).await;
    let mine = view(app.addr, &token).await["draws"][0].clone();
    let theirs = view(app.addr, &other_token).await["draws"][0].clone();
    let their_item = strings(&theirs["options"])[0].clone();

    // Another user's draw and a missing one answer the same.
    assert_code(
        &choose(app.addr, &token, &theirs["id"], &their_item).await,
        404,
        "botarga.draw_not_found",
    );
    assert_code(
        &choose(app.addr, &token, &json!(2_147_483_647_u32), "x").await,
        404,
        "botarga.draw_not_found",
    );
    assert_code(
        &choose(app.addr, &token, &json!(4_000_000_000_u32), "x").await,
        404,
        "botarga.draw_not_found",
    );
    // An owned attack, or anything else, that is not an option.
    for item in ["foam_headbutt", "propeller_beanie", "idle_2"] {
        assert_code(
            &choose(app.addr, &token, &mine["id"], item).await,
            422,
            "botarga.draw_option_invalid",
        );
    }
    // Bodies that are not a choice.
    for bad in [
        String::new(),
        "{}".to_owned(),
        json!({"draw_id": mine["id"], "item_id": ""}).to_string(),
        json!({"draw_id": mine["id"], "item_id": "x".repeat(65)}).to_string(),
        json!({"draw_id": -1, "item_id": "x"}).to_string(),
        json!({"draw_id": mine["id"], "item_id": "x", "user_id": other.to_string()}).to_string(),
    ] {
        assert_code(
            &send(app.addr, "POST", CHOOSE, &token, &bad).await,
            422,
            "botarga.invalid_request",
        );
    }
    // Without the origin, or without a session.
    let body = json!({"draw_id": mine["id"], "item_id": "x"}).to_string();
    let no_origin = http::send_body(
        app.addr,
        "POST",
        CHOOSE,
        &[
            ("cookie", oauth::session_cookie(&token).as_str()),
            ("content-type", "application/json"),
        ],
        &body,
    )
    .await;
    assert_eq!(no_origin.status, 403);
    let anonymous = http::send_body(
        app.addr,
        "POST",
        CHOOSE,
        &[("origin", ORIGIN), ("content-type", "application/json")],
        &body,
    )
    .await;
    assert_eq!(anonymous.status, 401);
    // Nothing of that chose anything.
    let chosen: i64 =
        sqlx::query_scalar("SELECT count(*) FROM botarga_draws WHERE chosen IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(chosen, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_levels_in_a_row_queue_the_second_draw_until_the_first_is_chosen() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    grant(&pool, user, xp_between(1, 10)).await;
    let v = view(app.addr, &token).await;
    assert_eq!(v["progress"]["level"], 10);
    // One draw open, the other queued without options.
    let draws = v["draws"].as_array().unwrap();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0]["level"], 5);
    assert_eq!(v["queued_draws"], 1);
    let queued: i32 =
        sqlx::query_scalar("SELECT id FROM botarga_draws WHERE user_id = $1 AND options IS NULL")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    // A queued draw cannot be chosen yet.
    assert_code(
        &choose(app.addr, &token, &json!(queued), "x").await,
        404,
        "botarga.draw_not_found",
    );
    let first = strings(&draws[0]["options"]);
    let item = first[0].clone();
    let response = choose(app.addr, &token, &draws[0]["id"], &item).await;
    assert_eq!(response.status, 200, "{}", response.text());
    let after: Value = serde_json::from_str(&response.text()).unwrap();
    // The choice opens the next draw, which may offer what the first one did not give.
    assert_eq!(after["queued_draws"], 0);
    let next = after["draws"].as_array().unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0]["level"], 10);
    assert_eq!(next[0]["id"], json!(queued));
    let options = strings(&next[0]["options"]);
    assert_eq!(options.len(), 3);
    assert!(!options.contains(&item), "the chosen attack is owned now");
    assert!(!options.contains(&"foam_headbutt".to_owned()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_attacks_left_a_draw_offers_cosmetics() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    let catalog = BotargaCatalog::embedded().unwrap();
    for id in catalog.drawable_abilities(100) {
        sqlx::query(
            "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) \
             VALUES ($1, 'ability', $2, 'draw') ON CONFLICT DO NOTHING",
        )
        .bind(user)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    grant(&pool, user, xp_between(1, 5)).await;
    let v = view(app.addr, &token).await;
    let draw = &v["draws"][0];
    assert_eq!(draw["kind"], "cosmetic");
    let item = strings(&draw["options"])[0].clone();
    let before = v["build"].clone();
    let response = choose(app.addr, &token, &draw["id"], &item).await;
    assert_eq!(response.status, 200, "{}", response.text());
    let after: Value = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(strings(&after["unlocks"]["cosmetics"]), vec![item]);
    // A cosmetic is not equipped by the choice.
    assert_eq!(after["build"], before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_last_level_keeps_no_xp_and_every_draw_offers_something_new() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    grant(&pool, user, u32::MAX).await;
    let v = view(app.addr, &token).await;
    assert_eq!(v["progress"]["level"], 100);
    assert_eq!(v["progress"]["xp"], 0);
    assert_eq!(v["progress"]["xp_to_next"], 0);
    assert_eq!(v["progress"]["slots_open"], 4);
    assert_eq!(v["progress"]["budget_free"], 100);
    assert_eq!(v["draws"][0]["level"], 5);
    assert_eq!(v["queued_draws"], 19);
    // More XP at the last level changes nothing.
    grant(&pool, user, 1000).await;
    assert_eq!(view(app.addr, &token).await, v);
    // Choosing the first option of each draw walks the queue: every level in order, never an
    // option the user owns, and a level with nothing left to offer is dropped.
    let mut owned: HashSet<String> = HashSet::from(["foam_headbutt".to_owned()]);
    let mut levels = Vec::new();
    let mut v = v;
    while let Some(draw) = v["draws"].as_array().unwrap().first().cloned() {
        levels.push(draw["level"].as_u64().unwrap());
        let options = strings(&draw["options"]);
        for option in &options {
            assert!(!owned.contains(option), "{option} is owned");
        }
        let response = choose(app.addr, &token, &draw["id"], &options[0]).await;
        assert_eq!(response.status, 200, "{}", response.text());
        owned.insert(options[0].clone());
        v = serde_json::from_str(&response.text()).unwrap();
    }
    assert_eq!(v["queued_draws"], 0);
    assert!(levels.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(levels.iter().all(|level| level % 5 == 0));
    // The catalog and the 8 clips cover the 20 draws: none was dropped.
    assert_eq!(levels.len(), 20);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_database_guards_the_draws_and_the_xp() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    grant(&pool, user, xp_between(1, 5)).await;
    let draw = view(app.addr, &token).await["draws"][0].clone();
    let id = i32::try_from(draw["id"].as_i64().unwrap()).unwrap();
    let options = strings(&draw["options"]);

    let code = |error: sqlx::Error| {
        error
            .as_database_error()
            .and_then(|e| e.code().map(|c| c.into_owned()))
            .unwrap()
    };
    let fails = |sql: &'static str| {
        let pool = pool.clone();
        async move { code(sqlx::query(sql).execute(&pool).await.unwrap_err()) }
    };
    // Not an option: the CHECK.
    let err = sqlx::query("UPDATE botarga_draws SET chosen = 'x', chosen_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(code(err), "23514");
    // Other columns, a delete and a truncate: the guard.
    for sql in [
        "UPDATE botarga_draws SET options = ARRAY['a']",
        "UPDATE botarga_draws SET level = 10",
        "DELETE FROM botarga_draws",
        "TRUNCATE botarga_draws",
    ] {
        assert_eq!(fails(sql).await, "TA012", "{sql}");
    }
    // A second draw of the same level.
    let err = sqlx::query(
        "INSERT INTO botarga_draws (user_id, level, kind, options) VALUES ($1, 5, 'ability', ARRAY['a'])",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(err), "23505");
    // A second open draw, of another level.
    let err = sqlx::query(
        "INSERT INTO botarga_draws (user_id, level, kind, options) VALUES ($1, 10, 'ability', ARRAY['a'])",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(err), "23505");
    // A queued draw: no options, nothing chosen, and it is the only row a delete may remove.
    for sql in [
        "INSERT INTO botarga_draws (user_id, level, kind) VALUES ($1, 10, 'ability')",
        "INSERT INTO botarga_draws (user_id, level, chosen, chosen_at) VALUES ($1, 10, 'a', now())",
    ] {
        let err = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(code(err), "23514", "{sql}");
    }
    sqlx::query("INSERT INTO botarga_draws (user_id, level) VALUES ($1, 10)")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let deleted = sqlx::query("DELETE FROM botarga_draws WHERE user_id = $1 AND level = 10")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected(), 1);
    // Chosen once; then no change at all.
    sqlx::query("UPDATE botarga_draws SET chosen = $2, chosen_at = now() WHERE id = $1")
        .bind(id)
        .bind(&options[0])
        .execute(&pool)
        .await
        .unwrap();
    let err = sqlx::query("UPDATE botarga_draws SET chosen = $2 WHERE id = $1")
        .bind(id)
        .bind(&options[1])
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(code(err), "TA012");
    // The XP stays inside its level, and at 0 at the last one.
    for sql in [
        "UPDATE botargas SET xp = 200 WHERE level = 5",
        "UPDATE botargas SET level = 100, xp = 1",
    ] {
        assert_eq!(fails(sql).await, "23514", "{sql}");
    }
    let rows = sqlx::query("SELECT level, xp FROM botargas WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((rows.get::<i16, _>(0), rows.get::<i32, _>(1)), (5, 0));
}
