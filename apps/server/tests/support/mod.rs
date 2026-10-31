//! Helpers shared by the integration tests. Each test crate uses a subset.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

pub mod app;
pub mod auth;
pub mod bin;
pub mod config;
pub mod db;
pub mod fake_provider;
pub mod fights;
pub mod http;
pub mod logs;
pub mod oauth;
pub mod pg_proxy;
pub mod public_verifier;
pub mod ws;

/// A port that was free a moment ago (the listener is dropped before returning).
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
