//! 008 RF-C6 (T9, the ban criterion of the intent): `server admin grant|revoke|ban|unban`, the
//! real binary against the database of an app running in process. Each command changes `role`
//! or `banned_at`, revokes every session (the app closes the sockets through `NOTIFY`) and writes
//! exactly one row of `admin_audit`, in one transaction. 009 RF17: `grant` keeps the cutoff of
//! the accounts and prints the providers within it, and the help says when to run it again.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use server::app::{AppState, Settings};
use server::http::admin::RecentAdmin;
use sqlx::{PgPool, Row};
use support::app::{AlwaysUp, ORIGIN};
use support::bin;
use support::db::TestDb;
use support::fake_provider::Kind;
use support::oauth;
use support::ws::{self, Ending};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn admin(db: &TestDb, args: &[&str]) -> bin::Finished {
    let url = db.url();
    let mut all = vec!["admin"];
    all.extend_from_slice(args);
    bin::spawn(&all, &[("DATABASE_URL", url.as_str())])
        .finish(Duration::from_secs(20))
        .await
}

fn event_of(finished: &bin::Finished) -> String {
    let last: serde_json::Value = serde_json::from_str(finished.stdout.last().unwrap()).unwrap();
    last["event"].as_str().unwrap().to_owned()
}

async fn user_of(pool: &PgPool, token: &str) -> Uuid {
    sqlx::query("SELECT user_id FROM sessions WHERE token_hash = $1")
        .bind(server::sessions::TokenHash::of(token).0.as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

async fn socket(app: &oauth::App, token: &str) -> ws::Client {
    let cookie = oauth::session_cookie(token);
    let mut client = ws::connect_with(app.addr, ORIGIN, &[("cookie", cookie.as_str())])
        .await
        .unwrap();
    ws::send_text(&mut client, r#"{"type":"echo","payload":"ready"}"#).await;
    assert_eq!(
        ws::next_data(&mut client, Duration::from_secs(5)).await,
        Some(Message::Text(r#"{"type":"echo","payload":"ready"}"#.into()))
    );
    client
}

async fn closes(client: &mut ws::Client) {
    assert_eq!(
        ws::ending(client, Duration::from_secs(5)).await,
        Ending::Close(1008, "auth.unauthenticated".into())
    );
}

async fn alive(app: &oauth::App, token: &str) -> bool {
    oauth::me(app, token).await.status == 200
}

/// The rows of `admin_audit` for `user`: `(actor, admin_id is null, action, payload)`.
async fn audit(pool: &PgPool, user: Uuid) -> Vec<(String, bool, String, serde_json::Value)> {
    sqlx::query(
        "SELECT actor, admin_id IS NULL, action, payload::text FROM admin_audit \
         WHERE target = $1 ORDER BY id",
    )
    .bind(user.to_string())
    .fetch_all(pool)
    .await
    .unwrap()
    .iter()
    .map(|r| {
        (
            r.get(0),
            r.get(1),
            r.get(2),
            serde_json::from_str(&r.get::<String, _>(3)).unwrap(),
        )
    })
    .collect()
}

/// The status `RecentAdmin` gives the session `token` (a route of this test only).
async fn recent_admin(pool: &PgPool, db: &TestDb, token: &str) -> StatusCode {
    use tower::ServiceExt;
    async fn only(_admin: RecentAdmin) -> StatusCode {
        StatusCode::NO_CONTENT
    }
    let url = db.url();
    let config = support::config::config(&[("DATABASE_URL", url.as_str())]);
    let state = AppState::from_config(
        &config,
        &Settings::default(),
        pool.clone(),
        Arc::new(AlwaysUp),
    )
    .unwrap();
    let cookie = format!("{}={token}", state.auth.cookies.session_name());
    let request = Request::builder()
        .uri("/test/recent-admin")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();
    Router::new()
        .route("/test/recent-admin", get(only))
        .with_state(state)
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

/// T9: grant revokes the sessions (and closes the socket), the next login is a 12-hour admin
/// session that `RecentAdmin` lets through, and revoke undoes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grant_and_revoke_change_the_role_and_revoke_every_session() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let identity = format!("g-{}", Uuid::new_v4().simple());
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let user = user_of(&pool, &token).await;
    let mut client = socket(&app, &token).await;
    let user_text = user.to_string();

    let granted = admin(&db, &["grant", &user_text]).await;
    assert_eq!(granted.status.code(), Some(0), "{:?}", granted.stdout);
    assert_eq!(event_of(&granted), "admin.grant_applied");
    closes(&mut client).await;
    assert!(!alive(&app, &token).await);
    let role: String = sqlx::query("SELECT role FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(role, "admin");

    let done = oauth::login(&app, Kind::Google, &identity).await;
    let admin_token = oauth::session_of(&done);
    let line = done
        .set_cookies()
        .into_iter()
        .find(|l| l.starts_with(&format!("{}=", oauth::session_name())) && !l.contains("=;"))
        .unwrap()
        .to_owned();
    assert!(line.contains("Max-Age=43200"), "{line}");
    let me: serde_json::Value =
        serde_json::from_str(&oauth::me(&app, &admin_token).await.text()).unwrap();
    assert_eq!(me["role"], "admin");
    assert_eq!(
        recent_admin(&pool, &db, &admin_token).await,
        StatusCode::NO_CONTENT
    );

    let revoked = admin(&db, &["revoke", &user_text]).await;
    assert_eq!(revoked.status.code(), Some(0));
    assert_eq!(event_of(&revoked), "admin.revoke_applied");
    assert!(!alive(&app, &admin_token).await);
    let after = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    assert_eq!(
        recent_admin(&pool, &db, &after).await,
        StatusCode::FORBIDDEN
    );
    let rows = audit(&pool, user).await;
    assert_eq!(
        rows,
        [
            (
                "cli".to_owned(),
                true,
                "grant".to_owned(),
                serde_json::json!({})
            ),
            (
                "cli".to_owned(),
                true,
                "revoke".to_owned(),
                serde_json::json!({})
            ),
        ]
    );
}

/// The ban criterion of the intent: the sessions give 401, the socket closes, a new login is
/// `auth.banned`; after `unban` the user gets back in. A second ban keeps the first date.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ban_and_unban() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let identity = format!("g-{}", Uuid::new_v4().simple());
    let token = oauth::session_of(&oauth::login(&app, Kind::Google, &identity).await);
    let user = user_of(&pool, &token).await;
    let other = support::app::session_for(&pool, user).await;
    let mut client = socket(&app, &token).await;
    let user_text = user.to_string();

    let banned = admin(&db, &["ban", &user_text, "--reason", "spam en el chat"]).await;
    assert_eq!(banned.status.code(), Some(0), "{:?}", banned.stdout);
    assert_eq!(event_of(&banned), "admin.ban_applied");
    closes(&mut client).await;
    assert!(!alive(&app, &token).await);
    assert!(!alive(&app, &other).await);
    let refused = oauth::login(&app, Kind::Google, &identity).await;
    assert_eq!(oauth::error_of(&refused), "auth.banned");

    let first: String = sqlx::query("SELECT banned_at::text FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    tokio::time::sleep(Duration::from_millis(20)).await;
    let again = admin(&db, &["ban", &user_text, "--reason", "otra vez"]).await;
    assert_eq!(again.status.code(), Some(0));
    let second: String = sqlx::query("SELECT banned_at::text FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(first, second);

    let unbanned = admin(&db, &["unban", &user_text]).await;
    assert_eq!(unbanned.status.code(), Some(0));
    assert_eq!(event_of(&unbanned), "admin.unban_applied");
    let back = oauth::login(&app, Kind::Google, &identity).await;
    assert!(alive(&app, &oauth::session_of(&back)).await);

    let rows = audit(&pool, user).await;
    assert_eq!(
        rows,
        [
            (
                "cli".to_owned(),
                true,
                "ban".to_owned(),
                serde_json::json!({ "reason": "spam en el chat" })
            ),
            (
                "cli".to_owned(),
                true,
                "ban".to_owned(),
                serde_json::json!({ "reason": "otra vez" })
            ),
            (
                "cli".to_owned(),
                true,
                "unban".to_owned(),
                serde_json::json!({})
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_user_or_a_bad_database_url_exit_with_1() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let missing = Uuid::new_v4().to_string();
    for args in [
        vec!["grant", missing.as_str()],
        vec!["ban", missing.as_str(), "--reason", "x"],
    ] {
        let finished = admin(&db, &args).await;
        assert_eq!(finished.status.code(), Some(1), "{args:?}");
        assert_eq!(event_of(&finished), "admin.user_not_found");
        assert!(finished.stderr.is_empty());
    }
    let rows: i64 = sqlx::query("SELECT count(*) FROM admin_audit")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(rows, 0);
    let finished = bin::spawn(
        &["admin", "grant", &missing],
        &[("DATABASE_URL", "mysql://u:c4n4ry@127.0.0.1/x")],
    )
    .finish(Duration::from_secs(10))
    .await;
    assert_eq!(finished.status.code(), Some(1));
    let line: serde_json::Value = serde_json::from_str(&finished.stdout[0]).unwrap();
    assert_eq!(line["event"], "config.invalid");
    assert_eq!(line["var"], "DATABASE_URL");
    assert!(finished.stdout.iter().all(|l| !l.contains("c4n4ry")));
    // The CLI never needs the secrets of `serve`: without them it still runs.
    let finished = bin::spawn(
        &["admin", "grant", &missing],
        &[
            ("DATABASE_URL", db.url().as_str()),
            ("INTERNAL_API_SECRET", ""),
            ("IP_HASH_KEY", ""),
        ],
    )
    .finish(Duration::from_secs(20))
    .await;
    assert_eq!(finished.status.code(), Some(1));
    assert_eq!(event_of(&finished), "admin.user_not_found");
}

const PROVIDERS_LINE: &str = "providers within the admin cutoff: ";

/// The line of providers of a `grant`. It comes before the event, is not JSON and is not a log.
fn providers_line(finished: &bin::Finished) -> Option<&str> {
    finished
        .stdout
        .iter()
        .find_map(|line| line.strip_prefix(PROVIDERS_LINE))
}

async fn cutoff_of(pool: &PgPool, user: Uuid) -> Option<i64> {
    sqlx::query("SELECT admin_account_cutoff FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

async fn max_account(pool: &PgPool, user: Uuid) -> Option<i64> {
    sqlx::query("SELECT max(id) FROM oauth_accounts WHERE user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// 009 RF17: `grant` keeps the `max(id)` as the cutoff and says which providers it covers, a
/// second `grant` updates it, `revoke` clears it, and nothing of that line reaches an event.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grant_keeps_the_cutoff_and_prints_the_providers_within_it() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let first = oauth::session_of(
        &oauth::login(
            &app,
            Kind::Google,
            &format!("g-{}", Uuid::new_v4().simple()),
        )
        .await,
    );
    let user = user_of(&pool, &first).await;
    oauth::session_of(&oauth::link(&app, Kind::Discord, "180351110224678914", &first).await);
    let user_text = user.to_string();

    let granted = admin(&db, &["grant", &user_text]).await;
    assert_eq!(granted.status.code(), Some(0), "{:?}", granted.stdout);
    assert_eq!(providers_line(&granted), Some("google, discord"));
    assert_eq!(event_of(&granted), "admin.grant_applied");
    let mut lines = 0;
    for line in &granted.stdout {
        if !line.starts_with(PROVIDERS_LINE) {
            let event: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(!event.to_string().contains("discord"), "{line}");
            lines += 1;
        }
    }
    assert!(lines >= 1);
    assert_eq!(cutoff_of(&pool, user).await, max_account(&pool, user).await);
    assert!(cutoff_of(&pool, user).await.is_some());

    // A provider linked after the grant is outside until a second one.
    sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ('dev', 'x', $1)",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    assert_ne!(cutoff_of(&pool, user).await, max_account(&pool, user).await);
    let again = admin(&db, &["grant", &user_text]).await;
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(providers_line(&again), Some("google, discord, dev"));
    assert_eq!(cutoff_of(&pool, user).await, max_account(&pool, user).await);

    let revoked = admin(&db, &["revoke", &user_text]).await;
    assert_eq!(revoked.status.code(), Some(0));
    assert_eq!(providers_line(&revoked), None);
    assert_eq!(cutoff_of(&pool, user).await, None);
    // `ban` and `unban` print no line either, and keep the cutoff out of it.
    for args in [
        vec!["ban", user_text.as_str(), "--reason", "x"],
        vec!["unban", user_text.as_str()],
    ] {
        let finished = admin(&db, &args).await;
        assert_eq!(finished.status.code(), Some(0));
        assert_eq!(providers_line(&finished), None);
    }

    // A user without accounts: `none`.
    let (lonely, _) = support::app::user_with_session(&pool).await;
    let granted = admin(&db, &["grant", &lonely.to_string()]).await;
    assert_eq!(granted.status.code(), Some(0));
    assert_eq!(providers_line(&granted), Some("none"));
    assert_eq!(cutoff_of(&pool, lonely).await, None);
}

/// 009 RF17: the help says when to run `grant` again and exits with 0 without connecting.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_help_explains_when_to_grant_again_without_connecting() {
    bin::warm_up().await;
    for args in [
        vec!["admin", "--help"],
        vec!["admin", "grant", "--help"],
        vec!["admin", "revoke", "--help"],
    ] {
        for envs in [
            vec![("DATABASE_URL", "mysql://u:c4n4ry@127.0.0.1/x")],
            vec![("DATABASE_URL", "")],
        ] {
            let finished = bin::spawn(&args, &envs)
                .finish(Duration::from_secs(10))
                .await;
            assert_eq!(finished.status.code(), Some(0), "{args:?}");
            let text = finished.stdout.join("\n");
            assert_eq!(text.trim(), server::admin::HELP.trim());
            assert!(finished.stderr.is_empty());
            assert!(!text.contains("c4n4ry"));
        }
    }
    let help = server::admin::HELP;
    for words in [
        "after migrating to 009",
        "after linking a new provider as an admin",
        "/account",
        "revoke",
        "grant",
        "ban",
        "unban",
    ] {
        assert!(help.contains(words), "{words}");
    }
    // Still exit 2 for what is not a help.
    for args in [
        vec!["admin", "--help", "extra"],
        vec!["admin", "promote", "--help"],
        vec!["admin", "grant", "--help", "extra"],
    ] {
        let finished = bin::spawn(&args, &[("DATABASE_URL", "mysql://x")])
            .finish(Duration::from_secs(10))
            .await;
        assert_eq!(finished.status.code(), Some(2), "{args:?}");
    }
}
