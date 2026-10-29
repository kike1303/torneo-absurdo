//! The route manifest (008 T1, RF-A5): every route is registered from [`ROUTES`] and nowhere
//! else, and no safe method may mutate except the declared exceptions.

use axum::Router;
use axum::http::Method;
use axum::routing::{MethodFilter, MethodRouter, on};

use super::{errors, health};
use crate::app::AppState;

/// What a route does to the server state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Read,
    Mutate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSpec {
    pub method: Method,
    pub path: &'static str,
    pub effect: Effect,
}

/// Every route of the server.
pub const ROUTES: &[RouteSpec] = &[
    RouteSpec {
        method: Method::GET,
        path: "/api/health",
        effect: Effect::Read,
    },
    RouteSpec {
        method: Method::GET,
        path: "/ws",
        effect: Effect::Read,
    },
];

/// Safe-method routes allowed to mutate (ADR-0008 D4): the OAuth callback, a top-level
/// redirect the provider makes with `GET` (phase B).
pub const SAFE_METHOD_EXCEPTIONS: &[(Method, &str)] =
    &[(Method::GET, "/api/auth/{provider}/callback")];

/// `Err` with the first entry that mutates with `GET`, `HEAD` or `OPTIONS` without being an
/// exception.
pub fn check_manifest(routes: &[RouteSpec]) -> Result<(), RouteSpec> {
    for route in routes {
        let safe = matches!(route.method, Method::GET | Method::HEAD | Method::OPTIONS);
        let excepted = SAFE_METHOD_EXCEPTIONS
            .iter()
            .any(|(method, path)| *method == route.method && *path == route.path);
        if route.effect == Effect::Mutate && safe && !excepted {
            return Err(route.clone());
        }
    }
    Ok(())
}

/// The handler of a manifest entry, `None` for an entry without one (a UT forbids it).
fn route_for(spec: &RouteSpec) -> Option<MethodRouter<AppState>> {
    let filter = MethodFilter::try_from(spec.method.clone()).ok()?;
    match spec.path {
        "/api/health" => Some(on(filter, health::handler).fallback(errors::method_not_allowed)),
        "/ws" => Some(on(filter, crate::ws::handler)),
        _ => None,
    }
}

/// Registers every entry of [`ROUTES`] on `router`.
pub fn register(router: Router<AppState>) -> Router<AppState> {
    ROUTES
        .iter()
        .fold(router, |router, spec| match route_for(spec) {
            Some(handler) => router.route(spec.path, handler),
            None => router,
        })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn the_manifest_has_no_safe_method_that_mutates() {
        assert_eq!(check_manifest(ROUTES), Ok(()));
    }

    #[test]
    fn a_get_that_mutates_fails_the_check() {
        let bad = RouteSpec {
            method: Method::GET,
            path: "/api/account/delete",
            effect: Effect::Mutate,
        };
        let routes = [ROUTES[0].clone(), bad.clone()];
        assert_eq!(check_manifest(&routes), Err(bad));
        for method in [Method::HEAD, Method::OPTIONS] {
            let bad = RouteSpec {
                method,
                path: "/x",
                effect: Effect::Mutate,
            };
            assert!(check_manifest(std::slice::from_ref(&bad)).is_err());
        }
    }

    #[test]
    fn the_declared_exception_and_unsafe_mutations_pass() {
        let routes = [
            RouteSpec {
                method: Method::GET,
                path: "/api/auth/{provider}/callback",
                effect: Effect::Mutate,
            },
            RouteSpec {
                method: Method::POST,
                path: "/api/auth/logout",
                effect: Effect::Mutate,
            },
        ];
        assert_eq!(check_manifest(&routes), Ok(()));
    }

    #[test]
    fn every_entry_has_a_handler() {
        for spec in ROUTES {
            assert!(route_for(spec).is_some(), "{spec:?}");
        }
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn routes_are_registered_only_from_the_manifest() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let this = src.join("http").join("routes.rs");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(files.len() > 10, "{files:?}");
        let needle = [".route", "("].concat();
        for file in files.into_iter().filter(|file| *file != this) {
            let text = std::fs::read_to_string(&file).unwrap();
            assert!(
                !text.contains(&needle),
                "{} registers a route",
                file.display()
            );
        }
    }
}
