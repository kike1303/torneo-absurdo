//! One `info` line per HTTP request (RF-B15, plan-b decision 5).
//!
//! Only `method` (from a closed set), `route` (the `MatchedPath` template, or `unmatched`),
//! `status`, `latency_ms` and a random `request_id`. Headers, URI, query and body are never
//! read here.

use axum::extract::{MatchedPath, Request};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use tokio::time::Instant;
use uuid::Uuid;

/// The method as a code; any extension method is `other` (never the client's text).
pub fn method_code(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        _ => "other",
    }
}

pub async fn layer(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = method_code(request.method());
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_owned(), |path| path.as_str().to_owned());
    let response = next.run(request).await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(
        event = "http.request",
        method,
        route = route.as_str(),
        status = response.status().as_u16(),
        latency_ms,
        request_id = %Uuid::new_v4(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_come_from_a_closed_set() {
        assert_eq!(method_code(&Method::GET), "GET");
        assert_eq!(method_code(&Method::DELETE), "DELETE");
        assert_eq!(method_code(&Method::TRACE), "other");
        assert_eq!(method_code(&Method::CONNECT), "other");
        assert_eq!(method_code(&Method::from_bytes(b"PURGE").unwrap()), "other");
    }
}
