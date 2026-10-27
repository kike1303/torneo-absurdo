//! Error codes of the bindings (003 RF19), on the pure `run` (natively: `JsError` needs a JS
//! host).
use engine::{SpecError, canonical_json};
use engine_wasm::{MAX_INPUT_BYTES, WasmError, run};

const SEED: &str = "0123456789abcdef0123456789ABCDEF0123456789abcdef0123456789abcdef";

const SPEC: &str = r#"{"slug":"rata-electrica","stats":{"hp":55,"attack":60,"defense":45,"speed":85,"special":55},"personality":{"aggression":70,"caution":20,"showmanship":90},"abilities":[{"id":"thunder_squeak","kind":"special","power":70,"cooldown":3,"element":"electric"}],"weaknesses":["ground"]}"#;

fn with(from: &str, to: &str) -> String {
    assert!(SPEC.contains(from), "{from}");
    SPEC.replacen(from, to, 1)
}

fn code(result: Result<String, WasmError>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(error) => error.code(),
    }
}

#[test]
fn a_valid_call_returns_the_canonical_log() {
    let json = run(SEED, SPEC, SPEC, 1).unwrap();
    let a: engine::CharacterSpec = serde_json::from_str(SPEC).unwrap();
    let mut seed = [0_u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&SEED[i * 2..i * 2 + 2], 16).unwrap();
    }
    let log = engine::simulate(
        &engine::Seed::from_bytes(seed),
        &a,
        &a,
        &engine::Ruleset::for_version(1).unwrap(),
    )
    .unwrap();
    assert_eq!(json.as_bytes(), canonical_json(&log).unwrap());
}

#[test]
fn inputs_above_8_kib_are_rejected_before_parsing() {
    let huge_invalid = "x".repeat(MAX_INPUT_BYTES + 1);
    assert_eq!(MAX_INPUT_BYTES, 8192);
    assert_eq!(code(run(&huge_invalid, SPEC, SPEC, 1)), "input.too_large");
    assert_eq!(code(run(SEED, &huge_invalid, SPEC, 1)), "input.too_large");
    assert_eq!(code(run(SEED, SPEC, &huge_invalid, 1)), "input.too_large");
    // Even before an unknown ruleset or a bad seed.
    assert_eq!(code(run("zz", &huge_invalid, SPEC, 9)), "input.too_large");
    // A valid spec padded with whitespace up to exactly 8 KiB is accepted.
    let padded = format!("{SPEC:<width$}", width = MAX_INPUT_BYTES);
    assert_eq!(padded.len(), MAX_INPUT_BYTES);
    assert_eq!(code(run(SEED, &padded, SPEC, 1)), "ok");
}

#[test]
fn seed_must_be_64_hex_characters() {
    for bad in [
        &SEED[..63],
        &format!("{SEED}0"),
        &SEED.replacen('0', "g", 1),
        &SEED.replacen('0', " ", 1),
        "",
        &"é".repeat(32),
    ] {
        assert_eq!(code(run(bad, SPEC, SPEC, 1)), "seed.invalid_hex", "{bad:?}");
    }
}

#[test]
fn unknown_ruleset() {
    assert_eq!(code(run(SEED, SPEC, SPEC, 0)), "ruleset.unknown");
    assert_eq!(code(run(SEED, SPEC, SPEC, 2)), "ruleset.unknown");
}

#[test]
fn specs_must_be_exactly_the_engine_shape() {
    let extra_field = with(r#""slug""#, r#""extra":1,"slug""#);
    let file_shape = with(
        r#""slug""#,
        r#""archetype":"electric-rodent","ruleset_version":1,"rivals":[],"slug""#,
    );
    let nested_extra = with(r#""hp":55"#, r#""hp":55,"luck":3"#);
    for bad in [
        extra_field.as_str(),
        file_shape.as_str(),
        nested_extra.as_str(),
        "{",
        "",
        "null",
        &with(r#""hp":55"#, r#""hp":55.5"#),
        &with(r#""hp":55"#, r#""hp":-1"#),
        &with(r#""hp":55"#, r#""hp":300"#),
        &with(r#""electric""#, r#""wood""#),
    ] {
        assert_eq!(code(run(SEED, bad, SPEC, 1)), "spec.invalid_json", "{bad}");
        assert_eq!(code(run(SEED, SPEC, bad, 1)), "spec.invalid_json", "{bad}");
    }
}

#[test]
fn every_spec_error_has_its_code() {
    let cases = [
        (
            SpecError::SlugInvalid,
            with(r#""rata-electrica""#, r#""Rata""#),
        ),
        (SpecError::StatOutOfRange, with(r#""hp":55"#, r#""hp":0"#)),
        (SpecError::BudgetExceeded, with(r#""hp":55"#, r#""hp":100"#)),
        (
            SpecError::PersonalityOutOfRange,
            with(r#""caution":20"#, r#""caution":101"#),
        ),
        (
            SpecError::AbilityCount,
            with(
                r#"[{"id":"thunder_squeak","kind":"special","power":70,"cooldown":3,"element":"electric"}]"#,
                "[]",
            ),
        ),
        (
            SpecError::AbilityIdInvalid,
            with("thunder_squeak", "strike"),
        ),
        (
            SpecError::AbilityIdDuplicate,
            with(
                r#"[{"id":"thunder_squeak","kind":"special","power":70,"cooldown":3,"element":"electric"}]"#,
                r#"[{"id":"zap","kind":"heavy","power":20,"cooldown":0},{"id":"zap","kind":"heavy","power":20,"cooldown":0}]"#,
            ),
        ),
        (
            SpecError::PowerOutOfRange,
            with(r#""power":70"#, r#""power":101"#),
        ),
        (
            SpecError::CooldownOutOfRange,
            with(r#""cooldown":3"#, r#""cooldown":9"#),
        ),
        (
            SpecError::WeaknessCount,
            with(r#"["ground"]"#, r#"["ground","fire","ice"]"#),
        ),
        (
            SpecError::WeaknessDuplicate,
            with(r#"["ground"]"#, r#"["ground","ground"]"#),
        ),
    ];
    for (error, spec) in &cases {
        let expected = format!("spec.{}", error.code());
        assert_eq!(code(run(SEED, spec, SPEC, 1)), expected);
        assert_eq!(code(run(SEED, SPEC, spec, 1)), expected);
    }
    // Every SpecError is produced above except the EventLimit guard, unreachable with v1.
    for error in SpecError::ALL {
        assert!(
            cases.iter().any(|(e, _)| *e == error) || error == SpecError::EventLimit,
            "{error:?}"
        );
    }
}

#[test]
fn every_code_is_a_bare_domain_reason() {
    let codes: Vec<String> = WasmError::FIXED
        .iter()
        .copied()
        .chain(SpecError::ALL.map(WasmError::Spec))
        .map(WasmError::code)
        .collect();
    assert_eq!(codes.len(), 5 + SpecError::ALL.len());
    for (i, code) in codes.iter().enumerate() {
        let (domain, reason) = code.split_once('.').unwrap();
        for part in [domain, reason] {
            assert!(
                !part.is_empty() && part.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'),
                "{code}"
            );
        }
        assert!(!codes.iter().take(i).any(|other| other == code), "{code}");
    }
    for fixed in [
        "input.too_large",
        "seed.invalid_hex",
        "spec.invalid_json",
        "ruleset.unknown",
        "log.encode_failed",
        "spec.event_limit",
    ] {
        assert!(codes.iter().any(|c| c == fixed), "{fixed}");
    }
}

#[test]
fn errors_never_carry_serde_json_text() {
    // serde_json messages look like "unknown field `extra`, expected one of ... at line 1 column 9".
    let inputs = [
        with(r#""slug""#, r#""extra":1,"slug""#),
        "{".to_owned(),
        with(r#""hp":55"#, r#""hp":"x""#),
    ];
    for bad in &inputs {
        let message = code(run(SEED, bad, SPEC, 1));
        assert_eq!(message, "spec.invalid_json");
        for fragment in ["line", "column", "unknown", "expected", "`", " "] {
            assert!(!message.contains(fragment), "{message}");
        }
    }
}
