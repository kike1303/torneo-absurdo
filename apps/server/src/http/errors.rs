//! Error responses: always `ApiError { error: <code> }`, never text (ADR-0007).

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{ApiError, ApiErrorCode, ErrorCode};

/// The safe methods: the `Allow` of the 405 of the read-only routes (`/api/health`).
pub const ALLOW_SAFE: &str = "GET, HEAD, OPTIONS";

pub fn error_response(status: StatusCode, code: impl Into<ErrorCode>) -> Response {
    (status, Json(ApiError::new(code))).into_response()
}

/// 404 `http.not_found` (router fallback).
pub async fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, ApiErrorCode::NotFound)
}

/// 405 `http.method_not_allowed` with `Allow: GET, HEAD, OPTIONS`.
pub fn method_not_allowed_response() -> Response {
    let mut response = error_response(
        StatusCode::METHOD_NOT_ALLOWED,
        ApiErrorCode::MethodNotAllowed,
    );
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(ALLOW_SAFE));
    response
}

/// Fallback of the method routers (for example `OPTIONS /api/health`).
pub async fn method_not_allowed() -> Response {
    method_not_allowed_response()
}

/// 422 `http.invalid_body` for every rejection of a JSON body (008 RF-C5, amendment
/// 2026-10-29): malformed JSON, a field the type does not have, a wrong type or a value outside
/// the enum, a `Content-Type` that is not JSON, and a body over the limit. Never axum's or
/// serde's text, which copies parts of the body.
pub fn invalid_body(rejection: &JsonRejection) -> Response {
    match rejection {
        JsonRejection::JsonDataError(_)
        | JsonRejection::JsonSyntaxError(_)
        | JsonRejection::MissingJsonContentType(_)
        | JsonRejection::BytesRejection(_) => {}
        // `#[non_exhaustive]`: a variant of a newer axum is a rejected body too.
        _ => {}
    }
    error_response(StatusCode::UNPROCESSABLE_ENTITY, ApiErrorCode::InvalidBody)
}

/// 429 with `code` and `Retry-After: secs` (008 RF-A4, RF-A7).
pub fn too_many_requests(code: impl Into<ErrorCode>, secs: u64) -> Response {
    let mut response = error_response(StatusCode::TOO_MANY_REQUESTS, code);
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(secs));
    response
}

#[cfg(test)]
mod tests {
    /// The four variants of `JsonRejection` that `invalid_body` lists are those of axum 0.8.9.
    /// A bump of axum fails here, so whoever bumps it checks the list (plan-c decision 8).
    #[test]
    fn the_known_json_rejections_are_those_of_axum_0_8_9() {
        let lock = include_str!("../../../../Cargo.lock");
        let axum = lock
            .split("[[package]]")
            .find(|p| p.contains("\nname = \"axum\"\n"))
            .unwrap();
        assert!(axum.contains("\nversion = \"0.8.9\"\n"), "{axum}");
    }
}
