//! 008 T1: every entry of the manifest answers on the real router by its method, and every
//! unsafe method on it (with a valid `Origin`) is 405.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::app::Settings;
use server::http::routes::{Effect, ROUTES, check_manifest};
use support::app::ORIGIN;
use support::http;

#[tokio::test]
async fn every_manifest_entry_is_routed_and_only_by_its_method() {
    assert_eq!(check_manifest(ROUTES), Ok(()));
    let app = support::app::start(support::app::state(&[], &Settings::default())).await;
    for route in ROUTES {
        // A sample value for the path parameters.
        let path = route.path.replace("{provider}", "dev").replace("{id}", "1");
        assert!(!path.contains('{'), "add a sample value for {route:?}");
        let method = route.method.http();
        let response = http::send(app.addr, method.as_str(), &path, &[("origin", ORIGIN)]).await;
        // Without `dev-auth` and without the OAuth credentials no provider is enabled, so the
        // provider routes answer the handler's 404; the router still matches them, as the 405
        // below shows.
        let provider_route = route.path.contains("{provider}");
        if cfg!(feature = "dev-auth") || !provider_route {
            assert!(
                response.status != 404 && response.status != 405,
                "{route:?}: {}",
                response.status
            );
        }
        for other in ["POST", "PUT", "PATCH", "DELETE"] {
            if other == method.as_str() {
                continue;
            }
            let response = http::send(app.addr, other, &path, &[("origin", ORIGIN)]).await;
            assert_eq!(response.status, 405, "{other} {path}");
        }
        // A safe method that mutates is one of the declared exceptions (check_manifest).
        if route.effect == Effect::Mutate && matches!(method.as_str(), "GET" | "HEAD") {
            assert!(
                server::http::routes::SAFE_METHOD_EXCEPTIONS
                    .iter()
                    .any(|(m, p)| *m == route.method && *p == route.path),
                "{route:?}"
            );
        }
    }
}

/// 008 phase C (T1 extended): the four routes of C are in the manifest as mutations, the `PUT`s
/// are covered by the `Origin` check like every other method, and `/api/auth/link/start` falls
/// in `link` (the static segment first) with the provider `start`: a 404, never `start`.
#[tokio::test]
async fn the_routes_of_phase_c() {
    for (method, path) in [
        ("POST", "/api/auth/link/{provider}"),
        ("POST", "/api/auth/unlink/{provider}"),
        ("PUT", "/api/me/display-name"),
        ("PUT", "/api/me/preferences"),
    ] {
        let route = ROUTES
            .iter()
            .find(|r| r.path == path)
            .unwrap_or_else(|| panic!("{path} is not in the manifest"));
        assert_eq!(route.method.http().as_str(), method, "{path}");
        assert_eq!(route.effect, Effect::Mutate, "{path}");
    }
    let mut vars: Vec<(&str, &str)> = support::config::PROVIDER_VARS.to_vec();
    vars.push(("DATABASE_URL", support::config::UNUSED_DB));
    let app = support::app::start(support::app::state(&vars, &Settings::default())).await;
    for path in ["/api/me/display-name", "/api/me/preferences"] {
        let forbidden = http::send(app.addr, "PUT", path, &[]).await;
        assert_eq!(forbidden.status, 403, "{path}");
        assert_eq!(forbidden.text(), r#"{"error":"auth.csrf_origin"}"#);
    }
    let link_start = http::send(
        app.addr,
        "POST",
        "/api/auth/link/start",
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(link_start.status, 404);
    assert_eq!(link_start.text(), r#"{"error":"http.not_found"}"#);
    assert!(link_start.header("location").is_none());
    // With Google on, `link/google` is the link route (no session: 303 to the login page, 008
    // "Enmienda 2026-10-30" of RF-C2), never a 404.
    let link = http::send(
        app.addr,
        "POST",
        "/api/auth/link/google",
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(link.status, 303);
    assert_eq!(link.header("location"), Some("/login"));
}

/// 009 decision 9 (T1 extended): `reauth` is a mutation of the manifest, a `POST` without
/// `Origin` is refused, and `/api/auth/reauth/start` falls in `reauth` with the provider `start`:
/// a 404, never `start`.
#[tokio::test]
async fn the_reauth_route_of_009() {
    let route = ROUTES
        .iter()
        .find(|r| r.path == "/api/auth/reauth/{provider}")
        .expect("reauth is not in the manifest");
    assert_eq!(route.method.http().as_str(), "POST");
    assert_eq!(route.effect, Effect::Mutate);
    let mut vars: Vec<(&str, &str)> = support::config::PROVIDER_VARS.to_vec();
    vars.push(("DATABASE_URL", support::config::UNUSED_DB));
    let app = support::app::start(support::app::state(&vars, &Settings::default())).await;
    let forbidden = http::send(app.addr, "POST", "/api/auth/reauth/google", &[]).await;
    assert_eq!(forbidden.status, 403);
    assert_eq!(forbidden.text(), r#"{"error":"auth.csrf_origin"}"#);
    let start = http::send(
        app.addr,
        "POST",
        "/api/auth/reauth/start",
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(start.status, 404);
    assert_eq!(start.text(), r#"{"error":"http.not_found"}"#);
    // No session: 303 to the login page, as `link`.
    let reauth = http::send(
        app.addr,
        "POST",
        "/api/auth/reauth/google",
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(reauth.status, 303);
    assert_eq!(reauth.header("location"), Some("/login"));
}

/// 009 decision 9: `reauth-status` is a read, a static segment beside `providers`, and without a
/// session it is the 401 of `/api/me`.
#[tokio::test]
async fn the_reauth_status_route_of_009() {
    let route = ROUTES
        .iter()
        .find(|r| r.path == "/api/auth/reauth-status")
        .expect("reauth-status is not in the manifest");
    assert_eq!(route.method.http().as_str(), "GET");
    assert_eq!(route.effect, Effect::Read);
    let app = support::app::start(support::app::state(&[], &Settings::default())).await;
    let refused = http::send(app.addr, "GET", "/api/auth/reauth-status", &[]).await;
    assert_eq!(refused.status, 401);
    assert_eq!(refused.text(), r#"{"error":"auth.unauthenticated"}"#);
}
