//! 021 phase 1 (TA-61): the botarga of a user over HTTP. A new user has one when the login ends, in
//! the same transaction; one of before creates it once with `POST /api/botarga`; `GET` never writes;
//! and `PUT /api/botarga/build` only saves a build of what the user owns that the engine accepts,
//! answering a code and never text. The session's user is the only user a route reads or writes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;

use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use support::app::ORIGIN;
use support::db::TestDb;
use support::fake_provider::Kind;
use support::http;
use support::oauth;
use uuid::Uuid;

const BOTARGA: &str = "/api/botarga";
const BUILD: &str = "/api/botarga/build";

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

async fn get(addr: SocketAddr, token: &str) -> http::Response {
    send(addr, "GET", BOTARGA, token, "").await
}

async fn post(addr: SocketAddr, token: &str) -> http::Response {
    send(addr, "POST", BOTARGA, token, "").await
}

async fn put(addr: SocketAddr, token: &str, build: &Value) -> http::Response {
    send(addr, "PUT", BUILD, token, &build.to_string()).await
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
    // The whole body: nothing of serde, of axum or of the engine next to the code.
    assert_eq!(response.text(), format!(r#"{{"error":"{code}"}}"#));
    assert_private(response);
}

/// The build of a new botarga (021 "Alta").
fn starter_build() -> Value {
    json!({
        "stats": {"hp": 40, "attack": 40, "defense": 40, "speed": 40, "special": 40},
        "style_id": "balanced",
        "abilities": ["foam_headbutt"],
        "accessories": [],
        "cosmetics": {"idle": null, "victory": null},
    })
}

/// [`starter_build`] with `change` applied.
fn build_with(change: impl FnOnce(&mut Value)) -> Value {
    let mut build = starter_build();
    change(&mut build);
    build
}

fn starter_view() -> Value {
    json!({
        "build": starter_build(),
        "progress": {
            "level": 1,
            "xp": 0,
            "xp_to_next": 120,
            "budget_free": 0,
            "slots_open": 1,
            "daily": {"attacks": 0, "factor_pct": 100},
        },
        "unlocks": {"abilities": ["foam_headbutt"], "accessories": [], "cosmetics": []},
        "body_type": "normal",
        "face_id": "rookie",
        "rating": 1000,
        "rated_fights": 0,
    })
}

/// A user with a session and its botarga, created by the route.
async fn user_with_botarga(addr: SocketAddr, pool: &PgPool) -> (Uuid, String) {
    let (user, token) = support::app::user_with_session(pool).await;
    assert_eq!(post(addr, &token).await.status, 200);
    (user, token)
}

async fn unlock(pool: &PgPool, user: Uuid, kind: &str, ids: &[&str]) {
    for id in ids {
        sqlx::query(
            "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) VALUES ($1, $2, $3, 'draw')",
        )
        .bind(user)
        .bind(kind)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn set_level(pool: &PgPool, user: Uuid, level: i16) {
    sqlx::query("UPDATE botargas SET level = $2 WHERE user_id = $1")
        .bind(user)
        .bind(level)
        .execute(pool)
        .await
        .unwrap();
}

/// Everything of the two tables about `user`, as text: what a request must leave untouched. `xmin`
/// changes with any write of the row, even one that sets the same values.
async fn snapshot(pool: &PgPool, user: Uuid) -> Vec<String> {
    let mut rows: Vec<String> =
        sqlx::query("SELECT xmin::text || ' ' || b::text FROM botargas b WHERE user_id = $1")
            .bind(user)
            .fetch_all(pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect();
    let unlocks = sqlx::query(
        "SELECT xmin::text || ' ' || u::text FROM botarga_unlocks u WHERE user_id = $1 \
         ORDER BY kind, item_id",
    )
    .bind(user)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.extend(unlocks.iter().map(|row| row.get::<String, _>(0)));
    rows
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    // `table` is a literal of this file.
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_user_has_a_botarga_when_the_login_ends() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, "g-botarga-new").await);
    let read = get(app.addr, &token).await;
    assert_eq!(read.status, 200, "{}", read.text());
    assert_private(&read);
    assert_eq!(body(&read), starter_view());

    let row = sqlx::query(
        "SELECT b.level, b.xp, b.hp + b.attack + b.defense + b.speed + b.special, b.style_id, \
         b.abilities, b.rating, b.rated_fights, b.created_at = u.created_at \
         FROM botargas b JOIN users u ON u.id = b.user_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i16, _>(0), 1);
    assert_eq!(row.get::<i32, _>(1), 0);
    assert_eq!(row.get::<i16, _>(2), 200);
    assert_eq!(row.get::<String, _>(3), "balanced");
    assert_eq!(row.get::<Vec<String>, _>(4), vec!["foam_headbutt"]);
    assert_eq!(row.get::<i32, _>(5), 1000);
    assert_eq!(row.get::<i32, _>(6), 0);
    // `now()` is the start of the transaction: the same instant means the same transaction.
    assert!(row.get::<bool, _>(7));
    let unlocks = sqlx::query("SELECT kind, item_id, source FROM botarga_unlocks")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(unlocks.len(), 1);
    assert_eq!(unlocks[0].get::<String, _>(0), "ability");
    assert_eq!(unlocks[0].get::<String, _>(1), "foam_headbutt");
    assert_eq!(unlocks[0].get::<String, _>(2), "starter");

    // A second login of the same identity is the same user and the same botarga.
    let again = oauth::session_of(&oauth::login(&app, Kind::Google, "g-botarga-new").await);
    assert_eq!(body(&get(app.addr, &again).await), starter_view());
    assert_eq!(count(&pool, "users").await, 1);
    assert_eq!(count(&pool, "botargas").await, 1);
    assert_eq!(count(&pool, "botarga_unlocks").await, 1);
    // And the `POST` of a user that already has one changes nothing.
    assert_eq!(body(&post(app.addr, &again).await), starter_view());
    assert_eq!(count(&pool, "botarga_unlocks").await, 1);
}

/// The botarga is part of the login transaction: if it cannot be written, there is no user either.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_whose_botarga_fails_creates_no_user() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    sqlx::raw_sql(
        "CREATE FUNCTION test_refuse() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'refused by the test'; END; $$; \
         CREATE TRIGGER test_refuse BEFORE INSERT ON botargas \
         FOR EACH ROW EXECUTE FUNCTION test_refuse();",
    )
    .execute(&pool)
    .await
    .unwrap();
    let failed = oauth::login(&app, Kind::Google, "g-botarga-fails").await;
    assert_eq!(oauth::error_of(&failed), "auth.try_again");
    for table in ["users", "oauth_accounts", "botargas", "sessions"] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    assert_eq!(
        count(&pool, "ledger_accounts WHERE user_id IS NOT NULL").await,
        0
    );
    sqlx::raw_sql("DROP TRIGGER test_refuse ON botargas")
        .execute(&pool)
        .await
        .unwrap();
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, "g-botarga-fails").await);
    assert_eq!(get(app.addr, &token).await.status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_user_of_before_creates_it_once_and_a_get_never_writes() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = support::app::user_with_session(&pool).await;

    // Nothing yet, and reading does not create it.
    for _ in 0..2 {
        assert_code(&get(app.addr, &token).await, 404, "botarga.not_created");
    }
    assert_code(
        &put(app.addr, &token, &starter_build()).await,
        404,
        "botarga.not_created",
    );
    assert_eq!(count(&pool, "botargas").await, 0);
    assert_eq!(count(&pool, "botarga_unlocks").await, 0);

    let created = post(app.addr, &token).await;
    assert_eq!(created.status, 200, "{}", created.text());
    assert_private(&created);
    assert_eq!(body(&created), starter_view());
    let first = snapshot(&pool, user).await;
    assert_eq!(first.len(), 2);

    // Again, alone and eight at once: the same answer, and not one write.
    assert_eq!(body(&post(app.addr, &token).await), starter_view());
    let mut racing = Vec::new();
    for _ in 0..8 {
        let (addr, token) = (app.addr, token.clone());
        racing.push(tokio::spawn(async move { post(addr, &token).await }));
    }
    for request in racing {
        let response = request.await.unwrap();
        assert_eq!(response.status, 200, "{}", response.text());
        assert_eq!(body(&response), starter_view());
    }
    assert_eq!(snapshot(&pool, user).await, first);

    // A `GET` of a botarga that exists does not touch its rows either.
    for _ in 0..3 {
        assert_eq!(body(&get(app.addr, &token).await), starter_view());
    }
    assert_eq!(snapshot(&pool, user).await, first);

    // The `POST` never resets a build that changed.
    let changed = build_with(|b| b["style_id"] = json!("showman"));
    assert_eq!(put(app.addr, &token, &changed).await.status, 200);
    assert_eq!(body(&post(app.addr, &token).await)["build"], changed);
}

/// Two first requests of the same user at once, with no row yet: one botarga, one set of unlocks.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_first_posts_create_one_botarga() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (_, token) = support::app::user_with_session(&pool).await;
    let mut racing = Vec::new();
    for _ in 0..8 {
        let (addr, token) = (app.addr, token.clone());
        racing.push(tokio::spawn(async move { post(addr, &token).await }));
    }
    for request in racing {
        let response = request.await.unwrap();
        assert_eq!(response.status, 200, "{}", response.text());
        assert_eq!(body(&response), starter_view());
    }
    assert_eq!(count(&pool, "botargas").await, 1);
    assert_eq!(count(&pool, "botarga_unlocks").await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_routes_need_a_session_and_the_writes_the_origin() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = support::app::user_with_session(&pool).await;
    let cookie = oauth::session_cookie(&token);
    let json_type = ("content-type", "application/json");
    let build = starter_build().to_string();

    // No session: 401, whatever the method.
    for (method, path, body) in [
        ("GET", BOTARGA, ""),
        ("POST", BOTARGA, ""),
        ("PUT", BUILD, build.as_str()),
    ] {
        let headers = [("origin", ORIGIN), json_type];
        let response = http::send_body(app.addr, method, path, &headers, body).await;
        assert_eq!(response.status, 401, "{method} {path}");
        assert_eq!(response.text(), r#"{"error":"auth.unauthenticated"}"#);
        let forged_cookie = oauth::session_cookie("nope");
        let forged = [
            ("origin", ORIGIN),
            json_type,
            ("cookie", forged_cookie.as_str()),
        ];
        let response = http::send_body(app.addr, method, path, &forged, body).await;
        assert_eq!(response.status, 401, "{method} {path}");
    }
    // A write from another origin, or from none, with a valid session: 403 before the handler.
    for (method, path, body) in [("POST", BOTARGA, ""), ("PUT", BUILD, build.as_str())] {
        for origin in [None, Some("https://evil.example")] {
            let mut headers = vec![("cookie", cookie.as_str()), json_type];
            if let Some(origin) = origin {
                headers.push(("origin", origin));
            }
            let response = http::send_body(app.addr, method, path, &headers, body).await;
            assert_eq!(response.status, 403, "{method} {path} {origin:?}");
            assert_eq!(response.text(), r#"{"error":"auth.csrf_origin"}"#);
        }
    }
    assert_eq!(count(&pool, "botargas").await, 0);
    assert!(snapshot(&pool, user).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_inside_the_rules_is_saved() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    unlock(&pool, user, "ability", &["confetti_cannon", "spicy_slap"]).await;
    unlock(&pool, user, "accessory", &["oven_mitts"]).await;
    unlock(&pool, user, "cosmetic", &["idle_3", "win_2"]).await;
    set_level(&pool, user, 5).await;

    // Level 5 has 204 points and two slots; 4 points stay free.
    let build = json!({
        "stats": {"hp": 60, "attack": 50, "defense": 30, "speed": 30, "special": 30},
        "style_id": "aggressive",
        "abilities": ["spicy_slap", "confetti_cannon"],
        "accessories": ["oven_mitts"],
        "cosmetics": {"idle": "idle_3", "victory": "win_2"},
    });
    let saved = put(app.addr, &token, &build).await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_private(&saved);
    let view = body(&saved);
    assert_eq!(view["build"], build);
    assert_eq!(view["progress"]["level"], 5);
    assert_eq!(view["progress"]["budget_free"], 4);
    assert_eq!(view["progress"]["slots_open"], 2);
    assert_eq!(
        view["unlocks"],
        json!({
            "abilities": ["confetti_cannon", "foam_headbutt", "spicy_slap"],
            "accessories": ["oven_mitts"],
            "cosmetics": ["idle_3", "win_2"],
        })
    );
    // What was saved is what a later read gives, in the order the abilities were sent.
    assert_eq!(body(&get(app.addr, &token).await), view);
    let row = sqlx::query(
        "SELECT hp, attack, defense, speed, special, style_id, abilities, accessories, \
         cosmetic_idle, cosmetic_victory, updated_at > created_at, level, rating \
         FROM botargas WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    let stats: Vec<i16> = (0..5).map(|at| row.get(at)).collect();
    assert_eq!(stats, vec![60, 50, 30, 30, 30]);
    assert_eq!(row.get::<String, _>(5), "aggressive");
    assert_eq!(
        row.get::<Vec<String>, _>(6),
        vec!["spicy_slap", "confetti_cannon"]
    );
    assert_eq!(row.get::<Vec<String>, _>(7), vec!["oven_mitts"]);
    assert_eq!(row.get::<Option<String>, _>(8).as_deref(), Some("idle_3"));
    assert_eq!(row.get::<Option<String>, _>(9).as_deref(), Some("win_2"));
    assert!(row.get::<bool, _>(10));
    // A build never moves the level nor the rating.
    assert_eq!(row.get::<i16, _>(11), 5);
    assert_eq!(row.get::<i32, _>(12), 1000);

    // Back to nothing picked: `null` clears a cosmetic.
    let plain = build_with(|b| b["stats"]["hp"] = json!(44));
    let saved = put(app.addr, &token, &plain).await;
    assert_eq!(body(&saved)["build"], plain, "{}", saved.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_outside_the_rules_is_refused_with_its_code() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    let specials = ["confetti_cannon", "joy_buzzer_hug", "dramatic_pause"];
    unlock(&pool, user, "ability", &specials).await;
    unlock(&pool, user, "ability", &["spicy_slap", "puddle_punt"]).await;
    let heads = ["propeller_beanie", "tinfoil_crown"];
    unlock(&pool, user, "accessory", &heads).await;
    unlock(
        &pool,
        user,
        "accessory",
        &["spandex_bike_shorts", "grandma_roller_skates"],
    )
    .await;
    // An unlock of something the catalog does not have (a retired id) equips nothing.
    unlock(&pool, user, "ability", &["retired_move"]).await;

    let set = |key: &'static str, value: Value| build_with(move |b| b[key] = value);
    let level_1 = [
        // Of the catalog, but not unlocked; and not of the catalog at all: the same code.
        (set("abilities", json!(["wet_noodle_whip"])), "not_unlocked"),
        (set("abilities", json!(["no_such_move"])), "not_unlocked"),
        (set("abilities", json!(["retired_move"])), "not_unlocked"),
        (set("accessories", json!(["oven_mitts"])), "not_unlocked"),
        (
            set("cosmetics", json!({"idle": "idle_2", "victory": null})),
            "not_unlocked",
        ),
        (
            set("cosmetics", json!({"idle": null, "victory": "win_1"})),
            "not_unlocked",
        ),
        (set("style_id", json!("berserk")), "style_unknown"),
        // 201 points at level 1.
        (
            build_with(|b| b["stats"]["hp"] = json!(41)),
            "budget_exceeded",
        ),
        (
            build_with(|b| {
                b["stats"] =
                    json!({"hp": 255, "attack": 255, "defense": 255, "speed": 255, "special": 255})
            }),
            "budget_exceeded",
        ),
        // Under the minimum of 20 of the ruleset.
        (
            build_with(|b| {
                b["stats"]["hp"] = json!(61);
                b["stats"]["special"] = json!(19);
            }),
            "stat_out_of_range",
        ),
        (set("abilities", json!([])), "ability_count"),
        // Two abilities with the one slot of level 1.
        (
            set("abilities", json!(["foam_headbutt", "spicy_slap"])),
            "slots_locked",
        ),
        (set("accessories", json!(heads)), "accessory_slot_duplicate"),
        (
            set(
                "accessories",
                json!([
                    "propeller_beanie",
                    "spandex_bike_shorts",
                    "grandma_roller_skates"
                ]),
            ),
            "accessory_stat_total",
        ),
    ];
    let before = snapshot(&pool, user).await;
    for (build, code) in &level_1 {
        let refused = put(app.addr, &token, build).await;
        assert_code(&refused, 422, &format!("botarga.{code}"));
    }
    assert_eq!(snapshot(&pool, user).await, before);

    // With the three slots of level 15: three specials, a repeated one and three elements.
    set_level(&pool, user, 15).await;
    let before = snapshot(&pool, user).await;
    for (abilities, code) in [
        (json!(specials), "ability_kind_count"),
        (
            json!(["foam_headbutt", "foam_headbutt"]),
            "ability_duplicate",
        ),
        (
            json!(["spicy_slap", "puddle_punt", "joy_buzzer_hug"]),
            "weakness_count",
        ),
    ] {
        let refused = put(app.addr, &token, &set("abilities", abilities)).await;
        assert_code(&refused, 422, &format!("botarga.{code}"));
    }
    assert_eq!(snapshot(&pool, user).await, before);
    assert_eq!(body(&get(app.addr, &token).await)["build"], starter_build());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_that_is_not_a_build_is_422_without_text() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, token) = user_with_botarga(app.addr, &pool).await;
    let before = snapshot(&pool, user).await;
    let long = "a".repeat(65);
    let bodies = [
        String::new(),
        "{".to_owned(),
        "[]".to_owned(),
        r#"{"stats":"canary-in-the-body"}"#.to_owned(),
        // A field the type does not have, at each depth: nobody sets a level, a rating or a user.
        build_with(|b| b["level"] = json!(100)).to_string(),
        build_with(|b| b["rating"] = json!(4000)).to_string(),
        build_with(|b| b["user_id"] = json!(Uuid::new_v4().to_string())).to_string(),
        build_with(|b| b["stats"]["luck"] = json!(1)).to_string(),
        build_with(|b| b["cosmetics"]["ko"] = json!("ko_1")).to_string(),
        // A missing field, a wrong type and a number outside a `u8`.
        build_with(|b| {
            b.as_object_mut().unwrap().remove("style_id");
        })
        .to_string(),
        build_with(|b| b["abilities"] = json!("foam_headbutt")).to_string(),
        build_with(|b| b["stats"]["hp"] = json!(256)).to_string(),
        build_with(|b| b["stats"]["hp"] = json!(-1)).to_string(),
        build_with(|b| b["stats"]["hp"] = json!(40.5)).to_string(),
        // A clip that is not one a player picks.
        build_with(|b| b["cosmetics"]["idle"] = json!("win_1")).to_string(),
        build_with(|b| b["cosmetics"]["victory"] = json!("ko_1")).to_string(),
        // The sizes: an id of 65 bytes, five abilities, six accessories, an empty id.
        build_with(|b| b["abilities"] = json!([long])).to_string(),
        build_with(|b| b["style_id"] = json!(long)).to_string(),
        build_with(|b| b["abilities"] = json!(["a", "b", "c", "d", "e"])).to_string(),
        build_with(|b| b["accessories"] = json!(["a", "b", "c", "d", "e", "f"])).to_string(),
        build_with(|b| b["accessories"] = json!([""])).to_string(),
        // Over the 4 KB of a form.
        build_with(|b| b["style_id"] = json!("s".repeat(5000))).to_string(),
    ];
    for sent in &bodies {
        let refused = send(app.addr, "PUT", BUILD, &token, sent).await;
        assert_code(&refused, 422, "botarga.invalid_request");
    }
    // A build that is fine, sent as something that is not JSON.
    let cookie = oauth::session_cookie(&token);
    let headers = [
        ("origin", ORIGIN),
        ("cookie", cookie.as_str()),
        ("content-type", "text/plain"),
    ];
    let build = starter_build().to_string();
    let refused = http::send_body(app.addr, "PUT", BUILD, &headers, &build).await;
    assert_code(&refused, 422, "botarga.invalid_request");
    assert_eq!(snapshot(&pool, user).await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_user_never_reads_nor_changes_the_botarga_of_another() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (ana, ana_token) = user_with_botarga(app.addr, &pool).await;
    let (beto, beto_token) = user_with_botarga(app.addr, &pool).await;
    // Only Ana owns the fire slap and a cosmetic, and only she is level 5.
    unlock(&pool, ana, "ability", &["spicy_slap"]).await;
    unlock(&pool, ana, "cosmetic", &["win_3"]).await;
    set_level(&pool, ana, 5).await;
    let beto_before = snapshot(&pool, beto).await;

    let hers = json!({
        "stats": {"hp": 44, "attack": 40, "defense": 40, "speed": 40, "special": 40},
        "style_id": "defensive",
        "abilities": ["foam_headbutt", "spicy_slap"],
        "accessories": [],
        "cosmetics": {"idle": null, "victory": "win_3"},
    });
    let saved = put(app.addr, &ana_token, &hers).await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_eq!(body(&saved)["build"], hers);

    // Beto still reads his own, untouched, and nothing of hers.
    assert_eq!(snapshot(&pool, beto).await, beto_before);
    let his = get(app.addr, &beto_token).await;
    assert_eq!(body(&his), starter_view());
    assert!(!his.text().contains("spicy_slap"));
    // What she owns is not his: her build, sent with his session, is refused whole.
    assert_code(
        &put(app.addr, &beto_token, &hers).await,
        422,
        "botarga.not_unlocked",
    );
    for attempt in [
        build_with(|b| b["abilities"] = json!(["spicy_slap"])),
        build_with(|b| b["cosmetics"]["victory"] = json!("win_3")),
        // Her level does not lend him its points.
        build_with(|b| b["stats"]["hp"] = json!(44)),
    ] {
        let refused = put(app.addr, &beto_token, &attempt).await;
        assert_eq!(refused.status, 422, "{}", refused.text());
    }
    // No request names a user: her id in his body is a field the type does not have.
    for key in ["user_id", "id", "user"] {
        let named = build_with(|b| b[key] = json!(ana.to_string()));
        assert_code(
            &put(app.addr, &beto_token, &named).await,
            422,
            "botarga.invalid_request",
        );
    }
    // Nor does a path or a query: there is no route of the botarga of somebody else.
    for path in [
        format!("{BOTARGA}/{ana}"),
        format!("{BOTARGA}/{ana}/build"),
        format!("{BUILD}/{ana}"),
    ] {
        for method in ["GET", "PUT", "POST"] {
            let response = send(app.addr, method, &path, &beto_token, &hers.to_string()).await;
            assert_eq!(response.status, 404, "{method} {path}");
            assert_eq!(response.text(), r#"{"error":"http.not_found"}"#);
        }
    }
    let queried = send(
        app.addr,
        "GET",
        &format!("{BOTARGA}?user_id={ana}"),
        &beto_token,
        "",
    )
    .await;
    assert_eq!(body(&queried), starter_view());
    assert_eq!(snapshot(&pool, beto).await, beto_before);
    // And hers is as she left it. No answer carries a user id.
    let read = get(app.addr, &ana_token).await;
    assert_eq!(body(&read)["build"], hers);
    for text in [read.text(), his.text()] {
        assert!(!text.contains(&ana.to_string()) && !text.contains(&beto.to_string()));
    }
}

/// The second barrier: the `CHECK`s of the table agree with the engine at every level.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_checks_of_the_table_follow_the_engine() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let (user, _) = user_with_botarga(app.addr, &pool).await;
    let constraint = |error: sqlx::Error| {
        error
            .as_database_error()
            .and_then(|e| e.constraint().map(str::to_owned))
    };
    for level in 1..=engine::loadout::LEVEL_MAX {
        let budget = i16::try_from(engine::loadout::budget(level).unwrap()).unwrap();
        for (hp, fits) in [(budget - 160, true), (budget - 159, false)] {
            let written = sqlx::query(
                "UPDATE botargas SET level = $2, hp = $3, attack = 40, defense = 40, speed = 40, \
                 special = 40 WHERE user_id = $1",
            )
            .bind(user)
            .bind(i16::from(level))
            .bind(hp)
            .execute(&pool)
            .await;
            match (fits, written) {
                (true, Ok(_)) => {}
                (false, Err(error)) => {
                    assert_eq!(constraint(error).as_deref(), Some("botargas_stats_ck"));
                }
                (fits, other) => panic!("level {level}, hp {hp}, fits {fits}: {other:?}"),
            }
        }
    }
    for (change, name) in [
        ("level = 0", "botargas_level_ck"),
        ("level = 101", "botargas_level_ck"),
        ("xp = -1", "botargas_xp_ck"),
        ("rating = -1", "botargas_rating_ck"),
        ("rating = 4001", "botargas_rating_ck"),
        ("rated_fights = -1", "botargas_rated_fights_ck"),
        ("hp = -1", "botargas_stats_ck"),
        ("hp = 256, level = 100", "botargas_stats_ck"),
        ("abilities = '{}'", "botargas_abilities_ck"),
        ("abilities = '{a,b,c,d,e}'", "botargas_abilities_ck"),
        ("abilities = ARRAY['a', NULL]", "botargas_abilities_ck"),
        ("accessories = '{a,b,c,d,e,f}'", "botargas_accessories_ck"),
        ("style_id = ''", "botargas_style_ck"),
        ("cosmetic_idle = ''", "botargas_cosmetics_ck"),
    ] {
        let error = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE botargas SET {change} WHERE user_id = $1"
        )))
        .bind(user)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(constraint(error).as_deref(), Some(name), "{change}");
    }
    // One botarga per user, and an unlock only of a kind and a source the table knows.
    let again = sqlx::query(
        "INSERT INTO botargas (user_id, hp, attack, defense, speed, special, style_id, abilities) \
         VALUES ($1, 40, 40, 40, 40, 40, 'balanced', '{foam_headbutt}')",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(constraint(again).as_deref(), Some("botargas_pkey"));
    for (kind, source, name) in [
        ("stat", "starter", "botarga_unlocks_kind_ck"),
        ("ability", "purchase", "botarga_unlocks_source_ck"),
        ("ability", "starter", "botarga_unlocks_pk"),
    ] {
        let error = sqlx::query(
            "INSERT INTO botarga_unlocks (user_id, kind, item_id, source) \
             VALUES ($1, $2, 'foam_headbutt', $3)",
        )
        .bind(user)
        .bind(kind)
        .bind(source)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(constraint(error).as_deref(), Some(name), "{kind} {source}");
    }
}
