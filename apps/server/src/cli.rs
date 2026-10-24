//! Subcommands (plan-b decision 2): `server` / `server serve` and `server healthcheck`.
//! No error is ever formatted to stdout or stderr: failures become exit codes and, for
//! `serve`, a log line with a code.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::app::{self, Settings, StartupError};
use crate::config::{Config, ConfigError};
use crate::shutdown;
use crate::telemetry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    Healthcheck,
    Invalid,
}

/// Parses the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Command {
    let args: Vec<OsString> = args.into_iter().collect();
    match args.as_slice() {
        [] => Command::Serve,
        [one] if one == "serve" => Command::Serve,
        [one] if one == "healthcheck" => Command::Healthcheck,
        _ => Command::Invalid,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("invalid command line")]
    Invalid,
    #[error("invalid configuration")]
    Config(#[from] ConfigError),
    #[error("logging could not start")]
    Telemetry,
    #[error("the async runtime could not start")]
    Runtime,
    #[error("startup failed")]
    Startup(#[from] StartupError),
    #[error("the health check failed")]
    Healthcheck,
}

/// Exit code of each failure: 2 for a bad command line, 1 for everything else.
pub fn exit_code(error: &CliError) -> ExitCode {
    match error {
        CliError::Invalid => ExitCode::from(2),
        _ => ExitCode::FAILURE,
    }
}

pub fn run_cli(command: Command) -> Result<(), CliError> {
    match command {
        Command::Serve => run_serve(),
        Command::Healthcheck => {
            crate::healthcheck::run_from_env().map_err(|_| CliError::Healthcheck)
        }
        Command::Invalid => {
            // Logging may not start; the exit code still says what happened.
            if telemetry::init("info").is_ok() {
                tracing::error!(event = "cli.invalid");
            }
            Err(CliError::Invalid)
        }
    }
}

fn run_serve() -> Result<(), CliError> {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            if telemetry::init("info").is_ok() {
                tracing::error!(event = "config.invalid", var = error.var());
            }
            return Err(error.into());
        }
    };
    telemetry::init(&config.rust_log).map_err(|_| CliError::Telemetry)?;
    if let Some(event) = config.echo.startup_event() {
        tracing::warn!(event);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| CliError::Runtime)?;
    runtime.block_on(app::serve(&config, Settings::default(), shutdown::signal()))?;
    Ok(())
}
