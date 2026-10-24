//! HTTP router and its layers (RF-B12, RF-B15; plan-b decision 6).

pub mod errors;
pub mod health;
pub mod read_only;
pub mod request_log;

use axum::Router;
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware;
use axum::routing::get;
use tower::ServiceBuilder;
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::app::AppState;

const X_INTERNAL_SECRET: HeaderName = HeaderName::from_static("x-internal-secret");
const X_CLIENT_IP: HeaderName = HeaderName::from_static("x-client-ip");
const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// The router with every layer. No `CorsLayer` (ADR-0008 D4). Layers, outermost first:
/// sensitive request headers, request log (so 405s are logged), sensitive response headers,
/// `nosniff` + HSTS (on every response, errors included), read-only, routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/health",
            get(health::handler).fallback(errors::method_not_allowed),
        )
        .fallback(errors::not_found)
        .method_not_allowed_fallback(errors::method_not_allowed)
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(SetSensitiveRequestHeadersLayer::new([
                    header::COOKIE,
                    header::AUTHORIZATION,
                    X_INTERNAL_SECRET,
                    X_CLIENT_IP,
                    X_FORWARDED_FOR,
                ]))
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
                .layer(middleware::from_fn(read_only::layer)),
        )
}
