//! Torneo Absurdo API server (`apps/server`): health, the WebSocket and, later, the domain.
//!
//! The binary (`main.rs`) is a thin wrapper; everything lives here so integration tests can
//! build the router and the state in process.

pub mod admin;
pub mod app;
pub mod auth;
mod cli;
pub mod config;
pub mod db;
pub mod healthcheck;
pub mod http;
pub mod jobs;
pub mod ledger;
pub mod net;
pub mod profile;
pub mod rate;
pub mod sessions;
pub mod shutdown;
pub mod telemetry;
pub mod version;
pub mod version_rule;
pub mod ws;

pub use cli::{CliError, Command, exit_code, parse, run_cli};
