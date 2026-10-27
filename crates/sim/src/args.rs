//! Command line of `sim` (003 RF15), parsed by hand: `clap` is not in the lock (plan-b
//! decision 6).
use std::ffi::OsString;
use std::path::PathBuf;

use crate::DEFAULT_SEED;

/// `--fights` when absent (RF15).
pub const DEFAULT_FIGHTS: u32 = 10_000;
/// Largest `--fights` accepted (plan-b decision 7).
pub const MAX_FIGHTS: u32 = 10_000_000;
/// `--content` when absent, relative to the working directory (`just sim` runs at the root).
pub const DEFAULT_CONTENT: &str = "content/characters";

pub const HELP: &str = "\
Mass fight simulation and balance report (feature 003, RF15 and RF17).

Usage: just sim [OPTIONS]
       cargo run -p sim --release -- [OPTIONS]

Options:
  --fights N      cross fights to run (default 10000, at most 10000000); each pair of distinct
                  characters gets floor(N / (2 * pairs)) seeds, each played in both orientations,
                  plus as many mirror fights per character, which only measure the side-a bias
  --seed HEX      root seed of the simulator, 64 hex digits (default: a fixed constant)
  --content DIR   roster directory (default content/characters)
  --json          print the report as JSON instead of text
  --check         exit with 1 if a balance criterion of RF17 fails
  --help          print this help

The root seed only derives the seeds of simulated fights: never pass the seed of a real fight.
Exit codes: 0 ok, 1 a criterion failed with --check, 2 usage, content or simulation error.
";

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(Args),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub fights: u32,
    pub seed: [u8; 32],
    pub content: PathBuf,
    pub json: bool,
    pub check: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            fights: DEFAULT_FIGHTS,
            seed: DEFAULT_SEED,
            content: PathBuf::from(DEFAULT_CONTENT),
            json: false,
            check: false,
        }
    }
}

/// A command line that cannot be run. Carries the flag (a developer typed it), never a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("usage.unknown_flag: {0}")]
    UnknownFlag(String),
    #[error("usage.not_utf8: an argument is not UTF-8")]
    NotUtf8,
    #[error("usage.missing_value: {0} needs a value")]
    MissingValue(&'static str),
    #[error("usage.repeated_flag: {0}")]
    RepeatedFlag(&'static str),
    #[error("usage.invalid_fights: --fights must be an integer from 1 to {MAX_FIGHTS}")]
    InvalidFights,
    #[error("usage.invalid_seed: --seed must be exactly 64 hex digits")]
    InvalidSeed,
}

impl ArgsError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownFlag(_) => "usage.unknown_flag",
            Self::NotUtf8 => "usage.not_utf8",
            Self::MissingValue(_) => "usage.missing_value",
            Self::RepeatedFlag(_) => "usage.repeated_flag",
            Self::InvalidFights => "usage.invalid_fights",
            Self::InvalidSeed => "usage.invalid_seed",
        }
    }
}

/// Parses the arguments after the program name. `--help` anywhere wins.
///
/// # Errors
/// The first argument that is unknown, repeated, not UTF-8 or has an invalid value.
pub fn parse<I>(args: I) -> Result<Command, ArgsError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mut out = Args::default();
    let mut seen: Vec<&'static str> = Vec::new();
    let mut help = false;
    while let Some(arg) = args.next() {
        let arg = arg.into_string().map_err(|_| ArgsError::NotUtf8)?;
        let flag: &'static str = match arg.as_str() {
            "--help" | "-h" => {
                help = true;
                continue;
            }
            "--fights" => "--fights",
            "--seed" => "--seed",
            "--content" => "--content",
            "--json" => "--json",
            "--check" => "--check",
            _ => return Err(ArgsError::UnknownFlag(arg)),
        };
        if seen.contains(&flag) {
            return Err(ArgsError::RepeatedFlag(flag));
        }
        seen.push(flag);
        let mut value = || {
            args.next()
                .ok_or(ArgsError::MissingValue(flag))?
                .into_string()
                .map_err(|_| ArgsError::NotUtf8)
        };
        match flag {
            "--fights" => out.fights = parse_fights(&value()?)?,
            "--seed" => out.seed = parse_seed_hex(&value()?).ok_or(ArgsError::InvalidSeed)?,
            "--content" => out.content = PathBuf::from(value()?),
            "--json" => out.json = true,
            _ => out.check = true,
        }
    }
    Ok(if help {
        Command::Help
    } else {
        Command::Run(out)
    })
}

fn parse_fights(value: &str) -> Result<u32, ArgsError> {
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return Err(ArgsError::InvalidFights);
    }
    match value.parse::<u32>() {
        Ok(n) if (1..=MAX_FIGHTS).contains(&n) => Ok(n),
        _ => Err(ArgsError::InvalidFights),
    }
}

/// 64 hex digits (either case) to 32 bytes.
#[must_use]
pub fn parse_seed_hex(value: &str) -> Option<[u8; 32]> {
    let digits = value.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let mut seed = [0_u8; 32];
    for (byte, [high, low]) in seed.iter_mut().zip(digits.as_chunks::<2>().0) {
        *byte = (hex_digit(*high)? << 4) | hex_digit(*low)?;
    }
    Some(seed)
}

/// Lowercase hex of the seed, as the report prints it.
#[must_use]
pub fn seed_hex(seed: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in seed {
        for nibble in [byte >> 4, byte & 0x0f] {
            if let Some(&c) = DIGITS.get(usize::from(nibble)) {
                out.push(char::from(c));
            }
        }
    }
    out
}

const fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Command, ArgsError> {
        parse(args.iter().map(OsString::from))
    }

    fn run_ok(args: &[&str]) -> Args {
        match run(args) {
            Ok(Command::Run(args)) => args,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn defaults() {
        assert_eq!(run_ok(&[]), Args::default());
        let d = Args::default();
        assert_eq!(d.fights, 10_000);
        assert_eq!(d.seed, *b"torneo-absurdo-sim-default-v1!!!");
        assert_eq!(d.content, PathBuf::from("content/characters"));
        assert!(!d.json && !d.check);
    }

    #[test]
    fn every_flag() {
        let hex = "00".repeat(31) + "ff";
        let args = run_ok(&[
            "--fights",
            "400",
            "--seed",
            &hex,
            "--content",
            "x/y",
            "--json",
            "--check",
        ]);
        let mut seed = [0; 32];
        seed[31] = 0xff;
        assert_eq!(
            args,
            Args {
                fights: 400,
                seed,
                content: PathBuf::from("x/y"),
                json: true,
                check: true,
            }
        );
        assert_eq!(run(&["--check", "--help"]), Ok(Command::Help));
        assert_eq!(run(&["-h"]), Ok(Command::Help));
    }

    #[test]
    fn invalid_command_lines() {
        assert_eq!(
            run(&["--fight", "3"]),
            Err(ArgsError::UnknownFlag("--fight".to_owned()))
        );
        assert_eq!(run(&["400"]), Err(ArgsError::UnknownFlag("400".to_owned())));
        assert_eq!(run(&["--fights"]), Err(ArgsError::MissingValue("--fights")));
        assert_eq!(
            run(&["--json", "--json"]),
            Err(ArgsError::RepeatedFlag("--json"))
        );
        for bad in ["abc", "0", "-1", "+5", "", "1e3", "10000001", "99999999999"] {
            assert_eq!(
                run(&["--fights", bad]),
                Err(ArgsError::InvalidFights),
                "{bad}"
            );
        }
        assert_eq!(run_ok(&["--fights", "10000000"]).fights, MAX_FIGHTS);
        assert_eq!(run_ok(&["--fights", "1"]).fights, 1);
        assert_eq!(run(&["--seed", "zz"]), Err(ArgsError::InvalidSeed));
    }

    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![b'-', b'-', 0xff]);
        assert_eq!(parse([bad]), Err(ArgsError::NotUtf8));
    }

    #[test]
    fn codes_lead_the_display() {
        let all = [
            ArgsError::UnknownFlag("--x".to_owned()),
            ArgsError::NotUtf8,
            ArgsError::MissingValue("--seed"),
            ArgsError::RepeatedFlag("--seed"),
            ArgsError::InvalidFights,
            ArgsError::InvalidSeed,
        ];
        for error in all {
            assert!(error.to_string().starts_with(error.code()), "{error}");
        }
    }

    #[test]
    fn seed_hex_both_cases_and_exact_length() {
        let lower = "0123456789abcdef".repeat(4);
        let upper = lower.to_uppercase();
        let expected: [u8; 32] =
            std::array::from_fn(|i| [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef][i % 8]);
        assert_eq!(parse_seed_hex(&lower), Some(expected));
        assert_eq!(parse_seed_hex(&upper), Some(expected));
        assert_eq!(seed_hex(&expected), lower);
        assert_eq!(parse_seed_hex(&lower[..63]), None);
        assert_eq!(parse_seed_hex(&format!("{lower}0")), None);
        assert_eq!(parse_seed_hex(&format!("{}g", &lower[..63])), None);
        assert_eq!(parse_seed_hex(&format!("{} ", &lower[..63])), None);
        assert_eq!(parse_seed_hex(&"é".repeat(32)), None);
    }

    #[test]
    fn the_default_seed_round_trips_through_hex() {
        let hex = seed_hex(&DEFAULT_SEED);
        assert_eq!(hex.len(), 64);
        assert_eq!(parse_seed_hex(&hex), Some(DEFAULT_SEED));
    }

    #[test]
    fn the_help_says_real_seeds_never_go_in() {
        assert!(HELP.contains("never pass the seed of a real fight"));
        for flag in [
            "--fights",
            "--seed",
            "--content",
            "--json",
            "--check",
            "--help",
        ] {
            assert!(HELP.contains(flag), "{flag}");
        }
    }
}
