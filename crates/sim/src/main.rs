//! `sim` binary (003 RF15): `just sim [--fights N] [--seed HEX] [--content DIR] [--json] [--check]`.
//!
//! Exit codes (plan-b decision 12): 0 ok, 1 a criterion of RF17 fails with `--check`, 2 a
//! usage, content or simulation error (its code and path on stderr). The elapsed time is
//! measured here, around `sim::run`, and is never part of the deterministic `Report`.
use std::process::ExitCode;
use std::time::Instant;

use engine::{CharacterSpec, Ruleset};
use serde::Serialize;
use sim::args::{self, Command, HELP};
use sim::{Config, Criteria, Failure, RULESET_VERSION, Report, Warning};

const EXIT_FAILED_CHECK: u8 = 1;
const EXIT_ERROR: u8 = 2;

/// `--json`: the report plus what `--check` and the reader need.
#[derive(Serialize)]
struct JsonOutput<'a> {
    report: &'a Report,
    failures: Vec<Failure>,
    warnings: Vec<Warning>,
    elapsed_ms: u128,
}

fn main() -> ExitCode {
    let args = match args::parse(std::env::args_os().skip(1)) {
        Ok(Command::Run(args)) => args,
        Ok(Command::Help) => {
            out(HELP);
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            err(&format!("sim: {error}\n\n{HELP}"));
            return ExitCode::from(EXIT_ERROR);
        }
    };
    let Some(ruleset) = Ruleset::for_version(RULESET_VERSION) else {
        err("sim: sim.unknown_ruleset");
        return ExitCode::from(EXIT_ERROR);
    };
    let roster: Vec<CharacterSpec> = match content::load_roster(&args.content, &ruleset) {
        Ok(roster) => roster
            .into_iter()
            .map(content::Character::into_spec)
            .collect(),
        Err(error) => {
            err(&format!(
                "sim: content.{} in {}: {error}",
                error.code(),
                args.content.display()
            ));
            return ExitCode::from(EXIT_ERROR);
        }
    };
    let config = Config {
        fights: args.fights,
        seed: args.seed,
    };
    let start = Instant::now();
    let report = match sim::run(&config, &roster, &ruleset) {
        Ok(report) => report,
        Err(error) => {
            err(&format!("sim: {error}"));
            return ExitCode::from(EXIT_ERROR);
        }
    };
    let elapsed_ms = start.elapsed().as_millis();
    let criteria = Criteria::RF17;
    let failures = report.failures(&criteria);
    let failed = !failures.is_empty();
    if args.json {
        let output = JsonOutput {
            report: &report,
            failures,
            warnings: report.warnings(&criteria),
            elapsed_ms,
        };
        match serde_json::to_string_pretty(&output) {
            Ok(json) => out(&format!("{json}\n")),
            Err(_) => {
                err("sim: sim.encode_failed");
                return ExitCode::from(EXIT_ERROR);
            }
        }
    } else {
        out(&format!(
            "{}elapsed: {elapsed_ms} ms (simulation and canonical_json)\n",
            report.render_text(&criteria)
        ));
    }
    if args.check && failed {
        ExitCode::from(EXIT_FAILED_CHECK)
    } else {
        ExitCode::SUCCESS
    }
}

#[allow(
    clippy::print_stdout,
    reason = "the report is the output of the CLI (RF15)"
)]
fn out(text: &str) {
    print!("{text}");
}

#[allow(
    clippy::print_stderr,
    reason = "errors of the CLI, with their code (RF15)"
)]
fn err(text: &str) {
    eprintln!("{text}");
}
