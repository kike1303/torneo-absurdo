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
        assert!(!route.path.contains('{'), "add a sample path for {route:?}");
        let response = http::send(
            app.addr,
            route.method.http().as_str(),
            route.path,
            &[("origin", ORIGIN)],
        )
        .await;
        assert!(
            response.status != 404 && response.status != 405,
            "{route:?}: {}",
            response.status
        );
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            if method == route.method.http().as_str() {
                continue;
            }
            let response = http::send(app.addr, method, route.path, &[("origin", ORIGIN)]).await;
            assert_eq!(response.status, 405, "{method} {}", route.path);
        }
        // Phase A has no mutation at all.
        assert_eq!(route.effect, Effect::Read, "{route:?}");
    }
}
