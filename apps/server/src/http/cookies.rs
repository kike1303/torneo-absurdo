//! Cookie names and attributes (008 RF-B9): one policy, derived from the config at startup.
//!
//! Production: `__Host-session` and `__Host-oauth` with `Secure; HttpOnly; SameSite=Lax; Path=/`
//! and no `Domain`. It is the only policy a binary without the `dev-auth` feature can build.
//! The loopback development policy (`ta_dev_session`, `ta_dev_oauth`, without `Secure`) exists
//! only with the feature and a loopback `http` origin. The server reads one name per cookie:
//! the one of the active policy.

use axum::http::{HeaderMap, HeaderValue, header};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CookiePolicy {
    session_name: &'static str,
    flow_name: &'static str,
    secure: bool,
}

impl CookiePolicy {
    pub const fn production() -> Self {
        Self {
            session_name: "__Host-session",
            flow_name: "__Host-oauth",
            secure: true,
        }
    }

    /// `http://localhost` development (WebKit drops `Secure` cookies over plain http).
    #[cfg(feature = "dev-auth")]
    pub const fn loopback_dev() -> Self {
        Self {
            session_name: "ta_dev_session",
            flow_name: "ta_dev_oauth",
            secure: false,
        }
    }

    pub const fn session_name(&self) -> &'static str {
        self.session_name
    }

    pub const fn flow_name(&self) -> &'static str {
        self.flow_name
    }

    pub const fn secure(&self) -> bool {
        self.secure
    }

    /// The session cookie of this policy: exactly one cookie with exactly its name, whose value
    /// is base64url (what the server issues). Any other cookie name, a repeated cookie or
    /// another value means no session.
    pub fn read_session<'a>(&self, headers: &'a HeaderMap) -> Option<&'a str> {
        read(headers, self.session_name)
    }

    /// The flow cookie of this policy, with the same rules.
    pub fn read_flow<'a>(&self, headers: &'a HeaderMap) -> Option<&'a str> {
        read(headers, self.flow_name)
    }

    /// `Set-Cookie` of a new session, with `Max-Age` equal to its absolute expiry.
    pub fn set_session(&self, token: &str, max_age_secs: u64) -> HeaderValue {
        self.cookie(self.session_name, token, max_age_secs)
    }

    /// `Set-Cookie` of a new OAuth flow (10 minutes, ADR-0008 D2).
    pub fn set_flow(&self, id: &str) -> HeaderValue {
        self.cookie(self.flow_name, id, FLOW_MAX_AGE_SECS)
    }

    pub fn expire_session(&self) -> HeaderValue {
        self.cookie(self.session_name, "", 0)
    }

    pub fn expire_flow(&self) -> HeaderValue {
        self.cookie(self.flow_name, "", 0)
    }

    fn cookie(&self, name: &str, value: &str, max_age_secs: u64) -> HeaderValue {
        let secure = if self.secure { "; Secure" } else { "" };
        let text = format!(
            "{name}={value}; Max-Age={max_age_secs}; Path=/; HttpOnly; SameSite=Lax{secure}"
        );
        // Names and values are base64url and ASCII constants: always a valid header value.
        let mut value =
            HeaderValue::from_str(&text).unwrap_or_else(|_| HeaderValue::from_static(""));
        value.set_sensitive(true);
        value
    }
}

/// Lifetime of the flow cookie and of `oauth_flows` rows.
pub const FLOW_MAX_AGE_SECS: u64 = 600;

fn is_base64url(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn read<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut found = None;
    for line in headers.get_all(header::COOKIE) {
        let Ok(text) = line.to_str() else {
            continue;
        };
        for pair in text.split(';') {
            let Some((key, value)) = pair.trim().split_once('=') else {
                continue;
            };
            if key != name {
                continue;
            }
            if found.is_some() {
                // Two cookies with the name: ambiguous, so none.
                return None;
            }
            found = Some(value);
        }
    }
    found.filter(|value| is_base64url(value))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::*;

    fn cookies(lines: &[&str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for line in lines {
            map.append(header::COOKIE, HeaderValue::from_str(line).unwrap());
        }
        map
    }

    const P: CookiePolicy = CookiePolicy::production();

    #[test]
    fn reads_only_the_session_cookie_of_the_policy() {
        let map = cookies(&["theme=dark; __Host-session=abc_-1; locale=es"]);
        assert_eq!(P.read_session(&map), Some("abc_-1"));
        assert_eq!(P.read_flow(&map), None);
        // Across several Cookie lines too.
        let map = cookies(&["theme=dark", "__Host-oauth=flow1"]);
        assert_eq!(P.read_flow(&map), Some("flow1"));
        // The development name is ignored by the production policy (RF-B9).
        let map = cookies(&["ta_dev_session=abc"]);
        assert_eq!(P.read_session(&map), None);
        // A prefix or suffix of the name is another cookie.
        let map = cookies(&["x__Host-session=a; __Host-session2=b"]);
        assert_eq!(P.read_session(&map), None);
    }

    #[test]
    fn a_repeated_cookie_is_no_cookie() {
        for lines in [
            &["__Host-session=a; __Host-session=b"][..],
            &["__Host-session=a", "__Host-session=a"][..],
        ] {
            assert_eq!(P.read_session(&cookies(lines)), None, "{lines:?}");
        }
    }

    #[test]
    fn empty_or_non_ascii_values_are_no_cookie() {
        assert_eq!(P.read_session(&cookies(&["__Host-session="])), None);
        let mut map = HeaderMap::new();
        map.append(
            header::COOKIE,
            HeaderValue::from_bytes(b"__Host-session=\xff").unwrap(),
        );
        assert_eq!(P.read_session(&map), None);
    }

    #[test]
    fn set_cookie_has_the_fixed_attributes() {
        assert_eq!(
            P.set_session("tok", 2_592_000).to_str().unwrap(),
            "__Host-session=tok; Max-Age=2592000; Path=/; HttpOnly; SameSite=Lax; Secure"
        );
        assert_eq!(
            P.set_flow("fid").to_str().unwrap(),
            "__Host-oauth=fid; Max-Age=600; Path=/; HttpOnly; SameSite=Lax; Secure"
        );
        assert_eq!(
            P.expire_session().to_str().unwrap(),
            "__Host-session=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax; Secure"
        );
        assert_eq!(
            P.expire_flow().to_str().unwrap(),
            "__Host-oauth=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax; Secure"
        );
        for value in [P.set_session("t", 1), P.set_flow("f")] {
            assert!(value.is_sensitive());
            assert!(!value.to_str().unwrap().contains("Domain"));
        }
    }

    #[cfg(feature = "dev-auth")]
    #[test]
    fn the_development_policy_has_no_secure() {
        let dev = CookiePolicy::loopback_dev();
        assert_eq!(
            dev.set_session("tok", 60).to_str().unwrap(),
            "ta_dev_session=tok; Max-Age=60; Path=/; HttpOnly; SameSite=Lax"
        );
        assert_eq!(dev.read_session(&cookies(&["__Host-session=a"])), None);
        assert_eq!(dev.read_session(&cookies(&["ta_dev_session=a"])), Some("a"));
    }
}
