//! Subcommands (plan-b decision 2, plan-c decision 10): `server` / `server serve`,
//! `server healthcheck` and `server admin grant|revoke|ban|unban <user_id>`.
//! No error is ever formatted to stdout or stderr: failures become exit codes and, for `serve`
//! and `admin`, a log line with a code (never an argument).

use std::ffi::OsString;
use std::process::ExitCode;

use crate::admin::{self, AdminCommand, AdminConfig};
use crate::app::{self, Settings, StartupError};
use crate::config::{Config, ConfigError};
use crate::shutdown;
use crate::telemetry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve,
    Healthcheck,
    Admin(AdminCommand),
    Invalid,
}

/// Parses the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Command {
    let args: Vec<OsString> = args.into_iter().collect();
    match args.as_slice() {
        [] => Command::Serve,
        [one] if one == "serve" => Command::Serve,
        [one] if one == "healthcheck" => Command::Healthcheck,
        [first, rest @ ..] if first == "admin" => {
            AdminCommand::parse(rest).map_or(Command::Invalid, Command::Admin)
        }
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
    /// The admin command did not apply (a missing user or the database).
    #[error("the admin command failed")]
    Admin,
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
        Command::Admin(command) => run_admin(&command),
        Command::Invalid => {
            // Logging may not start; the exit code still says what happened.
            if telemetry::init("info").is_ok() {
                tracing::error!(event = "cli.invalid");
            }
            Err(CliError::Invalid)
        }
    }
}

/// Only `DATABASE_URL` and `RUST_LOG`: an error in the config of `serve` never stops a ban.
fn run_admin(command: &AdminCommand) -> Result<(), CliError> {
    let config = match AdminConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            if telemetry::init("info").is_ok() {
                tracing::error!(event = "config.invalid", var = error.var());
            }
            return Err(error.into());
        }
    };
    telemetry::init(&config.rust_log).map_err(|_| CliError::Telemetry)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CliError::Runtime)?;
    runtime
        .block_on(admin::run(&config.database, command))
        .map_err(|_| CliError::Admin)
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::admin::{AdminAction, AdminCommand};

    const USER: &str = "0b3f5a6e-1c2d-4e5f-8a9b-0c1d2e3f4a5b";

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn admin(list: &[&str]) -> AdminCommand {
        match parse(args(list)) {
            Command::Admin(command) => command,
            other => panic!("{list:?} gave {other:?}"),
        }
    }

    #[test]
    fn serve_and_healthcheck_are_unchanged() {
        assert_eq!(parse(args(&[])), Command::Serve);
        assert_eq!(parse(args(&["serve"])), Command::Serve);
        assert_eq!(parse(args(&["healthcheck"])), Command::Healthcheck);
        assert_eq!(parse(args(&["--warm-up"])), Command::Invalid);
    }

    #[test]
    fn the_four_admin_forms() {
        let user = Uuid::parse_str(USER).unwrap();
        for (word, action) in [
            ("grant", AdminAction::Grant),
            ("revoke", AdminAction::Revoke),
            ("unban", AdminAction::Unban),
        ] {
            let command = admin(&["admin", word, USER]);
            assert_eq!(command.action, action);
            assert_eq!(command.user, user);
            assert!(command.reason.is_none());
        }
        let ban = admin(&["admin", "ban", USER, "--reason", "  spam in chat  "]);
        assert_eq!(ban.action, AdminAction::Ban);
        assert_eq!(ban.user, user);
        assert_eq!(ban.reason.unwrap().as_str(), "spam in chat");
        let longest = "é".repeat(500);
        let ban = admin(&["admin", "ban", USER, "--reason", &longest]);
        assert_eq!(ban.reason.unwrap().as_str(), longest);
    }

    #[test]
    fn anything_else_is_invalid() {
        let too_long = "x".repeat(501);
        for list in [
            vec!["admin"],
            vec!["admin", "grant"],
            vec!["admin", "ban", USER],
            vec!["admin", "ban", USER, "--reason"],
            vec!["admin", "ban", USER, "--reason", ""],
            vec!["admin", "ban", USER, "--reason", "   "],
            vec!["admin", "ban", USER, "--reason", too_long.as_str()],
            vec!["admin", "ban", USER, "--reason", "line\nbreak"],
            vec!["admin", "ban", USER, "--reason", "tab\there"],
            vec!["admin", "ban", USER, "--reason", "spam", "extra"],
            vec!["admin", "ban", "--reason", "spam", USER],
            vec!["admin", "ban", USER, "--motivo", "spam"],
            vec!["admin", "grant", USER, "--reason", "spam"],
            vec!["admin", "unban", USER, "--reason", "spam"],
            vec!["admin", "grant", USER, "extra"],
            vec!["admin", "grant", "not-a-uuid"],
            vec!["admin", "grant", ""],
            vec!["admin", "promote", USER],
            vec!["admin", "GRANT", USER],
            vec!["serve", "admin"],
        ] {
            assert_eq!(parse(args(&list)), Command::Invalid, "{list:?}");
        }
    }

    /// A8: the `Debug` of a command shows neither the user nor the reason.
    #[test]
    fn debug_never_shows_the_user_nor_the_reason() {
        let ban = admin(&["admin", "ban", USER, "--reason", "canary reason"]);
        for shown in [
            format!("{ban:?}"),
            format!("{:?}", Command::Admin(ban.clone())),
            format!("{:?}", ban.reason),
        ] {
            assert!(!shown.contains(USER), "{shown}");
            assert!(!shown.contains(&USER.replace('-', "")), "{shown}");
            assert!(!shown.contains("canary"), "{shown}");
        }
        assert!(format!("{ban:?}").contains("Ban"));
    }
}
