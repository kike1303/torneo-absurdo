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
        let path = route.path.replace("{provider}", "dev");
        assert!(!path.contains('{'), "add a sample value for {route:?}");
        let method = route.method.http();
        let response = http::send(app.addr, method.as_str(), &path, &[("origin", ORIGIN)]).await;
        // Without `dev-auth` no provider is enabled yet (Google and Discord are phase C), so
        // the provider routes answer the handler's 404; the router still matches them, as the
        // 405 below shows.
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
