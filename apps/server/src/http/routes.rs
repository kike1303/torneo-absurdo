//! The route manifest (008 T1, RF-A5): every route is registered from [`ROUTES`] and nowhere
//! else, and no safe method may mutate except the declared exceptions.

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{MethodFilter, MethodRouter, on};
use axum::{Router, middleware};
use tower_http::set_header::SetResponseHeaderLayer;

use super::{errors, health};
use crate::app::AppState;
use crate::{fights, rate};

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
    AuthStart,
    AuthLink,
    AuthReauth,
    AuthUnlink,
    AuthCallback,
    AuthProviders,
    AuthReauthStatus,
    Logout,
    LogoutAll,
    Me,
    MeDisplayName,
    MePreferences,
    FightsList,
    FightsCurrent,
    FightsVoidStats,
    FightDetail,
    BonusClaim,
    BailoutClaim,
    Wallet,
    Bet,
    MyBets,
    BetHistory,
    Leaderboard,
    Seasons,
    FightPool,
    BetQuote,
    Botarga,
    BotargaCreate,
    BotargaBuild,
    BotargaDrawChoose,
    PvpFightCreate,
    PvpFights,
    PvpFightDetail,
    PvpLeaderboard,
    PvpSeasons,
    #[cfg(feature = "dev-auth")]
    DevAuthorize,
    #[cfg(feature = "dev-auth")]
    DevToken,
    #[cfg(feature = "dev-auth")]
    DevUserinfo,
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
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/{provider}/start",
        effect: Effect::Mutate,
        handler: Handler::AuthStart,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/link/{provider}",
        effect: Effect::Mutate,
        handler: Handler::AuthLink,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/reauth/{provider}",
        effect: Effect::Mutate,
        handler: Handler::AuthReauth,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/unlink/{provider}",
        effect: Effect::Mutate,
        handler: Handler::AuthUnlink,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/auth/{provider}/callback",
        effect: Effect::Mutate,
        handler: Handler::AuthCallback,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/auth/providers",
        effect: Effect::Read,
        handler: Handler::AuthProviders,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/auth/reauth-status",
        effect: Effect::Read,
        handler: Handler::AuthReauthStatus,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/logout",
        effect: Effect::Mutate,
        handler: Handler::Logout,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/logout-all",
        effect: Effect::Mutate,
        handler: Handler::LogoutAll,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/me",
        effect: Effect::Read,
        handler: Handler::Me,
    },
    RouteSpec {
        method: RouteMethod::Put,
        path: "/api/me/display-name",
        effect: Effect::Mutate,
        handler: Handler::MeDisplayName,
    },
    RouteSpec {
        method: RouteMethod::Put,
        path: "/api/me/preferences",
        effect: Effect::Mutate,
        handler: Handler::MePreferences,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/fights",
        effect: Effect::Read,
        handler: Handler::FightsList,
    },
    // The static segment wins over the parameter (010 RF34, RF35).
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/fights/current",
        effect: Effect::Read,
        handler: Handler::FightsCurrent,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/fights/void-stats",
        effect: Effect::Read,
        handler: Handler::FightsVoidStats,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/fights/{id}",
        effect: Effect::Read,
        handler: Handler::FightDetail,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/bonus/claim",
        effect: Effect::Mutate,
        handler: Handler::BonusClaim,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/bailout/claim",
        effect: Effect::Mutate,
        handler: Handler::BailoutClaim,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/wallet",
        effect: Effect::Read,
        handler: Handler::Wallet,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/bets",
        effect: Effect::Mutate,
        handler: Handler::Bet,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/bets/mine",
        effect: Effect::Read,
        handler: Handler::MyBets,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/bets/history",
        effect: Effect::Read,
        handler: Handler::BetHistory,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/leaderboard",
        effect: Effect::Read,
        handler: Handler::Leaderboard,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/seasons",
        effect: Effect::Read,
        handler: Handler::Seasons,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/fights/{id}/pool",
        effect: Effect::Read,
        handler: Handler::FightPool,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/fights/{id}/quote",
        effect: Effect::Read,
        handler: Handler::BetQuote,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/botarga",
        effect: Effect::Read,
        handler: Handler::Botarga,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/botarga",
        effect: Effect::Mutate,
        handler: Handler::BotargaCreate,
    },
    RouteSpec {
        method: RouteMethod::Put,
        path: "/api/botarga/build",
        effect: Effect::Mutate,
        handler: Handler::BotargaBuild,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/botarga/draw/choose",
        effect: Effect::Mutate,
        handler: Handler::BotargaDrawChoose,
    },
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/pvp/fights",
        effect: Effect::Mutate,
        handler: Handler::PvpFightCreate,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/pvp/fights",
        effect: Effect::Read,
        handler: Handler::PvpFights,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/pvp/fights/{id}",
        effect: Effect::Read,
        handler: Handler::PvpFightDetail,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/pvp/leaderboard",
        effect: Effect::Read,
        handler: Handler::PvpLeaderboard,
    },
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/pvp/seasons",
        effect: Effect::Read,
        handler: Handler::PvpSeasons,
    },
    #[cfg(feature = "dev-auth")]
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/auth/dev/oauth/authorize",
        effect: Effect::Mutate,
        handler: Handler::DevAuthorize,
    },
    #[cfg(feature = "dev-auth")]
    RouteSpec {
        method: RouteMethod::Post,
        path: "/api/auth/dev/oauth/token",
        effect: Effect::Mutate,
        handler: Handler::DevToken,
    },
    #[cfg(feature = "dev-auth")]
    RouteSpec {
        method: RouteMethod::Get,
        path: "/api/auth/dev/oauth/userinfo",
        effect: Effect::Read,
        handler: Handler::DevUserinfo,
    },
];

/// Largest body of the form routes (008 plan-b decision 10).
pub const MAX_FORM_BYTES: usize = 4096;

/// Safe-method routes allowed to mutate (ADR-0008 D4): the OAuth callback, a top-level
/// redirect the provider makes with `GET`.
pub const SAFE_METHOD_EXCEPTIONS: &[(RouteMethod, &str)] = &[
    (RouteMethod::Get, "/api/auth/{provider}/callback"),
    // The authorize endpoint of the dev provider stores a code in memory, as real providers do
    // on their side of a GET (008 plan-b question 7). Only with `dev-auth`.
    #[cfg(feature = "dev-auth")]
    (RouteMethod::Get, "/api/auth/dev/oauth/authorize"),
];

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

/// A read of the database by anyone: `FIGHT_READ_IP`.
fn fight_read(route: MethodRouter<AppState>, state: &AppState) -> MethodRouter<AppState> {
    route
        .layer::<_, std::convert::Infallible>(middleware::from_fn_with_state(
            state.rate.fight_read_ip.clone(),
            rate::http::layer,
        ))
        // Outside the limiter, so its 429 says `no-store` too (RF34).
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}

/// The method router of a manifest entry, with its per-route layers.
fn route_for(spec: &RouteSpec, state: &AppState) -> MethodRouter<AppState> {
    use crate::auth;
    let filter = spec.method.filter();
    let body = DefaultBodyLimit::max(MAX_FORM_BYTES);
    // `start`, `link` (phase C), `reauth` (009) and `callback` share the AUTH_IP bucket (D7).
    // They are browser navigations, so over the quota they redirect instead of answering JSON
    // (008 "Enmienda 2026-10-30" of RF-B5).
    let auth_ip = |on_limit| {
        let navigation = rate::http::Navigation {
            limiter: state.rate.auth_ip.clone(),
            on_limit,
        };
        middleware::from_fn_with_state(navigation, rate::http::redirect_layer)
    };
    let keep = rate::http::OnLimit::KeepCookies;
    match spec.handler {
        Handler::Health => on(filter, health::handler).fallback(errors::method_not_allowed),
        Handler::Ws => on(filter, crate::ws::handler),
        Handler::AuthStart => on(filter, auth::start::handler)
            .layer(body)
            .layer(auth_ip(keep)),
        Handler::AuthLink => on(filter, auth::link::handler)
            .layer(body)
            .layer(auth_ip(keep)),
        Handler::AuthReauth => on(filter, auth::reauth::handler)
            .layer(body)
            .layer(auth_ip(keep)),
        // No body extractor (like logout): nothing for the limit to reject (RF-C3).
        Handler::AuthUnlink => on(filter, auth::unlink::handler),
        Handler::AuthCallback => on(filter, auth::callback::handler)
            .layer(auth_ip(rate::http::OnLimit::ExpireFlow(state.auth.cookies))),
        Handler::AuthProviders => on(filter, auth::providers),
        Handler::AuthReauthStatus => on(filter, auth::reauth_status::handler),
        Handler::Logout => on(filter, auth::logout::logout).layer(body),
        Handler::LogoutAll => on(filter, auth::logout::logout_all).layer(body),
        Handler::Me => on(filter, crate::profile::me::handler),
        Handler::MeDisplayName => on(filter, crate::profile::update::display_name).layer(body),
        Handler::MePreferences => on(filter, crate::profile::update::preferences).layer(body),
        Handler::FightsCurrent => on(filter, fights::api::current),
        // One bucket for both: a visitor reads the list and the detail (011 RF7).
        Handler::FightsList => fight_read(on(filter, fights::api::list), state),
        Handler::FightDetail => fight_read(on(filter, fights::api::detail), state),
        // The same limit and the same cache as the detail (012 RF23).
        Handler::FightsVoidStats => fight_read(on(filter, fights::void_stats::handler), state),
        // 012 RF29, RF30: no body extractor, and the limit anyway, as the spec asks.
        Handler::BonusClaim => on(filter, crate::bets::claims::bonus).layer(body),
        Handler::BailoutClaim => on(filter, crate::bets::claims::bailout).layer(body),
        Handler::Wallet => on(filter, crate::bets::wallet::handler),
        // 012 RF25: over `MAX_FORM_BYTES`, `JsonRejection` gives 422 `bet.invalid_request`.
        Handler::Bet => on(filter, crate::bets::api::place).layer(body),
        Handler::MyBets => on(filter, crate::bets::api::mine),
        // 019: the quota and the cache of `bets/mine`.
        Handler::BetHistory => on(filter, crate::bets::history::handler),
        // 020: public reads, with the limit and the cache of the list of fights.
        Handler::Leaderboard => fight_read(on(filter, crate::seasons::api::leaderboard), state),
        Handler::Seasons => fight_read(on(filter, crate::seasons::api::list), state),
        // 012 RF34: the limit and the cache of the detail.
        Handler::FightPool => fight_read(on(filter, crate::bets::pool::pool), state),
        Handler::BetQuote => on(filter, crate::bets::pool::quote),
        Handler::Botarga => on(filter, crate::botarga::api::get),
        // 021: no body extractor in the first, and the limit anyway; over `MAX_FORM_BYTES` the
        // second gives 422 `botarga.invalid_request`.
        Handler::BotargaCreate => on(filter, crate::botarga::api::create).layer(body),
        Handler::BotargaBuild => on(filter, crate::botarga::api::put_build).layer(body),
        // 021 phase 3: over `MAX_FORM_BYTES`, 422 `botarga.invalid_request`.
        Handler::BotargaDrawChoose => on(filter, crate::botarga::api::choose_draw).layer(body),
        // 021: over `MAX_FORM_BYTES`, 422 `pvp.invalid_request`. The list is private; the detail
        // is public, with the limit and the cache of the tournament fights.
        Handler::PvpFightCreate => on(filter, crate::pvp::api::create).layer(body),
        Handler::PvpFights => on(filter, crate::pvp::api::list),
        Handler::PvpFightDetail => fight_read(on(filter, crate::pvp::api::detail), state),
        // 021 phase 4: public reads, as the leaderboard of 020.
        Handler::PvpLeaderboard => fight_read(on(filter, crate::pvp::api::leaderboard), state),
        Handler::PvpSeasons => fight_read(on(filter, crate::pvp::api::seasons), state),
        #[cfg(feature = "dev-auth")]
        Handler::DevAuthorize => on(filter, auth::dev::authorize),
        #[cfg(feature = "dev-auth")]
        Handler::DevToken => on(filter, auth::dev::token).layer(body),
        #[cfg(feature = "dev-auth")]
        Handler::DevUserinfo => on(filter, auth::dev::userinfo),
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
    fn the_safe_method_exceptions_per_variant() {
        #[cfg(not(feature = "dev-auth"))]
        assert_eq!(
            SAFE_METHOD_EXCEPTIONS,
            &[(RouteMethod::Get, "/api/auth/{provider}/callback")]
        );
        #[cfg(feature = "dev-auth")]
        assert_eq!(
            SAFE_METHOD_EXCEPTIONS,
            &[
                (RouteMethod::Get, "/api/auth/{provider}/callback"),
                (RouteMethod::Get, "/api/auth/dev/oauth/authorize"),
            ]
        );
    }

    /// Without the feature no `dev` route exists (RF-D9 is checked on the release binary in
    /// phase D).
    #[cfg(not(feature = "dev-auth"))]
    #[test]
    fn no_dev_route_without_the_feature() {
        assert!(ROUTES.iter().all(|r| !r.path.starts_with("/api/auth/dev/")));
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
