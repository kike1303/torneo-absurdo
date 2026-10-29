//! Cookie names and attributes (008 RF-B9): one policy, derived from the config at startup.
//!
//! Production: `__Host-session` and `__Host-oauth` with `Secure; HttpOnly; SameSite=Lax; Path=/`
//! and no `Domain`. It is the only policy a binary without the `dev-auth` feature can build.
//! The loopback development policy (`ta_dev_session`, `ta_dev_oauth`, without `Secure`) exists
//! only with the feature and a loopback `http` origin. The server reads one name per cookie:
//! the one of the active policy.

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
}
