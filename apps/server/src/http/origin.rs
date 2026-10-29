//! `Origin` check of every unsafe request (008 RF-A5, ADR-0008 D4). Replaces the read-only
//! layer: `GET`, `HEAD` and `OPTIONS` pass; any other method needs a same-origin proof, or
//! it gets 403 `auth.csrf_origin` before reaching a handler (routes, `fallback` and 405
//! included).

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use protocol::AuthErrorCode;

use super::errors::error_response;
use crate::net::ClientContext;

pub const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
pub const X_CLIENT_ORIGIN: HeaderName = HeaderName::from_static("x-client-origin");
pub const X_CLIENT_FETCH_SITE: HeaderName = HeaderName::from_static("x-client-fetch-site");

const SAME_ORIGIN: &[u8] = b"same-origin";

/// The exact `PUBLIC_ORIGIN` a request must come from.
#[derive(Debug, Clone)]
pub struct OriginPolicy {
    public_origin: String,
}

impl OriginPolicy {
    pub fn new(public_origin: String) -> Self {
        Self { public_origin }
    }

    /// Whether a request with this method and headers may reach the router.
    pub fn allows(&self, method: &Method, headers: &HeaderMap, internal: bool) -> bool {
        if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
            return true;
        }
        let origin = self.public_origin.as_bytes();
        if internal {
            // `web` forwards the browser's headers; its own `Origin` is not looked at.
            return exactly_one(headers, &X_CLIENT_ORIGIN) == Some(Some(origin))
                && matches!(
                    exactly_one(headers, &X_CLIENT_FETCH_SITE),
                    None | Some(Some(SAME_ORIGIN))
                );
        }
        match exactly_one(headers, &header::ORIGIN) {
            Some(value) => value == Some(origin),
            None => exactly_one(headers, &SEC_FETCH_SITE) == Some(Some(SAME_ORIGIN)),
        }
    }
}

/// `None` if the header is absent, `Some(Some(bytes))` if it appears exactly once and
/// `Some(None)` if it is repeated (never valid).
fn exactly_one<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<Option<&'a [u8]>> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?;
    Some(values.next().is_none().then(|| first.as_bytes()))
}

pub async fn layer(
    State(policy): State<Arc<OriginPolicy>>,
    request: Request,
    next: Next,
) -> Response {
    let internal = request
        .extensions()
        .get::<ClientContext>()
        .is_some_and(|context| context.internal);
    if policy.allows(request.method(), request.headers(), internal) {
        next.run(request).await
    } else {
        error_response(StatusCode::FORBIDDEN, AuthErrorCode::CsrfOrigin)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    const ORIGIN: &str = "http://localhost:5173";

    fn policy() -> OriginPolicy {
        OriginPolicy::new(ORIGIN.to_owned())
    }

    fn headers(pairs: &[(HeaderName, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(name.clone(), HeaderValue::from_str(value).unwrap());
        }
        map
    }

    fn unsafe_methods() -> Vec<Method> {
        ["POST", "PUT", "PATCH", "DELETE", "PURGE"]
            .iter()
            .map(|m| Method::from_bytes(m.as_bytes()).unwrap())
            .collect()
    }

    #[test]
    fn safe_methods_pass_without_any_check() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            let foreign = headers(&[(header::ORIGIN, "https://evil.example")]);
            assert!(policy().allows(&method, &foreign, false));
            assert!(policy().allows(&method, &HeaderMap::new(), true));
        }
    }

    #[test]
    fn browser_requests() {
        let cases: [(&[(HeaderName, &str)], bool); 10] = [
            (&[(header::ORIGIN, ORIGIN)], true),
            (&[(SEC_FETCH_SITE, "same-origin")], true),
            (&[], false),
            (&[(header::ORIGIN, "https://evil.example")], false),
            (&[(header::ORIGIN, "null")], false),
            (&[(header::ORIGIN, ORIGIN), (header::ORIGIN, ORIGIN)], false),
            (&[(SEC_FETCH_SITE, "same-site")], false),
            (&[(SEC_FETCH_SITE, "cross-site")], false),
            (&[(SEC_FETCH_SITE, "none")], false),
            // An `Origin` decides on its own: a valid Sec-Fetch-Site does not rescue it.
            (
                &[
                    (header::ORIGIN, "https://evil.example"),
                    (SEC_FETCH_SITE, "same-origin"),
                ],
                false,
            ),
        ];
        for method in unsafe_methods() {
            for (pairs, expected) in &cases {
                assert_eq!(
                    policy().allows(&method, &headers(pairs), false),
                    *expected,
                    "{method} {pairs:?}"
                );
            }
        }
    }

    #[test]
    fn internal_requests() {
        let cases: [(&[(HeaderName, &str)], bool); 9] = [
            (&[(X_CLIENT_ORIGIN, ORIGIN)], true),
            (
                &[
                    (X_CLIENT_ORIGIN, ORIGIN),
                    (X_CLIENT_FETCH_SITE, "same-origin"),
                ],
                true,
            ),
            // web's own Origin is not looked at.
            (
                &[
                    (X_CLIENT_ORIGIN, ORIGIN),
                    (header::ORIGIN, "https://evil.example"),
                ],
                true,
            ),
            (&[(header::ORIGIN, ORIGIN)], false),
            (&[(X_CLIENT_ORIGIN, "https://evil.example")], false),
            (
                &[(X_CLIENT_ORIGIN, ORIGIN), (X_CLIENT_ORIGIN, ORIGIN)],
                false,
            ),
            (
                &[
                    (X_CLIENT_ORIGIN, ORIGIN),
                    (X_CLIENT_FETCH_SITE, "cross-site"),
                ],
                false,
            ),
            (
                &[
                    (X_CLIENT_ORIGIN, ORIGIN),
                    (X_CLIENT_FETCH_SITE, "same-site"),
                ],
                false,
            ),
            (
                &[(X_CLIENT_ORIGIN, ORIGIN), (X_CLIENT_FETCH_SITE, "none")],
                false,
            ),
        ];
        for method in unsafe_methods() {
            for (pairs, expected) in &cases {
                assert_eq!(
                    policy().allows(&method, &headers(pairs), true),
                    *expected,
                    "{method} {pairs:?}"
                );
            }
        }
    }

    #[test]
    fn a_repeated_fetch_site_is_refused() {
        let pairs = [
            (X_CLIENT_ORIGIN, ORIGIN),
            (X_CLIENT_FETCH_SITE, "same-origin"),
            (X_CLIENT_FETCH_SITE, "same-origin"),
        ];
        assert!(!policy().allows(&Method::POST, &headers(&pairs), true));
        let pairs = [
            (SEC_FETCH_SITE, "same-origin"),
            (SEC_FETCH_SITE, "same-origin"),
        ];
        assert!(!policy().allows(&Method::POST, &headers(&pairs), false));
    }
}
