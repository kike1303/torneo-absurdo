//! The HTTP layer of a limiter (008 RF-A4): 429 `http.rate_limited` with `Retry-After`.
//! Phase A mounts it on no route; phase B mounts it on `start`, `link` and `callback`.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use protocol::ApiErrorCode;

use super::KeyedLimiter;
use crate::http::errors::too_many_requests;
use crate::net::{ClientContext, IpKey};

pub async fn layer(
    State(limiter): State<Arc<KeyedLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    // `net::layer` always runs first; without it, one shared bucket (more restrictive).
    let key = request
        .extensions()
        .get::<ClientContext>()
        .map_or(IpKey::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), |c| c.key);
    match limiter.check(&key).retry_after_secs() {
        None => next.run(request).await,
        Some(secs) => too_many_requests(ApiErrorCode::RateLimited, secs),
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
}
