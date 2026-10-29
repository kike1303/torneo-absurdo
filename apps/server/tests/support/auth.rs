//! The login flow with the dev provider, step by step (008 RF-B5, RF-B6). Each step returns
//! the raw response so a test can check it or tamper with what follows.

use std::net::SocketAddr;
use std::time::Duration;

use server::app::Settings;
use server::rate::{RateSettings, ServerClock};

use super::app::{ORIGIN, RunningApp};
use super::db::TestDb;
use super::http::{self, Response};

/// The app with a manual rate clock. The steps that hit the AUTH_IP bucket (start and callback,
/// burst 5) advance it a minute first, so a test can log in many times from 127.0.0.1 without
/// changing the quota; the 429 itself has its own test.
pub struct App {
    pub app: RunningApp,
    pub addr: SocketAddr,
    pub clock: ServerClock,
}

impl App {
    fn refill(&self) {
        self.clock.advance(Duration::from_secs(60));
    }
}

/// `settings` with a manual rate clock.
pub fn with_manual_clock(settings: Settings) -> (Settings, ServerClock) {
    let clock = ServerClock::manual();
    let settings = Settings {
        rate: RateSettings {
            clock: clock.clone(),
            ..RateSettings::default()
        },
        ..settings
    };
    (settings, clock)
}

pub async fn start_app(db: &TestDb, settings: Settings) -> (App, sqlx::PgPool) {
    let (settings, clock) = with_manual_clock(settings);
    let (app, pool) = super::app::start_db(db, &[], &settings).await;
    let addr = app.addr;
    (App { app, addr, clock }, pool)
}

pub const SESSION: &str = "ta_dev_session";
pub const FLOW: &str = "ta_dev_oauth";

pub fn form(pairs: &[(&str, &str)]) -> String {
    let mut out = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        out.append_pair(k, v);
    }
    out.finish()
}

/// `POST /api/auth/dev/start` from the web origin, with an optional session cookie.
pub async fn start(app: &App, fields: &[(&str, &str)], cookie: Option<&str>) -> Response {
    app.refill();
    let addr = app.addr;
    let mut headers = vec![
        ("origin", ORIGIN),
        ("content-type", "application/x-www-form-urlencoded"),
    ];
    let cookie_line;
    if let Some(cookie) = cookie {
        cookie_line = cookie.to_owned();
        headers.push(("cookie", &cookie_line));
    }
    http::send_body(addr, "POST", "/api/auth/dev/start", &headers, &form(fields)).await
}

/// The authorize step (the browser follows the 303 of `start`).
pub async fn authorize(app: &App, start: &Response) -> Response {
    assert_eq!(start.status, 303, "start: {}", start.text());
    http::send(app.addr, "GET", &start.location_path(), &[]).await
}

/// The callback step with these cookies (the browser follows the 303 of `authorize`).
pub async fn callback(app: &App, path: &str, cookies: &[String]) -> Response {
    app.refill();
    let addr = app.addr;
    let line = cookies.join("; ");
    let headers: Vec<(&str, &str)> = if line.is_empty() {
        Vec::new()
    } else {
        vec![("cookie", line.as_str())]
    };
    http::send(addr, "GET", path, &headers).await
}

/// The whole flow for `identity`; returns the callback response.
pub async fn login(
    app: &App,
    identity: &str,
    next: Option<&str>,
    session: Option<&str>,
) -> Response {
    let mut fields = vec![("dev_identity", identity)];
    if let Some(next) = next {
        fields.push(("next", next));
    }
    let session_cookie = session.map(|token| format!("{SESSION}={token}"));
    let started = start(app, &fields, session_cookie.as_deref()).await;
    let flow = started.cookie(FLOW).expect("no flow cookie").to_owned();
    let authorized = authorize(app, &started).await;
    assert_eq!(authorized.status, 303, "authorize: {}", authorized.text());
    let mut cookies = vec![format!("{FLOW}={flow}")];
    cookies.extend(session_cookie);
    callback(app, &authorized.location_path(), &cookies).await
}

/// The session token a successful login set.
pub fn session_of(response: &Response) -> String {
    let token = response.cookie(SESSION).expect("no session cookie");
    assert!(!token.is_empty(), "the session cookie was expired");
    token.to_owned()
}

/// `GET /api/me` with a session token.
pub async fn me(addr: SocketAddr, token: &str) -> Response {
    let cookie = format!("{SESSION}={token}");
    http::send(addr, "GET", "/api/me", &[("cookie", &cookie)]).await
}
