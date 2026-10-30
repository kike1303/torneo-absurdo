//! The HTTP layers of a limiter (008 RF-A4).
//!
//! - [`layer`]: 429 `http.rate_limited` with `Retry-After`, for routes a page calls with
//!   `fetch` or a form action. Mounted on none today.
//! - [`redirect_layer`]: for the routes the browser navigates to (`start`, `link` and
//!   `callback`, which share AUTH_IP): 303 to the login page with `auth.try_again`, without
//!   `Retry-After` or a body (008 "Enmienda 2026-10-30" of RF-B5). It never touches the database
//!   and never logs: the `code` and `state` of a callback are in its query.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use protocol::{ApiErrorCode, CallbackErrorCode};

use super::KeyedLimiter;
use crate::auth::callback::{error_redirect, try_again_redirect};
use crate::http::cookies::CookiePolicy;
use crate::http::errors::too_many_requests;
use crate::net::{ClientContext, IpKey};

/// The bucket key of a request: its `IpKey`. `net::layer` always runs first; without it, one
/// shared bucket (more restrictive).
fn key_of(request: &Request) -> IpKey {
    request
        .extensions()
        .get::<ClientContext>()
        .map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.key)
}

pub async fn layer(
    State(limiter): State<Arc<KeyedLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    match limiter.check(&key_of(&request)).retry_after_secs() {
        None => next.run(request).await,
        Some(secs) => too_many_requests(ApiErrorCode::RateLimited, secs),
    }
}

/// What a navigation route does with its cookies when the quota refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnLimit {
    /// `start` and `link`: no flow cookie is set, and nothing else changes.
    KeepCookies,
    /// `callback`: the flow cookie is expired, by the same function as every callback error.
    ExpireFlow(CookiePolicy),
}

/// The state of [`redirect_layer`].
#[derive(Clone)]
pub struct Navigation {
    pub limiter: Arc<KeyedLimiter>,
    pub on_limit: OnLimit,
}

pub async fn redirect_layer(
    State(navigation): State<Navigation>,
    request: Request,
    next: Next,
) -> Response {
    if navigation
        .limiter
        .check(&key_of(&request))
        .retry_after_secs()
        .is_none()
    {
        return next.run(request).await;
    }
    match navigation.on_limit {
        OnLimit::KeepCookies => try_again_redirect(HeaderValue::from_static("no-store")),
        OnLimit::ExpireFlow(cookies) => error_redirect(&cookies, CallbackErrorCode::TryAgain),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::{Body, to_bytes};
    use axum::http::StatusCode;
    use axum::{Extension, Router, middleware};
    use tower::ServiceExt;

    use super::*;
    use crate::rate::{AUTH_IP, RateSettings, ServerClock};

    fn context(ip: &str) -> ClientContext {
        let ip: IpAddr = ip.parse().unwrap();
        ClientContext {
            ip,
            key: IpKey::from(ip),
            internal: false,
        }
    }

    fn app(limiter: Arc<KeyedLimiter>, client: ClientContext) -> Router {
        Router::new()
            .fallback(|| async { "ok" })
            .layer(middleware::from_fn_with_state(limiter, layer))
            .layer(Extension(client))
    }

    async fn call(app: &Router) -> (StatusCode, Option<String>, String) {
        let response = app
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        let status = response.status();
        let retry = response
            .headers()
            .get("retry-after")
            .map(|v| v.to_str().unwrap().to_owned());
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        (status, retry, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn the_sixth_request_in_a_burst_is_429_with_retry_after() {
        let clock = ServerClock::manual();
        let settings = RateSettings {
            max_keys: 2,
            clock: clock.clone(),
        };
        let limiter = Arc::new(KeyedLimiter::new(AUTH_IP, &settings));
        let first = app(limiter.clone(), context("203.0.113.7"));
        for _ in 0..5 {
            assert_eq!(call(&first).await.0, StatusCode::OK);
        }
        let (status, retry, body) = call(&first).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(retry.as_deref(), Some("6"));
        assert_eq!(body, r#"{"error":"http.rate_limited"}"#);
        clock.advance(Duration::from_secs(6));
        assert_eq!(call(&first).await.0, StatusCode::OK);
        assert_eq!(call(&first).await.0, StatusCode::TOO_MANY_REQUESTS);

        // Two IPv6 of the same /64 share the bucket (T22, IP part).
        let a = app(limiter.clone(), context("2001:db8:1:2::a"));
        let b = app(limiter.clone(), context("2001:db8:1:2:ffff::1"));
        for _ in 0..5 {
            assert_eq!(call(&a).await.0, StatusCode::OK);
        }
        assert_eq!(call(&b).await.0, StatusCode::TOO_MANY_REQUESTS);

        // The table (cap 2) is full: a new key gets 429 with the sweep interval.
        let third = app(limiter, context("198.51.100.1"));
        let (status, retry, body) = call(&third).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(retry.as_deref(), Some("60"));
        assert_eq!(body, r#"{"error":"http.rate_limited"}"#);
    }

    fn navigation_app(
        limiter: Arc<KeyedLimiter>,
        on_limit: OnLimit,
        client: ClientContext,
    ) -> Router {
        let navigation = Navigation { limiter, on_limit };
        Router::new()
            .fallback(|| async { "ok" })
            .layer(middleware::from_fn_with_state(navigation, redirect_layer))
            .layer(Extension(client))
    }

    async fn response(app: &Router) -> axum::response::Response {
        app.clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap()
    }

    /// 008 "Enmienda 2026-10-30" of RF-B5: over the burst, and with the key table full, a 303
    /// to `auth.try_again` with no `Retry-After` and no body. `start` and `link` set no cookie;
    /// the callback expires the flow cookie with the handler's own function.
    #[tokio::test]
    async fn the_navigation_layer_redirects_instead_of_answering_json() {
        let policy = CookiePolicy::production();
        for on_limit in [OnLimit::KeepCookies, OnLimit::ExpireFlow(policy)] {
            let settings = RateSettings {
                max_keys: 1,
                clock: ServerClock::manual(),
            };
            let limiter = Arc::new(KeyedLimiter::new(AUTH_IP, &settings));
            let first = navigation_app(limiter.clone(), on_limit, context("203.0.113.7"));
            for _ in 0..5 {
                assert_eq!(call(&first).await.0, StatusCode::OK);
            }
            // Over the burst, then a new key with the table full (cap 1).
            let full = navigation_app(limiter, on_limit, context("198.51.100.1"));
            for app in [&first, &full] {
                let limited = response(app).await;
                assert_eq!(limited.status(), StatusCode::SEE_OTHER);
                let headers = limited.headers().clone();
                // Split: `/login` and its query are only written in `auth/callback.rs`.
                let location = ["/login", "?error=auth.try_again"].concat();
                assert_eq!(headers["location"], location.as_str());
                assert_eq!(headers["cache-control"], "no-store");
                assert!(headers.get("retry-after").is_none());
                assert!(headers.get("content-type").is_none());
                let cookies: Vec<_> = headers.get_all("set-cookie").iter().collect();
                match on_limit {
                    OnLimit::KeepCookies => assert!(cookies.is_empty(), "{cookies:?}"),
                    OnLimit::ExpireFlow(policy) => {
                        let handler = error_redirect(&policy, CallbackErrorCode::TryAgain);
                        let expected: Vec<_> =
                            handler.headers().get_all("set-cookie").iter().collect();
                        assert_eq!(cookies, expected);
                        assert_eq!(cookies, [&policy.expire_flow()]);
                    }
                }
                let body = to_bytes(limited.into_body(), 1024).await.unwrap();
                assert!(body.is_empty(), "{body:?}");
            }
        }
    }
}
