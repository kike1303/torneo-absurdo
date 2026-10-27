//! WASM bindings of the fight engine (003 RF19). One exported function, [`simulate`], over
//! the pure [`run`], which the native tests exercise.
//!
//! Untrusted input (JSON from the client): every text argument is rejected above 8 KiB before
//! parsing, specs must be exactly the engine shape (`deny_unknown_fields`), and errors carry
//! only a code, never a message from `serde_json`.
#![deny(
    clippy::float_arithmetic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use engine::{CharacterSpec, Ruleset, Seed, SpecError};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// Largest accepted text argument, in bytes.
pub const MAX_INPUT_BYTES: usize = 8 * 1024;

/// Error of the bindings: a code, never text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmError {
    InputTooLarge,
    SeedInvalidHex,
    SpecInvalidJson,
    RulesetUnknown,
    LogEncodeFailed,
    Spec(SpecError),
}

impl WasmError {
    /// Every code except `spec.<code>`, which comes from `SpecError::ALL`.
    pub const FIXED: [Self; 5] = [
        Self::InputTooLarge,
        Self::SeedInvalidHex,
        Self::SpecInvalidJson,
        Self::RulesetUnknown,
        Self::LogEncodeFailed,
    ];

    /// `domain.reason`, e.g. `input.too_large` or `spec.budget_exceeded`.
    #[must_use]
    pub fn code(self) -> String {
        match self {
            Self::InputTooLarge => "input.too_large".to_owned(),
            Self::SeedInvalidHex => "seed.invalid_hex".to_owned(),
            Self::SpecInvalidJson => "spec.invalid_json".to_owned(),
            Self::RulesetUnknown => "ruleset.unknown".to_owned(),
            Self::LogEncodeFailed => "log.encode_failed".to_owned(),
            Self::Spec(error) => format!("spec.{}", error.code()),
        }
    }
}

const fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn parse_seed(seed_hex: &str) -> Result<Seed, WasmError> {
    let digits = seed_hex.as_bytes();
    let mut bytes = [0_u8; 32];
    if digits.len() != bytes.len() * 2 {
        return Err(WasmError::SeedInvalidHex);
    }
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, [hi, lo]) in bytes.iter_mut().zip(pairs) {
        match (hex_value(*hi), hex_value(*lo)) {
            (Some(hi), Some(lo)) => *byte = (hi << 4) | lo,
            _ => return Err(WasmError::SeedInvalidHex),
        }
    }
    Ok(Seed::from_bytes(bytes))
}

fn parse_spec(json: &str) -> Result<CharacterSpec, WasmError> {
    serde_json::from_str(json).map_err(|_| WasmError::SpecInvalidJson)
}

/// The whole binding in plain Rust: sizes, seed, ruleset, specs, simulation, canonical JSON.
///
/// # Errors
/// The first [`WasmError`] found, in that order.
pub fn run(
    seed_hex: &str,
    a_json: &str,
    b_json: &str,
    ruleset_version: u16,
) -> Result<String, WasmError> {
    if [seed_hex, a_json, b_json]
        .iter()
        .any(|text| text.len() > MAX_INPUT_BYTES)
    {
        return Err(WasmError::InputTooLarge);
    }
    let seed = parse_seed(seed_hex)?;
    let ruleset = Ruleset::for_version(ruleset_version).ok_or(WasmError::RulesetUnknown)?;
    let a = parse_spec(a_json)?;
    let b = parse_spec(b_json)?;
    let log = engine::simulate(&seed, &a, &b, &ruleset).map_err(WasmError::Spec)?;
    let bytes = engine::canonical_json(&log).map_err(|_| WasmError::LogEncodeFailed)?;
    String::from_utf8(bytes).map_err(|_| WasmError::LogEncodeFailed)
}

/// Simulates a fight and returns its canonical JSON; on error, a `JsError` whose message is
/// only the code (RF19).
///
/// # Errors
/// See [`run`].
#[wasm_bindgen]
pub fn simulate(
    seed_hex: &str,
    a_json: &str,
    b_json: &str,
    ruleset_version: u16,
) -> Result<String, JsError> {
    run(seed_hex, a_json, b_json, ruleset_version).map_err(|error| JsError::new(&error.code()))
}
