//! Error responses: always `ApiError { error: <code> }`, never text (ADR-0007).

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{ApiError, ApiErrorCode, ErrorCode};

/// Methods the API accepts until auth arrives (RF-B12).
pub const ALLOW_READ_ONLY: &str = "GET, HEAD, OPTIONS";

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
        .insert(header::ALLOW, HeaderValue::from_static(ALLOW_READ_ONLY));
    response
}

/// Fallback of the method routers (for example `OPTIONS /api/health`).
pub async fn method_not_allowed() -> Response {
    method_not_allowed_response()
}
