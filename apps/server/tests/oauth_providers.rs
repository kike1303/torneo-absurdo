//! 008 RF-C1 and T19: Google and Discord against fake local providers (never the network). The
//! authorization URL asks for exactly one scope, the token request carries Basic and the right
//! `code_verifier`, the first login creates the user with the grant and an alias that has
//! nothing of `userinfo`, and every failure of the provider is `auth.provider_error`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use support::app::ORIGIN;
use support::db::TestDb;
use support::fake_provider::{self as fake, Kind};
use support::oauth;

const KINDS: [Kind; 2] = [Kind::Google, Kind::Discord];

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// A Discord identity is a snowflake; a Google `sub` is any text.
fn identity(kind: Kind) -> String {
    match kind {
        Kind::Google => format!("g-sub-{}", uuid::Uuid::new_v4().simple()),
        Kind::Discord => format!("{}", 100_000_000_000_000_000u64 + u64::from(rand_u16())),
    }
}

fn rand_u16() -> u16 {
    uuid::Uuid::new_v4().as_bytes()[0].into()
}

/// ADR-0012 B4.6: with the production URLs (no fakes), `start` and `link` answer a 303 (never a
/// 307 or 308, which keep the method and the body) whose `Location` is the authorize constant
/// plus the query the server builds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_redirect_to_a_real_provider_is_a_303_to_its_authorize_constant() {
    use server::auth::providers::{DISCORD_AUTHORIZE, GOOGLE_AUTHORIZE};
    let db = TestDb::create().await;
    let (app, pool) = support::app::start_db(
        &db,
        &support::config::PROVIDER_VARS,
        &server::app::Settings::default(),
    )
    .await;
    // A fresh session: `link` needs a confirmation (009 RF8).
    let (_, token) = support::app::user_with_dev_account(&pool).await;
    let session = oauth::session_cookie(&token);
    for (path, cookie, authorize) in [
        ("/api/auth/google/start", None, GOOGLE_AUTHORIZE),
        ("/api/auth/discord/start", None, DISCORD_AUTHORIZE),
        (
            "/api/auth/link/google",
            Some(session.as_str()),
            GOOGLE_AUTHORIZE,
        ),
        (
            "/api/auth/link/discord",
            Some(session.as_str()),
            DISCORD_AUTHORIZE,
        ),
    ] {
        let mut headers = vec![
            ("origin", ORIGIN),
            ("content-type", "application/x-www-form-urlencoded"),
        ];
        headers.extend(cookie.map(|c| ("cookie", c)));
        let response = support::http::send_body(app.addr, "POST", path, &headers, "next=%2F").await;
        assert_eq!(response.status, 303, "{path}: {}", response.text());
        let location = response.header("location").unwrap();
        assert!(
            location.starts_with(&format!("{authorize}?")),
            "{path}: {location}"
        );
    }
}

/// 009 RF11: the same for `reauth`, which needs an account of the provider. Its own server,
/// because the test above already spends most of the AUTH_IP burst (5) of 127.0.0.1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reauth_redirect_to_a_real_provider_is_a_303_to_its_authorize_constant() {
    use server::auth::providers::{DISCORD_AUTHORIZE, GOOGLE_AUTHORIZE};
    let db = TestDb::create().await;
    let (app, pool) = support::app::start_db(
        &db,
        &support::config::PROVIDER_VARS,
        &server::app::Settings::default(),
    )
    .await;
    let (user, token) = support::app::user_with_session(&pool).await;
    let session = oauth::session_cookie(&token);
    for (provider, authorize) in [("google", GOOGLE_AUTHORIZE), ("discord", DISCORD_AUTHORIZE)] {
        sqlx::query(
            "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ($1, $2, $3)",
        )
        .bind(provider)
        .bind(format!("{provider}-{}", user.simple()))
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
        let path = format!("/api/auth/reauth/{provider}");
        let headers = [
            ("origin", ORIGIN),
            ("content-type", "application/x-www-form-urlencoded"),
            ("cookie", session.as_str()),
        ];
        let response =
            support::http::send_body(app.addr, "POST", &path, &headers, "next=%2F").await;
        assert_eq!(response.status, 303, "{path}: {}", response.text());
        let location = response.header("location").unwrap();
        assert!(
            location.starts_with(&format!("{authorize}?")),
            "{path}: {location}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_authorization_url_asks_for_one_scope_with_pkce() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    for kind in KINDS {
        let started = oauth::start(&app, kind, None).await;
        assert_eq!(started.status, 303, "{}", started.text());
        let location = started.header("location").unwrap();
        assert!(
            location.starts_with(&format!("http://{}/authorize?", app.fake(kind).addr)),
            "{location}"
        );
        let query = oauth::query_of(location);
        assert_eq!(query["scope"], kind.scope(), "{kind:?}");
        assert_eq!(query["client_id"], kind.client_id());
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(
            query["redirect_uri"],
            format!("{ORIGIN}/api/auth/{}/callback", kind.name())
        );
        assert_eq!(query["state"].len(), 22);
        assert_eq!(query["code_challenge"].len(), 43);
        // Only these parameters: no `login_hint`, no extra scope, no secret.
        let mut keys: Vec<_> = query.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "client_id",
                "code_challenge",
                "code_challenge_method",
                "redirect_uri",
                "response_type",
                "scope",
                "state"
            ]
        );
        assert!(!location.contains(kind.client_secret()));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_login_creates_the_user_with_the_grant_and_an_unrelated_alias() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    for kind in KINDS {
        let id = identity(kind);
        let started = oauth::start(&app, kind, None).await;
        let challenge =
            oauth::query_of(started.header("location").unwrap())["code_challenge"].clone();
        let done = oauth::finish(&app, kind, &started, &id, None).await;
        assert_eq!(done.header("location"), Some("/cuenta"), "{kind:?}");
        let token = oauth::session_of(&done);
        // The exchange sent Basic with the test credentials and the verifier of the challenge.
        let (verifier, authorization) = app.fake(kind).last_token_request().unwrap();
        assert_eq!(authorization, format!("Basic {}", kind.basic()));
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            challenge
        );
        let me: serde_json::Value =
            serde_json::from_str(&oauth::me(&app, &token).await.text()).unwrap();
        assert_eq!(me["balance"], 100, "{me}");
        assert_eq!(me["providers"], serde_json::json!([kind.name()]));
        let alias = me["display_name"].as_str().unwrap().to_lowercase();
        for field in [
            id.as_str(),
            fake::NAME,
            fake::EMAIL,
            fake::USERNAME,
            fake::GLOBAL_NAME,
            "canary",
        ] {
            let field = field.to_lowercase();
            assert!(
                !alias.contains(&field) && !field.contains(&alias),
                "{alias} {field}"
            );
        }
        let stored: String =
            sqlx::query("SELECT provider_user_id FROM oauth_accounts WHERE provider = $1")
                .bind(kind.name())
                .fetch_one(&pool)
                .await
                .unwrap()
                .get(0);
        assert_eq!(stored, id);
        // The same identity again is the same user.
        let users = count(&pool, "users").await;
        let again = oauth::login(&app, kind, &id).await;
        oauth::session_of(&again);
        assert_eq!(count(&pool, "users").await, users);
    }
    assert_eq!(count(&pool, "users").await, 2);
    assert_eq!(count(&pool, "oauth_accounts").await, 2);
}

/// Only the identity field of `userinfo` is read; nothing else of the provider is stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_of_userinfo_but_the_identity_is_stored() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    for kind in KINDS {
        oauth::session_of(&oauth::login(&app, kind, &identity(kind)).await);
    }
    let dump: String = sqlx::query(
        "SELECT string_agg(t::text, ' ') FROM (SELECT row_to_json(u)::text AS t FROM users u \
         UNION ALL SELECT row_to_json(o)::text FROM oauth_accounts o \
         UNION ALL SELECT row_to_json(e)::text FROM auth_events e) AS rows",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    for canary in [fake::NAME, fake::EMAIL, fake::USERNAME, fake::GLOBAL_NAME] {
        assert!(!dump.contains(canary), "{canary}");
    }
    for kind in KINDS {
        assert!(!dump.contains(&kind.access_token()));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_enabled_providers_follow_the_credentials() {
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app(&db).await;
    let list = support::http::get(app.addr, "/api/auth/providers").await;
    let expected = if cfg!(feature = "dev-auth") {
        r#"["google","discord","dev"]"#
    } else {
        r#"["google","discord"]"#
    };
    assert_eq!(list.text(), expected);
    // Without Discord's pair (loopback), only Google.
    let db = TestDb::create().await;
    let (app, _pool) = oauth::start_app_with(
        &db,
        &[("DISCORD_CLIENT_ID", ""), ("DISCORD_CLIENT_SECRET", "")],
        server::app::Settings::default(),
    )
    .await;
    let list = support::http::get(app.addr, "/api/auth/providers").await;
    let expected = if cfg!(feature = "dev-auth") {
        r#"["google","dev"]"#
    } else {
        r#"["google"]"#
    };
    assert_eq!(list.text(), expected);
    let refused = oauth::start(&app, Kind::Discord, None).await;
    assert_eq!(refused.status, 404);
}

/// The provider's `error` (the user cancelled) never calls the token endpoint.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_authorization_is_provider_error_without_a_token_request() {
    let db = TestDb::create().await;
    let (app, pool) = oauth::start_app(&db).await;
    let started = oauth::start(&app, Kind::Google, None).await;
    let state = oauth::query_of(started.header("location").unwrap())["state"].clone();
    let flow = started.cookie(oauth::flow_name()).unwrap().to_owned();
    let done = oauth::callback(
        &app,
        &format!("/api/auth/google/callback?error=access_denied&state={state}"),
        &flow,
        None,
    )
    .await;
    assert_eq!(oauth::error_of(&done), "auth.provider_error");
    assert_eq!(app.google.token_requests(), 0);
    assert_eq!(count(&pool, "users").await, 0);
}
