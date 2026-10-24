//! Read-only API until auth (RF-B12): every method other than `GET`, `HEAD` and `OPTIONS`
//! gets 405 before reaching a handler.
//!
//! The auth feature replaces this layer with the `Origin` middleware of ADR-0008 D4, so no
//! mutation can be added without removing it on purpose.

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

use super::errors::method_not_allowed_response;

pub async fn layer(request: Request, next: Next) -> Response {
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        next.run(request).await
    } else {
        method_not_allowed_response()
    }
}
