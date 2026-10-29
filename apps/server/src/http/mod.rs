//! HTTP router and its layers (RF-B12, RF-B15; plan-b decision 6; 008 RF-A5).

pub mod errors;
pub mod health;
pub mod origin;
pub mod request_log;
pub mod routes;

use axum::Router;
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware;
use tower::ServiceBuilder;
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::app::AppState;
use crate::net::client_ip::{X_CLIENT_IP, X_FORWARDED_FOR};
use crate::net::internal::X_INTERNAL_SECRET;
use origin::{SEC_FETCH_SITE, X_CLIENT_FETCH_SITE, X_CLIENT_ORIGIN};

const X_REAL_IP: HeaderName = HeaderName::from_static("x-real-ip");
const CROSS_ORIGIN_RESOURCE_POLICY: HeaderName =
    HeaderName::from_static("cross-origin-resource-policy");

/// ADR-0012 B2 for the server: nothing to load and nothing may frame it.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; frame-ancestors 'none'";

/// Request headers whose values are marked sensitive, so no `Debug` of them prints the value
/// (ADR-0012 A4.1 and A4.3; 008 RF-A9).
const SENSITIVE_REQUEST_HEADERS: [HeaderName; 10] = [
    header::COOKIE,
    header::AUTHORIZATION,
    X_INTERNAL_SECRET,
    X_CLIENT_IP,
    X_FORWARDED_FOR,
    X_REAL_IP,
    header::FORWARDED,
    X_CLIENT_ORIGIN,
    X_CLIENT_FETCH_SITE,
    SEC_FETCH_SITE,
];

/// The router with every layer. No `CorsLayer` (ADR-0008 D4). Layers, outermost first:
/// sensitive request headers, request log (so 405s are logged), sensitive response headers,
/// `nosniff`, HSTS, CORP and CSP (on every response, errors included), the client context
/// (`net::layer`), the `Origin` check (before routing, so it also covers the fallback and the
/// 405s) and the routes of the manifest.
pub fn router(state: AppState) -> Router {
    let net = state.net.clone();
    let origin = state.origin.clone();
    routes::register(Router::new(), &state)
        .fallback(errors::not_found)
        .method_not_allowed_fallback(errors::method_not_allowed)
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(SetSensitiveRequestHeadersLayer::new(
                    SENSITIVE_REQUEST_HEADERS,
                ))
                .layer(middleware::from_fn(request_log::layer))
                .layer(SetSensitiveResponseHeadersLayer::new([header::SET_COOKIE]))
                .layer(SetResponseHeaderLayer::overriding(
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::STRICT_TRANSPORT_SECURITY,
                    HeaderValue::from_static("max-age=31536000"),
                ))
                // ADR-0012 B2: JSON responses have nothing to load or frame.
                .layer(SetResponseHeaderLayer::overriding(
                    CROSS_ORIGIN_RESOURCE_POLICY,
                    HeaderValue::from_static("same-origin"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::CONTENT_SECURITY_POLICY,
                    HeaderValue::from_static(CONTENT_SECURITY_POLICY),
                ))
                .layer(middleware::from_fn_with_state(net, crate::net::layer))
                .layer(middleware::from_fn_with_state(origin, origin::layer)),
        )
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::http::{Request, Response};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn sensitive_request_headers_are_marked() {
        let names = [
            "cookie",
            "authorization",
            "x-internal-secret",
            "x-client-ip",
            "x-forwarded-for",
            "x-real-ip",
            "forwarded",
            "x-client-origin",
            "x-client-fetch-site",
            "sec-fetch-site",
        ];
        assert_eq!(names.len(), SENSITIVE_REQUEST_HEADERS.len());
        // The inner service answers with the names of the headers that are not sensitive.
        let service = ServiceBuilder::new()
            .layer(SetSensitiveRequestHeadersLayer::new(
                SENSITIVE_REQUEST_HEADERS,
            ))
            .service_fn(|request: Request<()>| async move {
                let exposed: Vec<String> = request
                    .headers()
                    .iter()
                    .filter(|(_, value)| !value.is_sensitive())
                    .map(|(name, _)| name.to_string())
                    .collect();
                Ok::<_, Infallible>(Response::new(exposed))
            });
        let request = names
            .iter()
            .fold(Request::builder(), |builder, name| {
                builder.header(*name, "canary")
            })
            .body(())
            .unwrap();
        let response = service.oneshot(request).await.unwrap();
        assert!(response.body().is_empty(), "{:?}", response.body());
    }
}
