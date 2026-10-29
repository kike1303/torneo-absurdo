//! Internal requests from `web` (008 RF-A6, ADR-0008 D5, server side).

use std::fmt;

use axum::http::{HeaderMap, HeaderName};
use subtle::ConstantTimeEq;

pub const X_INTERNAL_SECRET: HeaderName = HeaderName::from_static("x-internal-secret");

/// `INTERNAL_API_SECRET`, as the base64url text it arrives in (the header carries the same
/// text). `Debug` never shows it.
#[derive(Clone)]
pub struct InternalSecret(Box<[u8]>);

impl InternalSecret {
    /// Wraps the already validated text (see `config`).
    pub fn new(text: &str) -> Self {
        Self(text.as_bytes().into())
    }
}

impl fmt::Debug for InternalSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InternalSecret(..)")
    }
}

/// Whether the request carries exactly one `X-Internal-Secret` equal to the secret, compared
/// in constant time. Without a secret, or with a missing, repeated or wrong header, the
/// request is external (no error of its own). The length is not secret (the `.env.example`
/// shows it), so `ct_eq` returning false on a length mismatch is fine.
pub fn is_internal(headers: &HeaderMap, secret: Option<&InternalSecret>) -> bool {
    let Some(secret) = secret else {
        return false;
    };
    let mut values = headers.get_all(X_INTERNAL_SECRET).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => bool::from(value.as_bytes().ct_eq(&secret.0)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    const SECRET: &str = "c2VjcmV0LXNlY3JldC1zZWNyZXQtc2VjcmV0LXNlY3JldA";

    fn with(values: &[&[u8]]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append(X_INTERNAL_SECRET, HeaderValue::from_bytes(value).unwrap());
        }
        map
    }

    #[test]
    fn only_exactly_one_matching_header_is_internal() {
        let secret = InternalSecret::new(SECRET);
        let some = Some(&secret);
        assert!(is_internal(&with(&[SECRET.as_bytes()]), some));
        let mut wrong = SECRET.as_bytes().to_vec();
        wrong[0] ^= 1;
        assert!(!is_internal(&with(&[&wrong]), some), "same length");
        assert!(!is_internal(&with(&[b"short"]), some), "other length");
        assert!(
            !is_internal(&with(&[SECRET.as_bytes(), SECRET.as_bytes()]), some),
            "repeated"
        );
        let mut non_ascii = SECRET.as_bytes().to_vec();
        non_ascii[0] = 0xff;
        assert!(!is_internal(&with(&[&non_ascii]), some), "non-ASCII");
        assert!(!is_internal(&HeaderMap::new(), some), "missing");
        assert!(
            !is_internal(&with(&[SECRET.as_bytes()]), None),
            "no secret configured"
        );
    }

    #[test]
    fn debug_never_shows_the_secret() {
        assert_eq!(
            format!("{:?}", InternalSecret::new(SECRET)),
            "InternalSecret(..)"
        );
    }
}
