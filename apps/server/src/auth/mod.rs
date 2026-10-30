//! Authentication (ADR-0008, 008 phase B): the OAuth flow (`start` and `callback`), the login
//! transaction, link, reauth (009), unlink, logout and, with the `dev-auth` feature only, the dev
//! provider.

pub mod callback;
pub mod client;
#[cfg(feature = "dev-auth")]
pub mod dev;
pub mod link;
pub mod login;
pub mod logout;
pub mod next;
pub mod providers;
pub mod reauth;
pub mod reauth_status;
pub mod start;
pub mod unlink;

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

use crate::app::{AppState, Settings, StartupError};
use crate::config::{ClientSecret, Config, OAuthCredentials};
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
    /// The credentials of Google and Discord (`None`: off).
    pub credentials: OAuthCredentials,
    /// Their URLs (the constants, or a test's fakes).
    pub urls: providers::OAuthSettings,
    /// The enabled providers, in the order of the login buttons.
    pub providers: Vec<Provider>,
    /// The client of the token and userinfo requests (TLS only with a real provider).
    pub http: client::HttpClient,
    /// The pause of the link race tests ([`link::NoLinkGate`] in production).
    pub link_gate: link::LinkGateConfig,
    /// The pause of the unlink race tests ([`unlink::NoUnlinkGate`] in production, 009 RF15).
    pub unlink_gate: unlink::UnlinkGateConfig,
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
    /// The auth state, with the TLS config of the OAuth client (built here, once). Fails with
    /// [`StartupError::Tls`] if a real provider is on and the config cannot be built.
    pub fn from_config(config: &Config, settings: &Settings) -> Result<Self, StartupError> {
        let tls = tls_for(config)?;
        Ok(Self {
            cookies: config.cookies,
            ip_hash_key: config.ip_hash_key.clone(),
            daily_grant: config.daily_grant,
            alias: settings.alias.0.clone(),
            public_origin: config.public_origin.clone(),
            credentials: config.oauth.clone(),
            urls: settings.oauth.clone(),
            providers: enabled_providers(&config.oauth),
            http: client::HttpClient::new(tls),
            link_gate: settings.link_gate.clone(),
            unlink_gate: settings.unlink_gate.clone(),
            #[cfg(feature = "dev-auth")]
            dev: dev::DevProvider::default(),
        })
    }

    /// The provider of a URL segment, if it exists and is enabled.
    pub fn provider(&self, segment: &str) -> Option<Provider> {
        Provider::parse(segment).filter(|p| self.providers.contains(p))
    }

    /// The OAuth endpoints of `provider`. The browser-facing ones on `PUBLIC_ORIGIN`, the
    /// server-to-server ones (token, userinfo) on `local`, the address of this very server.
    pub fn endpoints(&self, provider: Provider, local: Option<SocketAddr>) -> Option<Endpoints> {
        let (credentials, urls) = match provider {
            Provider::Google => (&self.credentials.google, &self.urls.google),
            Provider::Discord => (&self.credentials.discord, &self.urls.discord),
            #[cfg(feature = "dev-auth")]
            Provider::Dev => return Some(dev::endpoints(&self.public_origin, local)),
            #[cfg(not(feature = "dev-auth"))]
            Provider::Dev => return None,
        };
        let _ = local;
        let credentials = credentials.as_ref()?;
        let urls = urls.clone().or_else(|| providers::production(provider))?;
        Some(Endpoints {
            client_id: credentials.id.clone(),
            client_secret: Some(credentials.secret.clone()),
            scope: providers::scope(provider),
            authorize: urls.authorize,
            token: Some(urls.token),
            userinfo: Some(urls.userinfo),
            redirect: format!(
                "{}/api/auth/{}/callback",
                self.public_origin,
                provider.as_str()
            ),
        })
    }
}

/// The TLS config of the OAuth client, only if a real provider is on (plan-c decision 4).
pub fn tls_for(config: &Config) -> Result<Option<Arc<rustls::ClientConfig>>, StartupError> {
    tls_for_with(
        !config.oauth.configured().is_empty(),
        client::production_config,
    )
}

fn tls_for_with(
    real_provider: bool,
    build: impl FnOnce() -> Result<Arc<rustls::ClientConfig>, rustls::Error>,
) -> Result<Option<Arc<rustls::ClientConfig>>, StartupError> {
    if !real_provider {
        return Ok(None);
    }
    build().map(Some).map_err(|_| {
        tracing::error!(event = "startup.tls_unavailable");
        StartupError::Tls
    })
}

/// Google and Discord with their credentials, then `dev` with the feature.
fn enabled_providers(credentials: &OAuthCredentials) -> Vec<Provider> {
    let mut providers = Vec::new();
    if credentials.google.is_some() {
        providers.push(Provider::Google);
    }
    if credentials.discord.is_some() {
        providers.push(Provider::Discord);
    }
    #[cfg(feature = "dev-auth")]
    providers.push(Provider::Dev);
    providers
}

/// The OAuth endpoints of one provider.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub client_id: String,
    /// Google and Discord; the dev provider has none. `Debug` redacts it.
    pub client_secret: Option<ClientSecret>,
    /// The only scope asked for (none for `dev`).
    pub scope: Option<&'static str>,
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
mod tls_at_startup {
    use super::*;
    use crate::app::StartupError;

    /// plan-c decision 4: a TLS config that cannot be built stops the start, never a server
    /// whose real providers would always fail. `ring` never fails here, so the build is
    /// injected.
    #[test]
    fn a_tls_config_that_cannot_be_built_is_a_startup_error() {
        let failed = tls_for_with(true, || Err(rustls::Error::General("canary".to_owned())));
        assert_eq!(failed.err(), Some(StartupError::Tls));
        assert!(matches!(
            tls_for_with(true, client::production_config),
            Ok(Some(_))
        ));
    }

    #[test]
    fn without_a_real_provider_no_tls_config_is_built() {
        let built = tls_for_with(false, || -> Result<_, rustls::Error> {
            panic!("built without a real provider")
        });
        assert!(matches!(built, Ok(None)));
    }
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

    /// ADR-0012 B4.6 (security-reviewer, recommendation 1): the two provider origins of
    /// `form-action` are named only by the URL constants of `auth/providers.rs`, so no route can
    /// redirect to another path of those origins. The needles are split, so this test never
    /// counts itself, and the test module of `providers.rs` (which pins the constants) is not
    /// code.
    #[test]
    fn the_provider_origins_live_only_in_the_provider_constants() {
        let origins = [
            ["accounts", ".google", ".com"].concat(),
            ["discord", ".com"].concat(),
        ];
        let mut seen = 0;
        for (path, text) in sources() {
            let lower = text.to_lowercase();
            if !path.ends_with("auth/providers.rs") {
                for origin in &origins {
                    assert!(!lower.contains(origin), "{} names {origin}", path.display());
                }
                continue;
            }
            let code = lower.split("#[cfg(test)]").next().unwrap_or_default();
            for line in code.lines() {
                if origins.iter().any(|origin| line.contains(origin.as_str())) {
                    seen += 1;
                    assert!(
                        line.trim_start().starts_with("pub const "),
                        "providers.rs names a provider origin outside its constants: {line}"
                    );
                }
            }
        }
        // `GOOGLE_AUTHORIZE` and the three of Discord (`oauth2.googleapis.com` and
        // `openidconnect.googleapis.com` are other hosts).
        assert_eq!(seen, 4);
    }

    /// B4.6: a redirect of ours never keeps the method and the body (307, 308). The redirect of
    /// `start` and `link` to the provider is a 303; `tests/oauth_providers.rs` reads its status.
    #[test]
    fn no_redirect_of_the_server_is_307_or_308() {
        let needles = [
            ["Redirect::", "temporary"].concat(),
            ["Redirect::", "permanent"].concat(),
            ["TEMPORARY", "_REDIRECT"].concat(),
            ["PERMANENT", "_REDIRECT"].concat(),
            ["from_u16(", "307"].concat(),
            ["from_u16(", "308"].concat(),
        ];
        let files = sources();
        assert!(
            files
                .iter()
                .any(|(path, _)| path.ends_with("auth/start.rs"))
        );
        for (path, text) in files {
            for needle in &needles {
                assert!(
                    !text.contains(needle.as_str()),
                    "{} uses {needle}",
                    path.display()
                );
            }
        }
    }

    /// Every `Location` to the login page (the errors, and the bare `/login` of a `link` without
    /// a session) is a literal of `auth/callback.rs` and of no other code of `src/` (008
    /// "Enmienda 2026-10-30" of RF-B5 and RF-C2). Test modules are not code.
    #[test]
    fn every_login_location_is_built_in_the_callback_module() {
        let needle = ["\"", "/login"].concat();
        let hits: Vec<_> = sources()
            .into_iter()
            .map(|(path, text)| {
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                let count = code.matches(&needle).count();
                (path, count)
            })
            .filter(|(_, count)| *count > 0)
            .collect();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].0.ends_with("auth/callback.rs"), "{hits:?}");
        // `error_location` and `login_location`.
        assert_eq!(hits[0].1, 2, "{hits:?}");
    }
}
