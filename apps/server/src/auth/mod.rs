//! Authentication (ADR-0008, 008 phase B): the OAuth flow (`start` and `callback`), the login
//! transaction, logout and, with the `dev-auth` feature only, the dev provider.

pub mod callback;
pub mod client;
#[cfg(feature = "dev-auth")]
pub mod dev;
pub mod login;
pub mod logout;
pub mod next;
pub mod start;

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::Provider;
use rand_core::{OsRng, TryRngCore};
use sha2::{Digest, Sha256};

use crate::app::{AppState, Settings};
use crate::config::Config;
use crate::http::cookies::CookiePolicy;
use crate::ledger::Coins;
use crate::net::IpHashKey;
use crate::profile::alias::AliasSource;

/// What the auth routes need from the config and the settings.
pub struct AuthState {
    pub cookies: CookiePolicy,
    pub ip_hash_key: IpHashKey,
    pub daily_grant: Coins,
    pub alias: Arc<dyn AliasSource>,
    pub public_origin: String,
    /// The enabled providers, in the order of the login buttons.
    pub providers: Vec<Provider>,
    #[cfg(feature = "dev-auth")]
    pub dev: dev::DevProvider,
}

impl fmt::Debug for AuthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthState")
            .field("providers", &self.providers)
            .finish_non_exhaustive()
    }
}

impl AuthState {
    pub fn from_config(config: &Config, settings: &Settings) -> Self {
        Self {
            cookies: config.cookies,
            ip_hash_key: config.ip_hash_key.clone(),
            daily_grant: config.daily_grant,
            alias: settings.alias.0.clone(),
            public_origin: config.public_origin.clone(),
            providers: enabled_providers(),
            #[cfg(feature = "dev-auth")]
            dev: dev::DevProvider::default(),
        }
    }

    /// The provider of a URL segment, if it exists and is enabled.
    pub fn provider(&self, segment: &str) -> Option<Provider> {
        Provider::parse(segment).filter(|p| self.providers.contains(p))
    }

    /// The OAuth endpoints of `provider`. The browser-facing ones on `PUBLIC_ORIGIN`, the
    /// server-to-server ones (token, userinfo) on `local`, the address of this very server.
    pub fn endpoints(&self, provider: Provider, local: Option<SocketAddr>) -> Option<Endpoints> {
        match provider {
            #[cfg(feature = "dev-auth")]
            Provider::Dev => Some(dev::endpoints(&self.public_origin, local)),
            _ => {
                let _ = local;
                None
            }
        }
    }
}

/// Google and Discord arrive with phase C; `dev` only with the feature.
fn enabled_providers() -> Vec<Provider> {
    #[cfg(feature = "dev-auth")]
    {
        vec![Provider::Dev]
    }
    #[cfg(not(feature = "dev-auth"))]
    {
        Vec::new()
    }
}

/// The OAuth endpoints of one provider.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub client_id: String,
    pub authorize: String,
    pub token: Option<String>,
    pub userinfo: Option<String>,
    pub redirect: String,
}

/// `n` bytes of `OsRng` in base64url without padding (`state`, `code_verifier`, ids).
pub fn random_text(n: usize) -> Option<String> {
    let mut bytes = vec![0u8; n];
    OsRng.try_fill_bytes(&mut bytes).ok()?;
    Some(URL_SAFE_NO_PAD.encode(bytes))
}

/// `SHA-256` of a flow id: what `oauth_flows.id_hash` stores.
pub fn flow_hash(id: &str) -> [u8; 32] {
    Sha256::digest(id.as_bytes()).into()
}

/// `GET /api/auth/providers` (RF-B13).
pub async fn providers(State(state): State<AppState>) -> Json<Vec<Provider>> {
    Json(state.auth.providers.clone())
}

#[cfg(test)]
mod source_rules {
    use std::path::{Path, PathBuf};

    fn sources() -> Vec<(PathBuf, String)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.push((path, text));
                }
            }
        }
        out
    }

    /// `state` and `code_verifier` come from `OsRng`, never from the `rand 0.8` of `oauth2`
    /// (008 plan-b decision 1).
    #[test]
    fn oauth2_random_constructors_are_never_used() {
        let needle = ["new_", "random"].concat();
        for (path, text) in sources() {
            assert!(!text.contains(&needle), "{} uses {needle}", path.display());
        }
    }

    /// ADR-0001: `OsRng` is for tokens, `state`, `code_verifier` and the alias only, never in
    /// the engine or any other crate of `crates/` (008 plan-b decision 14).
    #[test]
    fn os_rng_never_enters_the_crates() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
        let needle = ["Os", "Rng"].concat();
        let mut stack = vec![crates];
        let mut files = 0;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if !path.ends_with("target") {
                        stack.push(path);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files += 1;
                    let text = std::fs::read_to_string(&path).unwrap();
                    assert!(!text.contains(&needle), "{} uses {needle}", path.display());
                }
            }
        }
        assert!(files > 10, "{files}");
    }

    /// The `Location` of a callback error is built in one place, from `CallbackErrorCode` only
    /// (ADR-0008, amendment 2026-10-28).
    #[test]
    fn the_error_location_is_built_once() {
        let needle = ["/login", "?error="].concat();
        let hits: Vec<_> = sources()
            .into_iter()
            .filter(|(_, text)| text.contains(&needle))
            .map(|(path, text)| (path, text.matches(&needle).count()))
            .collect();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].1, 1, "{hits:?}");
        assert!(hits[0].0.ends_with("auth/callback.rs"), "{hits:?}");
    }
}
