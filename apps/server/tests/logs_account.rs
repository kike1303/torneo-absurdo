//! 008 RF-B17 extended to phase C (plan-c decision 12, ADR-0012 A8): a login with the fake
//! Google, a failed one, linking the fake Discord, a rename that collides and one that wins, the
//! preferences, a broken JSON body, an unlink and a logout, in the three log levels of phase A.
//! No credential, token, identity, profile field or identifier of the run reaches the log, and
//! every own line has only allowed fields. 009 (plan decision 11): also a link refused without a
//! confirmation, `reauth-status`, a refused unlink, and a reauth that succeeds, one that comes
//! back with another user's identity and one whose transaction fails (`auth.reauth_failed`).
//!
//! The fakes are only injectable by `Settings`, so this runs in process with the server's real
//! log layer (`support::logs`), in its own binary with a single test (plan-c deviation 7).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex};

use base64::Engine as _;
use server::sessions::TokenHash;
use server::telemetry::ALLOWED_FIELDS;
use sqlx::Row;
use support::app::ORIGIN;
use support::db::TestDb;
use support::fake_provider::{self as fake, Kind, Mode};
use support::http;
use support::oauth;

const LEVELS: [&str; 3] = ["info", "debug", "debug,sqlx=trace"];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The values of one run that must never be logged, filled while it runs.
type Canaries = Arc<Mutex<Vec<String>>>;

async fn flow(canaries: Canaries) {
    let add = |value: String| canaries.lock().unwrap().push(value);
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    for kind in [Kind::Google, Kind::Discord] {
        add(kind.client_secret().to_owned());
        add(kind.basic());
        add(kind.access_token());
    }
    for value in [fake::NAME, fake::EMAIL, fake::USERNAME, fake::GLOBAL_NAME] {
        add(value.to_owned());
    }

    // A failed login (the provider answers 500): `auth.provider_failed` runs.
    app.google.set_mode(Mode::UserinfoStatus500);
    let failed = oauth::login(&app, Kind::Google, "g-sub-failed-c4n4ry").await;
    assert_eq!(oauth::error_of(&failed), "auth.provider_error");
    app.google.set_mode(Mode::Ok);

    // The login, with its flow cookie, `state`, `code` and `code_verifier` as canaries.
    let sub = format!("g-sub-c4n4ry-{}", uuid::Uuid::new_v4().simple());
    add(sub.clone());
    let started = oauth::start(&app, Kind::Google, None).await;
    add(started.cookie(oauth::flow_name()).unwrap().to_owned());
    let query = oauth::query_of(started.header("location").unwrap());
    add(query["state"].clone());
    let code = app.google.code_for(&query["code_challenge"], &sub);
    add(code.clone());
    let flow_cookie = started.cookie(oauth::flow_name()).unwrap().to_owned();
    let done = oauth::callback(
        &app,
        &format!(
            "/api/auth/google/callback?code={code}&state={}",
            query["state"]
        ),
        &flow_cookie,
        None,
    )
    .await;
    let token = oauth::session_of(&done);
    add(app.google.last_token_request().unwrap().0);
    add(token.clone());

    // Link the fake Discord (a snowflake as the identity).
    let discord = "180351110224678912".to_owned();
    add(discord.clone());
    let linked = oauth::link(&app, Kind::Discord, &discord, &token).await;
    let token = oauth::session_of(&linked);
    add(token.clone());
    // The rotated session has no confirmation (009 RF10): a reauthentication with Google before
    // the unlink at the end.
    let token = oauth::session_of(&oauth::reauth(&app, Kind::Google, &sub, &token).await);
    add(token.clone());

    // Another user holds a name; the rename to it collides, then one wins.
    let taken = server::profile::name::validate("Canario Ocupado").unwrap();
    add(taken.as_str().to_owned());
    add(taken.key().to_owned());
    sqlx::query("INSERT INTO users (display_name, display_name_key) VALUES ($1, $2)")
        .bind(taken.as_str())
        .bind(taken.key())
        .execute(&pool)
        .await
        .unwrap();
    let collided = oauth::send(
        &app,
        "PUT",
        "/api/me/display-name",
        &token,
        r#"{"display_name":"Canario Ocupado"}"#,
    )
    .await;
    assert_eq!(collided.status, 409);
    let new_name = "Canario Ganador";
    add(new_name.to_owned());
    add(server::profile::name::validate(new_name)
        .unwrap()
        .key()
        .to_owned());
    let renamed = oauth::send(
        &app,
        "PUT",
        "/api/me/display-name",
        &token,
        &serde_json::json!({ "display_name": new_name }).to_string(),
    )
    .await;
    assert_eq!(renamed.status, 200);
    let preferences = oauth::send(
        &app,
        "PUT",
        "/api/me/preferences",
        &token,
        r#"{"locale":"es","theme":"dark"}"#,
    )
    .await;
    assert_eq!(preferences.status, 200);
    // A broken JSON body with a canary (RF-C5): never in the log either.
    let broken = format!("c4n4ry-json-{}", uuid::Uuid::new_v4().simple());
    add(broken.clone());
    let refused = oauth::send(
        &app,
        "PUT",
        "/api/me/display-name",
        &token,
        &format!(r#"{{"display_name": "{broken}"#),
    )
    .await;
    assert_eq!(refused.status, 422);

    // 009: the session 11 minutes old. A link is refused before any flow, and so is an unlink.
    oauth::age(&pool, &token, 11).await;
    let refused = oauth::start_link(&app, Kind::Discord, &token).await;
    assert_eq!(
        refused.header("location"),
        Some("/login?error=auth.reauth_required")
    );
    let status = oauth::reauth_status(&app, &token).await;
    assert_eq!(status.status, 200);
    let refused = oauth::send(&app, "POST", "/api/auth/unlink/discord", &token, "").await;
    assert_eq!(refused.status, 403);

    // A reauth that comes back with another user's identity: nothing changes.
    let other_sub = format!("g-other-c4n4ry-{}", uuid::Uuid::new_v4().simple());
    add(other_sub.clone());
    let other = oauth::session_of(&oauth::login(&app, Kind::Google, &other_sub).await);
    add(other.clone());
    let row = sqlx::query("SELECT user_id, id FROM sessions WHERE token_hash = $1")
        .bind(TokenHash::of(&other).0.as_slice())
        .fetch_one(&pool)
        .await
        .unwrap();
    let other_user: uuid::Uuid = row.get(0);
    let other_session: uuid::Uuid = row.get(1);
    add(other_user.to_string());
    add(other_user.simple().to_string());
    add(other_session.to_string());
    let mismatch = oauth::reauth(&app, Kind::Google, &other_sub, &token).await;
    assert_eq!(oauth::error_of(&mismatch), "auth.reauth_mismatch");

    // A reauth whose transaction fails: `auth.reauth_failed`, and the session stays.
    sqlx::query(
        "CREATE FUNCTION test_reauth_fails() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
         IF NEW.kind = 'reauth' THEN \
         RAISE EXCEPTION USING ERRCODE = 'XX000', MESSAGE = 'test failure'; \
         END IF; RETURN NEW; END; $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER test_reauth_fails BEFORE INSERT ON auth_events \
         FOR EACH ROW EXECUTE FUNCTION test_reauth_fails()",
    )
    .execute(&pool)
    .await
    .unwrap();
    let failed = oauth::reauth(&app, Kind::Google, &sub, &token).await;
    assert_eq!(oauth::error_of(&failed), "auth.try_again");
    sqlx::query("DROP TRIGGER test_reauth_fails ON auth_events")
        .execute(&pool)
        .await
        .unwrap();

    // The reauth that succeeds, with its flow cookie, `state`, `code` and `code_verifier`.
    let started = oauth::start_reauth(&app, Kind::Google, &token).await;
    add(started.cookie(oauth::flow_name()).unwrap().to_owned());
    let query = oauth::query_of(started.header("location").unwrap());
    add(query["state"].clone());
    let code = app.google.code_for(&query["code_challenge"], &sub);
    add(code.clone());
    let flow_cookie = started.cookie(oauth::flow_name()).unwrap().to_owned();
    let done = oauth::callback(
        &app,
        &format!(
            "/api/auth/google/callback?code={code}&state={}",
            query["state"]
        ),
        &flow_cookie,
        Some(&token),
    )
    .await;
    let token = oauth::session_of(&done);
    add(app.google.last_token_request().unwrap().0);
    add(token.clone());

    // The identifiers of the run, read before the logout deletes the session.
    let row =
        sqlx::query("SELECT s.user_id, s.id, s.ip_hash FROM sessions s WHERE s.token_hash = $1")
            .bind(TokenHash::of(&token).0.as_slice())
            .fetch_one(&pool)
            .await
            .unwrap();
    let user: uuid::Uuid = row.get(0);
    let session: uuid::Uuid = row.get(1);
    let ip_hash: Vec<u8> = row.get(2);
    add(user.to_string());
    add(user.simple().to_string());
    add(session.to_string());
    add(hex(&ip_hash));
    add(base64::engine::general_purpose::STANDARD.encode(&ip_hash));

    // A statement over sqlx's 1 s threshold, on a worker thread: its `slow statement` warning
    // is the positive control of a third-party line (its text is dropped by the layer).
    let slow = pool.clone();
    tokio::spawn(async move {
        sqlx::query("SELECT pg_sleep(1.1)")
            .execute(&slow)
            .await
            .unwrap();
    })
    .await
    .unwrap();

    let cookie = oauth::session_cookie(&token);
    let unlinked = http::send(
        app.addr,
        "POST",
        "/api/auth/unlink/discord",
        &[("origin", ORIGIN), ("cookie", cookie.as_str())],
    )
    .await;
    assert_eq!(unlinked.status, 303);
    let out = http::send(
        app.addr,
        "POST",
        "/api/auth/logout",
        &[("origin", ORIGIN), ("cookie", cookie.as_str())],
    )
    .await;
    assert_eq!(out.status, 303);
}

#[test]
fn no_secret_identity_or_profile_field_reaches_the_log() {
    for level in LEVELS {
        let canaries: Canaries = Arc::default();
        let filled = canaries.clone();
        let logs = support::logs::run(level, |_| flow(filled));
        let text = logs.text();
        let canaries = canaries.lock().unwrap();
        assert!(canaries.len() > 25, "{}", canaries.len());
        for canary in canaries.iter() {
            assert!(
                !text.contains(canary.as_str()),
                "{level}: {canary} in the log"
            );
        }
        assert!(!text.contains("c4n4ry"), "{level}");
        let lines = logs.lines();
        // Positive controls: the run was logged, its events among them.
        for event in ["http.request", "auth.provider_failed", "auth.reauth_failed"] {
            assert!(
                lines.iter().any(|l| l["event"] == event),
                "{level}: no {event}"
            );
        }
        // And a third-party line (`target` of sqlx, capped at `warn`), emitted on a worker
        // thread: the capture sees other crates too, so `debug,sqlx=trace` is really tested
        // (nit of #55).
        assert!(
            lines.iter().any(|l| l["target"]
                .as_str()
                .is_some_and(|t| t == "sqlx" || t.starts_with("sqlx::"))
                && l["event"] == "external"),
            "{level}: no sqlx line"
        );
        for line in &lines {
            for key in line.as_object().unwrap().keys() {
                assert!(
                    ALLOWED_FIELDS.contains(&key.as_str())
                        || ["ts", "level", "target"].contains(&key.as_str()),
                    "{level}: {key} in {line}"
                );
            }
        }
    }
}
