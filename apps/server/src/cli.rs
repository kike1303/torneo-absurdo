//! Subcommands (plan-b decision 2, plan-c decision 10): `server` / `server serve`,
//! `server healthcheck`, `server admin grant|revoke|ban|unban <user_id>` and `server admin
//! [<action>] --help` (009 RF17, the only text that is not a log or a code), and `server fight void`
//! (012 RF16).
//! No error is ever formatted to stdout or stderr: failures become exit codes and, for `serve`
//! and `admin`, a log line with a code (never an argument).

use std::ffi::OsString;
use std::io::Write as _;
use std::process::ExitCode;

use std::sync::Arc;

use crate::admin::{self, AdminCommand, AdminConfig, VoidCommand};
use crate::app::{self, Settings, StartupError};
use crate::bets::BetsHook;
use crate::config::{Config, ConfigError};
use crate::fights::sealed::RealCompute;
use crate::shutdown;
use crate::telemetry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve,
    Healthcheck,
    Admin(AdminCommand),
    /// `admin --help` and `admin <action> --help`: no config and no connection.
    AdminHelp,
    FightVoid(VoidCommand),
    Invalid,
}

/// Parses the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Command {
    let args: Vec<OsString> = args.into_iter().collect();
    match args.as_slice() {
        [] => Command::Serve,
        [one] if one == "serve" => Command::Serve,
        [one] if one == "healthcheck" => Command::Healthcheck,
        [first, rest @ ..] if first == "admin" && admin::is_help(rest) => Command::AdminHelp,
        [first, rest @ ..] if first == "admin" => {
            AdminCommand::parse(rest).map_or(Command::Invalid, Command::Admin)
        }
        [first, second, rest @ ..] if first == "fight" && second == "void" => {
            VoidCommand::parse(rest).map_or(Command::Invalid, Command::FightVoid)
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
    /// `fight void` refused, or failed before deciding (012 RF16).
    #[error("the fight was not voided")]
    FightVoid,
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
        Command::FightVoid(command) => run_fight_void(&command),
        Command::AdminHelp => {
            // A closed stdout changes nothing: the help has no effect to report.
            let _ = std::io::stdout().lock().write_all(admin::HELP.as_bytes());
            Ok(())
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

/// Only `DATABASE_URL` and `RUST_LOG`: an error in the config of `serve` never stops a ban.
fn admin_runtime() -> Result<(AdminConfig, tokio::runtime::Runtime), CliError> {
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
    Ok((config, runtime))
}

fn run_admin(command: &AdminCommand) -> Result<(), CliError> {
    let (config, runtime) = admin_runtime()?;
    runtime
        .block_on(admin::run(&config.database, command))
        .map_err(|_| CliError::Admin)
}

/// The real engine and the real hook, always: the only caller of `run_fight_void`.
fn run_fight_void(command: &VoidCommand) -> Result<(), CliError> {
    let (config, runtime) = admin_runtime()?;
    runtime
        .block_on(admin::run_fight_void(
            &config.database,
            command,
            Arc::new(RealCompute),
            &BetsHook,
        ))
        .map_err(|_| CliError::FightVoid)
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

    /// 009 RF17: the help of the CLI, as `admin --help` and `admin <action> --help`.
    #[test]
    fn the_help_forms() {
        for list in [
            vec!["admin", "--help"],
            vec!["admin", "grant", "--help"],
            vec!["admin", "revoke", "--help"],
            vec!["admin", "ban", "--help"],
            vec!["admin", "unban", "--help"],
        ] {
            assert_eq!(parse(args(&list)), Command::AdminHelp, "{list:?}");
        }
        for list in [
            vec!["--help"],
            vec!["serve", "--help"],
            vec!["admin", "--help", "extra"],
            vec!["admin", "grant", "--help", "extra"],
            vec!["admin", "promote", "--help"],
            vec!["admin", "grant", USER, "--help"],
            vec!["admin", "-h"],
        ] {
            assert_eq!(parse(args(&list)), Command::Invalid, "{list:?}");
        }
    }

    fn void(list: &[&str]) -> VoidCommand {
        match parse(args(list)) {
            Command::FightVoid(command) => command,
            other => panic!("{list:?} gave {other:?}"),
        }
    }

    /// 012 RF16: the flags in any order, and the tags of RF24.
    #[test]
    fn the_forms_of_fight_void() {
        let user = Uuid::parse_str(USER).unwrap();
        let plain = void(&["fight", "void", "12", "--by", USER]);
        assert_eq!(plain.fight, 12);
        assert_eq!(plain.by, user);
        assert_eq!(plain.previous_binary, None);
        for tag in ["v1.2.3", "v0.10.0", "sha-abc1234"] {
            for list in [
                vec![
                    "fight",
                    "void",
                    "12",
                    "--by",
                    USER,
                    "--previous-binary",
                    tag,
                ],
                vec![
                    "fight",
                    "void",
                    "12",
                    "--previous-binary",
                    tag,
                    "--by",
                    USER,
                ],
            ] {
                let command = void(&list);
                assert_eq!(command.previous_binary.unwrap().as_str(), tag);
                assert_eq!(command.by, user);
            }
        }
        assert!(!plain.backup_checked);
        let backup = void(&["fight", "void", "12", "--backup-checked", "--by", USER]);
        assert!(backup.backup_checked);
        assert_eq!(
            void(&["fight", "void", "2147483647", "--by", USER]).fight,
            i32::MAX
        );
    }

    #[test]
    fn anything_else_about_fight_void_is_invalid() {
        let urn = format!("urn:uuid:{USER}");
        let upper = USER.to_uppercase();
        let simple = USER.replace('-', "");
        let braced = format!("{{{USER}}}");
        for list in [
            vec!["fight"],
            vec!["fight", "void"],
            vec!["fight", "void", "1"],
            vec!["fight", "void", "--by", USER],
            vec!["fight", "void", "1", "--by", &urn],
            vec!["fight", "void", "1", "--by", &upper],
            vec!["fight", "void", "1", "--by", &simple],
            vec!["fight", "void", "1", "--by", &braced],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--previous-binary",
                "v01.002.3",
            ],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--previous-binary",
                "v1.2.03",
            ],
            vec!["fight", "void", "1", "--by"],
            vec!["fight", "void", "1", "--by", "not-a-uuid"],
            vec!["fight", "void", "0", "--by", USER],
            vec!["fight", "void", "01", "--by", USER],
            vec!["fight", "void", "+1", "--by", USER],
            vec!["fight", "void", "-1", "--by", USER],
            vec!["fight", "void", "2147483648", "--by", USER],
            vec!["fight", "void", "1", "--by", USER, "--by", USER],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--backup-checked",
                "--backup-checked",
            ],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--backup-checked",
                "yes",
            ],
            vec!["fight", "void", "1", "--by", USER, "--reason", "x"],
            vec!["fight", "void", "1", "--by", USER, "extra"],
            vec!["fight", "void", "1", "--by", USER, "--previous-binary"],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--previous-binary",
                "dev",
            ],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--previous-binary",
                "1.2.3",
            ],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--previous-binary",
                "sha-ABC1234",
            ],
            vec![
                "fight",
                "void",
                "1",
                "--by",
                USER,
                "--previous-binary",
                "sha-abc123",
            ],
            vec!["fight", "cancel", "1", "--by", USER],
            vec!["fight", "seed", "1", "--by", USER],
            vec!["fight", "VOID", "1", "--by", USER],
        ] {
            assert_eq!(parse(args(&list)), Command::Invalid, "{list:?}");
        }
    }

    /// ADR-0012 A8: its `Debug` shows neither the fight nor the admin.
    #[test]
    fn the_debug_of_fight_void_shows_nothing_of_it() {
        let command = void(&[
            "fight",
            "void",
            "987654",
            "--by",
            USER,
            "--previous-binary",
            "v9.8.7",
        ]);
        for shown in [
            format!("{command:?}"),
            format!("{:?}", Command::FightVoid(command)),
        ] {
            assert!(!shown.contains(USER), "{shown}");
            assert!(!shown.contains(&USER.replace('-', "")), "{shown}");
            assert!(!shown.contains("987654"), "{shown}");
            assert!(!shown.contains("v9.8.7"), "{shown}");
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
