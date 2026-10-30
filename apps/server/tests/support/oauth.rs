//! The OAuth flow against the fake Google and Discord (008 phase C), step by step: `start` (or
//! `link`, or `reauth` of 009), the code of the fake for the challenge of the `Location`, and the callback. Each
//! step returns the raw response so a test can check it or tamper with what follows.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use server::app::Settings;
use server::auth::providers::OAuthSettings;
use server::rate::ServerClock;

use super::app::{ORIGIN, RunningApp};
use super::db::TestDb;
use super::fake_provider::{FakeProvider, Kind};
use super::http::{self, Response};

/// The app with both fakes, the test credentials and a manual rate clock (refilled before
/// each step that hits AUTH_IP, as in `support::auth`).
pub struct App {
    pub app: RunningApp,
    pub addr: SocketAddr,
    pub clock: ServerClock,
    pub google: FakeProvider,
    pub discord: FakeProvider,
}

impl App {
    pub fn refill(&self) {
        self.clock.advance(Duration::from_secs(60));
    }

    pub fn fake(&self, kind: Kind) -> &FakeProvider {
        match kind {
            Kind::Google => &self.google,
            Kind::Discord => &self.discord,
        }
    }
}

/// `Settings.oauth` pointing at the two fakes.
pub fn oauth_settings(google: &FakeProvider, discord: &FakeProvider) -> OAuthSettings {
    OAuthSettings {
        google: Some(google.urls()),
        discord: Some(discord.urls()),
    }
}

/// The app on `db` with both providers on, `extra` vars and `settings` (its `oauth` and `rate`
/// are replaced).
pub async fn start_app_with(
    db: &TestDb,
    extra: &[(&str, &str)],
    settings: Settings,
) -> (App, sqlx::PgPool) {
    let google = FakeProvider::start(Kind::Google).await;
    let discord = FakeProvider::start(Kind::Discord).await;
    let settings = Settings {
        oauth: oauth_settings(&google, &discord),
        ..settings
    };
    let (settings, clock) = super::auth::with_manual_clock(settings);
    let mut vars: Vec<(&str, &str)> = super::config::PROVIDER_VARS.to_vec();
    vars.extend_from_slice(extra);
    let (app, pool) = super::app::start_db(db, &vars, &settings).await;
    let addr = app.addr;
    (
        App {
            app,
            addr,
            clock,
            google,
            discord,
        },
        pool,
    )
}

pub async fn start_app(db: &TestDb) -> (App, sqlx::PgPool) {
    start_app_with(db, &[], Settings::default()).await
}

pub fn session_name() -> &'static str {
    super::config::config(&[]).cookies.session_name()
}

pub fn flow_name() -> &'static str {
    super::config::config(&[]).cookies.flow_name()
}

pub fn session_cookie(token: &str) -> String {
    format!("{}={token}", session_name())
}

/// The query of a URL, decoded.
pub fn query_of(url: &str) -> HashMap<String, String> {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

/// `POST /api/auth/{provider}/start` (or `/api/auth/link/{provider}` when `link`), from the web
/// origin, with an optional session cookie and a raw form body.
pub async fn begin(app: &App, path: &str, body: &str, session: Option<&str>) -> Response {
    app.refill();
    let cookie = session.map(session_cookie);
    let mut headers = vec![
        ("origin", ORIGIN),
        ("content-type", "application/x-www-form-urlencoded"),
    ];
    if let Some(cookie) = &cookie {
        headers.push(("cookie", cookie.as_str()));
    }
    http::send_body(app.addr, "POST", path, &headers, body).await
}

pub async fn start(app: &App, kind: Kind, session: Option<&str>) -> Response {
    begin(
        app,
        &format!("/api/auth/{}/start", kind.name()),
        "next=%2Fcuenta",
        session,
    )
    .await
}

pub async fn start_link(app: &App, kind: Kind, session: &str) -> Response {
    begin(
        app,
        &format!("/api/auth/link/{}", kind.name()),
        "next=%2Fcuenta",
        Some(session),
    )
    .await
}

/// The callback of a `start` (or `link`) response: a code of the fake for `identity`, the flow
/// cookie of `started` and, if given, a session cookie.
pub async fn finish(
    app: &App,
    kind: Kind,
    started: &Response,
    identity: &str,
    session: Option<&str>,
) -> Response {
    assert_eq!(started.status, 303, "start: {}", started.text());
    let location = started.header("location").expect("no location").to_owned();
    let query = query_of(&location);
    let code = app.fake(kind).code_for(&query["code_challenge"], identity);
    let flow = started
        .cookie(flow_name())
        .expect("no flow cookie")
        .to_owned();
    let path = format!(
        "/api/auth/{}/callback?code={code}&state={}",
        kind.name(),
        query["state"]
    );
    callback(app, &path, &flow, session).await
}

/// The callback with a flow cookie and an optional session cookie.
pub async fn callback(app: &App, path: &str, flow: &str, session: Option<&str>) -> Response {
    app.refill();
    let mut cookies = vec![format!("{}={flow}", flow_name())];
    cookies.extend(session.map(session_cookie));
    let line = cookies.join("; ");
    http::send(app.addr, "GET", path, &[("cookie", line.as_str())]).await
}

/// The whole login with `identity`.
pub async fn login(app: &App, kind: Kind, identity: &str) -> Response {
    let started = start(app, kind, None).await;
    finish(app, kind, &started, identity, None).await
}

/// The whole link of `identity` from the session `token`.
pub async fn link(app: &App, kind: Kind, identity: &str, token: &str) -> Response {
    let started = start_link(app, kind, token).await;
    finish(app, kind, &started, identity, Some(token)).await
}

/// `POST /api/auth/reauth/{provider}` from the session `token`, with `next` (009 RF11).
pub async fn start_reauth(app: &App, kind: Kind, token: &str) -> Response {
    begin(
        app,
        &format!("/api/auth/reauth/{}", kind.name()),
        "next=%2Fes%2Faccount%3Freauth%3D1",
        Some(token),
    )
    .await
}

/// The whole reauthentication of `identity` from the session `token`.
pub async fn reauth(app: &App, kind: Kind, identity: &str, token: &str) -> Response {
    let started = start_reauth(app, kind, token).await;
    finish(app, kind, &started, identity, Some(token)).await
}

/// The session token a successful callback set.
pub fn session_of(response: &Response) -> String {
    assert_eq!(response.status, 303, "{:?}", response.headers);
    let token = response
        .cookie(session_name())
        .unwrap_or_else(|| panic!("no session cookie: {:?}", response.headers));
    assert!(!token.is_empty(), "the session cookie was expired");
    token.to_owned()
}

/// The error code of a callback that failed: 303 to `/login?error=<code>` and no session.
pub fn error_of(response: &Response) -> String {
    assert_eq!(response.status, 303, "{}", response.text());
    let location = response.header("location").unwrap();
    let code = location
        .strip_prefix("/login?error=")
        .unwrap_or_else(|| panic!("not an error redirect: {location}"));
    assert!(
        response.cookie(session_name()).is_none_or(|v| v.is_empty()),
        "an error never sets a session"
    );
    code.to_owned()
}

/// A request with a session cookie, from the web origin.
pub async fn send(app: &App, method: &str, path: &str, token: &str, body: &str) -> Response {
    let cookie = session_cookie(token);
    http::send_body(
        app.addr,
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

/// `GET /api/me` with the session `token`.
pub async fn me(app: &App, token: &str) -> Response {
    let cookie = session_cookie(token);
    http::send(app.addr, "GET", "/api/me", &[("cookie", cookie.as_str())]).await
}

/// Moves the authentication of the session `token` `minutes` into the past (Postgres' clock).
pub async fn age(pool: &sqlx::PgPool, token: &str, minutes: i32) {
    sqlx::query(
        "UPDATE sessions SET authenticated_at = authenticated_at - make_interval(mins => $2) \
         WHERE token_hash = $1",
    )
    .bind(server::sessions::TokenHash::of(token).0.as_slice())
    .bind(minutes)
    .execute(pool)
    .await
    .unwrap();
}
