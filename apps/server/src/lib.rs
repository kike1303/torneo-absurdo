//! Torneo Absurdo API server (`apps/server`): health, the WebSocket and, later, the domain.
//!
//! The binary (`main.rs`) is a thin wrapper; everything lives here so integration tests can
//! build the router and the state in process.

pub mod config;
pub mod telemetry;
pub mod version;
pub mod version_rule;
