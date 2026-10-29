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

/// The methods a route may use. An enum (not `Method`) so the filter is an exhaustive match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMethod {
    Get,
    Head,
    Options,
    Post,
    Put,
    Patch,
    Delete,
}

impl RouteMethod {
    pub fn http(self) -> Method {
        match self {
            Self::Get => Method::GET,
            Self::Head => Method::HEAD,
            Self::Options => Method::OPTIONS,
            Self::Post => Method::POST,
            Self::Put => Method::PUT,
            Self::Patch => Method::PATCH,
            Self::Delete => Method::DELETE,
        }
    }

    fn filter(self) -> MethodFilter {
        match self {
            Self::Get => MethodFilter::GET,
            Self::Head => MethodFilter::HEAD,
            Self::Options => MethodFilter::OPTIONS,
            Self::Post => MethodFilter::POST,
            Self::Put => MethodFilter::PUT,
            Self::Patch => MethodFilter::PATCH,
            Self::Delete => MethodFilter::DELETE,
        }
    }

    fn is_safe(self) -> bool {
        matches!(self, Self::Get | Self::Head | Self::Options)
    }
}

/// The handler of a route. One variant per route: [`route_for`] matches it exhaustively, so
/// an entry without a handler does not compile (review of #53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    Health,
    Ws,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSpec {
    pub method: RouteMethod,
    pub path: &'static str,
    pub effect: Effect,
    pub handler: Handler,
}

/// Every route of the server.
pub const ROUTES: &[RouteSpec] = &[
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/health",
        effect: Effect::Read,
        handler: Handler::Health,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/ws",
        effect: Effect::Read,
        handler: Handler::Ws,
    },
];

/// Safe-method routes allowed to mutate (ADR-0008 D4): the OAuth callback, a top-level
/// redirect the provider makes with `GET`.
pub const SAFE_METHOD_EXCEPTIONS: &[(RouteMethod, &str)] =
    &[(RouteMethod::Get, "/api/auth/{provider}/callback")];

/// `Err` with the first entry that mutates with `GET`, `HEAD` or `OPTIONS` without being an
/// exception.
pub fn check_manifest(routes: &[RouteSpec]) -> Result<(), RouteSpec> {
    for route in routes {
        let excepted = SAFE_METHOD_EXCEPTIONS
            .iter()
            .any(|(method, path)| *method == route.method && *path == route.path);
        if route.effect == Effect::Mutate && route.method.is_safe() && !excepted {
            return Err(route.clone());
        }
    }
    Ok(())
}

/// The method router of a manifest entry, with its per-route layers.
fn route_for(spec: &RouteSpec, _state: &AppState) -> MethodRouter<AppState> {
    let filter = spec.method.filter();
    match spec.handler {
        Handler::Health => on(filter, health::handler).fallback(errors::method_not_allowed),
        Handler::Ws => on(filter, crate::ws::handler),
    }
}

/// Registers every entry of [`ROUTES`] on `router`.
pub fn register(router: Router<AppState>, state: &AppState) -> Router<AppState> {
    ROUTES.iter().fold(router, |router, spec| {
        router.route(spec.path, route_for(spec, state))
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
            method: RouteMethod::Get,
            path: "/api/account/delete",
            effect: Effect::Mutate,
            handler: Handler::Health,
        };
        let routes = [ROUTES[0].clone(), bad.clone()];
        assert_eq!(check_manifest(&routes), Err(bad));
        for method in [RouteMethod::Head, RouteMethod::Options] {
            let bad = RouteSpec {
                method,
                path: "/x",
                effect: Effect::Mutate,
                handler: Handler::Health,
            };
            assert!(check_manifest(std::slice::from_ref(&bad)).is_err());
        }
    }

    #[test]
    fn the_declared_exception_and_unsafe_mutations_pass() {
        let routes = [
            RouteSpec {
                method: RouteMethod::Get,
                path: "/api/auth/{provider}/callback",
                effect: Effect::Mutate,
                handler: Handler::Health,
            },
            RouteSpec {
                method: RouteMethod::Post,
                path: "/api/auth/logout",
                effect: Effect::Mutate,
                handler: Handler::Health,
            },
        ];
        assert_eq!(check_manifest(&routes), Ok(()));
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
