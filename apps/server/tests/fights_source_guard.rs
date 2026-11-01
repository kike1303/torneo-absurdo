//! 010 RF5, RF16, RF17, RF24 and RF59: what a test can read in the sources of the server. The search
//! works on tokens, not on substrings or paths: `engine::simulate` and a `simulate` brought in by a
//! rename are the same to it, and a local `outcome` is not `.outcome`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Removes the comments, nested ones too, and keeps the text of the strings: SQL lives there. It does
/// not get lost with `"http://"`, a `'"'`, a lifetime or a raw string.
fn strip_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        if rest.starts_with("//") {
            let end = rest.find('\n').unwrap_or(rest.len());
            i += end;
        } else if rest.starts_with("/*") {
            let mut depth = 0;
            let mut j = i;
            while j < bytes.len() {
                if text[j..].starts_with("/*") {
                    depth += 1;
                    j += 2;
                } else if text[j..].starts_with("*/") {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += text[j..].chars().next().unwrap().len_utf8();
                }
            }
            out.push(' ');
            i = j;
        } else if let Some(len) = raw_string_len(rest) {
            out.push_str(&rest[..len]);
            i += len;
        } else if rest.starts_with('"') {
            let len = string_len(rest);
            out.push_str(&rest[..len]);
            i += len;
        } else if let Some(len) = char_literal_len(rest) {
            // A literal like `'"'` must not open a string. Its text is not code.
            out.push(' ');
            i += len;
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// The length of `r"…"`, `r#"…"#`, `br"…"`… at the start of `rest`, not after an identifier.
fn raw_string_len(rest: &str) -> Option<usize> {
    let after_b = rest.strip_prefix('b').unwrap_or(rest);
    let after_r = after_b.strip_prefix('r')?;
    let hashes = after_r.len() - after_r.trim_start_matches('#').len();
    let after_hashes = &after_r[hashes..];
    let body = after_hashes.strip_prefix('"')?;
    let close = format!("\"{}", "#".repeat(hashes));
    let end = body.find(&close)?;
    Some(rest.len() - body.len() + end + close.len())
}

fn string_len(rest: &str) -> usize {
    let mut chars = rest.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '"' => return i + 1,
            _ => {}
        }
    }
    rest.len()
}

/// The length of a char literal at the start of `rest`; `None` for a lifetime.
fn char_literal_len(rest: &str) -> Option<usize> {
    let mut chars = rest.char_indices();
    if chars.next()?.1 != '\'' {
        return None;
    }
    let (_, first) = chars.next()?;
    if first == '\\' {
        let close = rest[2..].find('\'')?;
        // `\'` itself.
        let close = if close == 1 && rest[2..].starts_with('\'') {
            1 + rest[3..].find('\'')?
        } else {
            close
        };
        return Some(2 + close + 1);
    }
    let (at, next) = chars.next()?;
    (next == '\'').then_some(at + 1)
}

/// Identifiers, without `r#` (R1), and the punctuation that matters.
fn tokens(text: &str) -> Vec<String> {
    let code = strip_comments(text);
    let mut out = Vec::new();
    let mut chars = code.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_alphabetic() || c == '_' {
            let mut end = i + c.len_utf8();
            while let Some((j, next)) = chars.peek() {
                if next.is_alphanumeric() || *next == '_' {
                    end = j + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &code[i..end];
            // `r#simulate` is `simulate`. A raw string is not an identifier: its `r` stays alone.
            if word == "r" && chars.peek().is_some_and(|(_, next)| *next == '#') {
                let mut lookahead = chars.clone();
                lookahead.next();
                if lookahead
                    .peek()
                    .is_some_and(|(_, next)| next.is_alphabetic() || *next == '_')
                {
                    chars.next();
                    continue;
                }
            }
            out.push(word.to_owned());
        } else if c == ':' && chars.peek().is_some_and(|(_, next)| *next == ':') {
            chars.next();
            out.push("::".to_owned());
        } else if matches!(c, '.' | '*') {
            out.push(c.to_string());
        } else if !c.is_whitespace() {
            out.push(" ".to_owned());
        }
    }
    out
}

const NAMES: [&str; 4] = ["simulate", "canonical_json", "FightLog", "Seed"];

/// The first form that breaks RF24 in this text, outside `fights/sealed.rs`.
fn violation(text: &str) -> Option<String> {
    let tokens = tokens(text);
    let at = |i: usize| tokens.get(i).map(String::as_str);
    for (i, token) in tokens.iter().enumerate() {
        if NAMES.contains(&token.as_str()) {
            return Some(token.clone());
        }
        // SQL does not tell `FIGHT_SECRETS` from `fight_secrets`.
        if token.eq_ignore_ascii_case("fight_secrets") {
            return Some(token.clone());
        }
        // `.outcome` and `::outcome`, whatever is in between. A local `outcome` is not one.
        if token == "outcome" && i > 0 && matches!(at(i - 1), Some("." | "::")) {
            return Some("qualified outcome".to_owned());
        }
        // `use engine as e;` and `extern crate engine as e;`, the same with `protocol`.
        if (token == "engine" || token == "protocol") && at(i + 1) == Some("as") {
            return Some(format!("{token} renamed"));
        }
        // `engine::*` and `protocol::*`, not `base64::engine::general_purpose`.
        if (token == "engine" || token == "protocol")
            && at(i + 1) == Some("::")
            && at(i + 2) == Some("*")
        {
            return Some(format!("{token} glob"));
        }
    }
    None
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(&src(), &mut files);
    assert!(files.len() > 30, "{files:?}");
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

#[test]
fn the_search_catches_every_form() {
    for text in [
        "use engine::{simulate, Seed};",
        "use engine as e;",
        "extern crate engine as e;",
        "use protocol as p;",
        "use engine::*;",
        "use protocol::*;",
        "e::simulate(a, b, c, d)",
        "let log = run(); let (w, r) = log.outcome().unwrap();",
        "log\n    .outcome()",
        "log . outcome ()",
        "FightLog::outcome(&log)",
        "<_>::outcome(&log)",
        "protocol::FightLog",
        "canonical_json(&log)",
        "sqlx::query(\"DELETE FROM fight_secrets\")",
        "sqlx::query(\"DELETE FROM FIGHT_SECRETS\")",
        "sqlx::query(\"delete from Fight_Secrets\")",
        "let s = \"SELECT seed FROM\\n fight_secrets\";",
        // R1: a raw identifier is the same identifier.
        "log.r#outcome()",
        "r#simulate(a, b, c, d)",
        "use engine::r#simulate;",
        // R1: what could make the search lose its place, and hide what follows.
        "let u = \"http://x\"; simulate();",
        "let c = '\"'; simulate();",
        "let c = '\\''; simulate();",
        "let r = r#\"a \" b\"#; simulate();",
        "let r = r##\"a \"# b\"##; simulate();",
        "/* a /* nested */ b */ simulate();",
        "fn f<'a>(x: &'a str) { simulate(); }",
    ] {
        assert!(violation(text).is_some(), "{text}");
    }
}

#[test]
fn the_search_lets_through_what_is_not_a_form() {
    for text in [
        "let outcome = 1;",
        "match outcome { _ => {} }",
        "outcome = async { 1 };",
        "use base64::engine::general_purpose::URL_SAFE_NO_PAD;",
        "use engine::{CharacterSpec, Ruleset};",
        "use protocol::{Side, FightEvent};",
        "struct SealedSeed;",
        "// simulate, Seed, FightLog and fight_secrets in a comment\nlet x = 1;",
        "/* simulate /* Seed */ canonical_json */ let x = 1;",
        "let a = seed_commit; let b = release_due;",
    ] {
        assert_eq!(violation(text), None, "{text}");
    }
}

/// RF24: outside `fights/sealed.rs` no name of the log nor of the seed, no rename and no glob.
#[test]
fn only_the_sealed_module_touches_the_seed_and_the_log() {
    let sealed = src().join("fights").join("sealed.rs");
    for (path, text) in sources() {
        if path != sealed {
            assert_eq!(violation(&text), None, "{}", path.display());
        }
    }
}

/// RF24: outside `fights/` nobody names the column of the reveal.
#[test]
fn the_reveal_stays_in_the_fights_module() {
    let fights = src().join("fights");
    for (path, text) in sources() {
        if !path.starts_with(&fights) {
            let found = tokens(&text).iter().any(|t| t == "revealed_seed");
            assert!(!found, "{}", path.display());
        }
    }
}

/// The file relative to `src`, with `/`.
fn relative(path: &Path) -> String {
    path.strip_prefix(src())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn has_token(text: &str, name: &str) -> bool {
    tokens(text).iter().any(|token| token == name)
}

/// Outside `allowed` nobody names `name`.
fn only_in(name: &str, allowed: &[&str]) {
    for (path, text) in sources() {
        let file = relative(&path);
        if !allowed.contains(&file.as_str()) {
            assert!(!has_token(&text, name), "{name} in {file}");
        }
    }
}

/// The code without comments or spaces: `include !` and `# [path` are the same to the compiler.
fn compact(text: &str) -> String {
    strip_comments(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// `include!` and `#[path` bring in code that this test does not read as what it is. The exception
/// is the exact line of `profile/name.rs`, its tests in their own file under `cfg(test)`.
fn brings_in_code(file: &str, text: &str) -> bool {
    let code = compact(text);
    let code = if file == "profile/name.rs" {
        code.replacen("#[cfg(test)]#[path=\"name_tests.rs\"]modtests;", "", 1)
    } else {
        code
    };
    code.contains("include!") || code.contains("#[path")
}

#[test]
fn the_search_for_code_brought_in_catches_every_form() {
    for text in [
        "include!(\"x.rs\");",
        "include ! (\"x.rs\");",
        "#[path = \"x.rs\"] mod x;",
        "# [ path = \"x.rs\" ] mod x;",
        "#[path = \"name_tests.rs\"]\nmod tests;",
    ] {
        assert!(brings_in_code("fights/hub.rs", text), "{text}");
        assert!(brings_in_code("profile/name.rs", text), "{text}");
    }
    for text in [
        "// include!(\"x.rs\");\nlet x = 1;",
        "/* #[path = \"x.rs\"] */ let x = 1;",
        "let a = include_str!(\"x.txt\");",
    ] {
        assert!(!brings_in_code("fights/hub.rs", text), "{text}");
    }
    // A second one in the file of the exception is not the exception.
    let two = "#[cfg(test)]\n#[path = \"name_tests.rs\"]\nmod tests;\n#[path = \"x.rs\"] mod x;";
    assert!(brings_in_code("profile/name.rs", two));
    assert!(!brings_in_code(
        "profile/name.rs",
        "#[cfg(test)]\n#[path = \"name_tests.rs\"]\nmod tests;"
    ));
    assert!(brings_in_code(
        "fights/hub.rs",
        "#[cfg(test)]\n#[path = \"name_tests.rs\"]\nmod tests;"
    ));
}

/// RF24: nothing in `src` brings in code around this test.
#[test]
fn nothing_brings_in_code_around_these_searches() {
    for (path, text) in sources() {
        let file = relative(&path);
        assert!(!brings_in_code(&file, &text), "{file}");
    }
}

#[test]
fn the_search_for_a_name_catches_every_form() {
    for (text, name) in [
        ("log.release_due(u32::MAX);", "release_due"),
        ("log\n    .release_due(0)", "release_due"),
        ("log . r#release_due (0)", "release_due"),
        ("SealedLog::release_due(&mut log, 0)", "release_due"),
        ("log.into_final(0)", "into_final"),
        ("clock.stopwatch(i64::MIN)", "stopwatch"),
        ("sealed::events_of_settled(bytes)", "events_of_settled"),
        (
            "sealed::open_window_with_seed(tx, 1, &s, 0)",
            "open_window_with_seed",
        ),
    ] {
        assert!(has_token(text, name), "{text}");
    }
    assert!(!has_token("// log.release_due(0)", "release_due"));
    assert!(!has_token("let stopwatch_of = 1;", "stopwatch"));
}

/// RF24 (`release_due(u32::MAX)`, the known limit): against mistakes, only the release asks for
/// events, and only with the time of its stopwatch. Whoever edits `release.rs` on purpose gets past
/// this, and that is for the review.
#[test]
fn only_the_release_asks_for_events() {
    only_in("release_due", &["fights/sealed.rs", "fights/release.rs"]);
    only_in("stopwatch", &["fights/clock.rs", "fights/release.rs"]);
    only_in("into_final", &["fights/sealed.rs", "fights/release.rs"]);
    for name in ["SealedLog", "FinalLog"] {
        only_in(
            name,
            &["fights/sealed.rs", "fights/release.rs", "fights/runner.rs"],
        );
    }
    only_in(
        "events_of_settled",
        &["fights/sealed.rs", "fights/runner.rs"],
    );

    let release = tokens(&std::fs::read_to_string(src().join("fights/release.rs")).unwrap());
    let calls: Vec<usize> = release
        .iter()
        .enumerate()
        .filter(|(_, token)| *token == "release_due")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(calls.len(), 1, "{calls:?}");
    // `release_due`, the parenthesis and its only argument.
    let call = calls[0];
    assert_eq!(release.get(call + 2).map(String::as_str), Some("elapsed"));
    assert_eq!(release.get(call + 3).map(String::as_str), Some(" "));
    assert!(!release.iter().any(|token| token == "MAX"));
    let text = strip_comments(&std::fs::read_to_string(src().join("fights/release.rs")).unwrap());
    assert!(text.contains("let Some(elapsed) = stopwatch.elapsed_ms() else {"));
}

/// RF38: a socket reads the hub, never what is sealed nor the database of the fights, and the hub
/// only holds what it was given.
#[test]
fn the_hub_and_the_sockets_do_not_reach_what_is_sealed() {
    let hub = std::fs::read_to_string(src().join("fights/hub.rs")).unwrap();
    assert!(!has_token(&hub, "sealed"));
    let ws = src().join("ws");
    let mut checked = 0;
    for (path, text) in sources() {
        if path.starts_with(&ws) {
            checked += 1;
            for name in ["sealed", "runner", "store"] {
                assert!(!has_token(&text, name), "{name} in {}", path.display());
            }
        }
    }
    assert!(checked >= 7, "{checked}");
}

/// RF45: the feature is the only way into a script, and without it none of this compiles.
#[test]
fn the_script_stays_behind_its_feature() {
    only_in(
        "open_window_with_seed",
        &["fights/sealed.rs", "fights/runner.rs"],
    );
    for (path, text) in sources() {
        let file = relative(&path);
        if file != "fights/script.rs" {
            assert!(!text.contains("FIGHT_SCRIPT_PATH"), "{file}");
        }
    }
    let gated = "#[cfg(feature=\"dev-fights\")]";
    for (file, item) in [
        ("fights/sealed.rs", "pubasyncfnopen_window_with_seed("),
        ("fights/store.rs", "pubasyncfnfight_rank("),
        ("fights/mod.rs", "pubmodscript;"),
    ] {
        let code = compact(&std::fs::read_to_string(src().join(file)).unwrap());
        assert_eq!(code.matches(item).count(), 1, "{file}");
        assert!(code.contains(&format!("{gated}{item}")), "{file}: {item}");
    }
}

/// A filter that finds no test passes: the gate of `dev-fights` with that feature alone exists, under
/// its `cfg`, and the recipe that runs it names it.
#[test]
fn the_test_of_the_gate_of_dev_fights_exists_and_runs() {
    let name = "dev_fights_needs_a_loopback_origin_and_bind_addr";
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/startup.rs");
    let code = compact(&std::fs::read_to_string(tests).unwrap());
    let declared = format!(
        "#[cfg(all(feature=\"dev-fights\",not(feature=\"dev-auth\")))]#[tokio::test]asyncfn{name}()"
    );
    assert!(code.contains(&declared), "{declared}");
    let justfile = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../justfile");
    let justfile = std::fs::read_to_string(justfile).unwrap();
    assert!(
        justfile.contains(&format!(
            "cargo test -p server --features dev-fights --locked --test startup {name}"
        )),
        "the justfile does not run {name}"
    );
}

/// RF23: `sealed.rs` cannot write a log line.
#[test]
fn the_sealed_module_has_no_tracing() {
    let text = std::fs::read_to_string(src().join("fights/sealed.rs")).unwrap();
    assert!(!tokens(&text).iter().any(|t| t == "tracing"));
}

/// RF40: neither does the release of the events.
#[test]
fn the_release_has_no_tracing() {
    let text = std::fs::read_to_string(src().join("fights/release.rs")).unwrap();
    assert!(!tokens(&text).iter().any(|t| t == "tracing"));
}

/// RF43 and RF45: neither do the hub and the script.
#[test]
fn the_hub_and_the_script_have_no_tracing() {
    for file in ["fights/hub.rs", "fights/script.rs"] {
        let text = std::fs::read_to_string(src().join(file)).unwrap();
        assert!(!has_token(&text, "tracing"), "{file}");
    }
}

/// RF5: the roster is only read to announce, never to simulate.
#[test]
fn only_the_roster_module_loads_the_roster() {
    let roster = src().join("fights").join("roster.rs");
    for (path, text) in sources() {
        if path != roster {
            let found = tokens(&text).iter().any(|t| t == "load_roster");
            assert!(!found, "{}", path.display());
        }
    }
}

/// RF17: only the store makes the proof that the window closed.
#[test]
fn only_the_store_makes_a_closed_window() {
    let (sealed, store) = (
        src().join("fights/sealed.rs"),
        src().join("fights/store.rs"),
    );
    for (path, text) in sources() {
        if path != sealed && path != store {
            assert!(
                !text.contains("ClosedWindow::confirmed"),
                "{}",
                path.display()
            );
        }
    }
}

/// RF16 (amendment M5): `Action::Cancel` is only built by the decision, and a cancellation is only
/// written by an attempt of the runner, through `sealed::cancel_fight`, the only caller of the store.
#[test]
fn a_cancel_only_comes_from_next_action() {
    let (state, runner, sealed) = (
        src().join("fights/state.rs"),
        src().join("fights/runner.rs"),
        src().join("fights/sealed.rs"),
    );
    for (path, text) in sources() {
        let code = strip_comments(&text);
        if path != state && path != runner {
            assert!(!code.contains("Action::Cancel"), "{}", path.display());
        }
        if path != sealed {
            assert!(!code.contains("store::cancel"), "{}", path.display());
        }
        if path != runner {
            assert!(!code.contains("sealed::cancel_fight"), "{}", path.display());
        }
    }
    let code = strip_comments(&std::fs::read_to_string(runner).unwrap());
    assert_eq!(code.matches("Action::Cancel(reason) =>").count(), 1);
    assert_eq!(code.matches("Action::Cancel").count(), 1);
    assert_eq!(code.matches("sealed::cancel_fight").count(), 1);
}

/// RF16 and RF59: nothing in the server writes `voided`, and the runner does not even name it.
#[test]
fn nothing_writes_a_voided_final() {
    for (path, text) in sources() {
        let sql: String = strip_comments(&text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        // As SQL writes it: with a parameter, a literal or a column on its right. A unit test of a
        // `FightRow` assigns a Rust field, which reaches no database.
        for form in [
            "void_reason = $",
            "void_reason = '",
            "void_reason = EXCLUDED",
        ] {
            assert!(!sql.contains(form), "{}", path.display());
        }
    }
    let store = strip_comments(&std::fs::read_to_string(src().join("fights/store.rs")).unwrap());
    // Read: once in the list of columns and once in its decoding, and nothing else.
    assert_eq!(store.matches("void_reason").count(), 2, "{store}");
    let runner = std::fs::read_to_string(src().join("fights/runner.rs")).unwrap();
    for token in tokens(&runner) {
        assert_ne!(token.to_lowercase(), "voided");
        assert_ne!(token, "VoidReason");
    }
}

/// RF16: no route cancels, voids or reads a seed, and there is no such command.
#[test]
fn neither_a_route_nor_a_command_cancels_voids_or_reads_a_seed() {
    use server::http::routes::ROUTES;
    for route in ROUTES {
        let path = route.path.to_lowercase();
        for word in ["cancel", "void", "seed", "secret"] {
            assert!(!path.contains(word), "{}", route.path);
        }
    }
    use server::{Command, parse};
    let args = |list: &[&str]| parse(list.iter().map(std::ffi::OsString::from));
    for line in [
        &["fight", "void", "1", "--by", "x"][..],
        &["fight"],
        &["fight", "cancel", "1"],
        &["fight", "seed", "1"],
        &["--help"],
    ] {
        assert_eq!(args(line), Command::Invalid, "{line:?}");
    }
    assert_eq!(args(&["admin", "--help"]), Command::AdminHelp);
    let cli = std::fs::read_to_string(src().join("cli.rs")).unwrap();
    let admin = std::fs::read_to_string(src().join("admin.rs")).unwrap();
    for text in [cli, admin] {
        let text = text.to_lowercase();
        assert!(!text.contains("fight"), "the help mentions fights");
    }
}

/// RF26: `FightLog` is only in its own type: no other type of the protocol contains it.
#[test]
fn no_type_of_the_protocol_contains_the_log() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/protocol/src");
    for file in ["ws.rs", "fights.rs", "error.rs", "me.rs", "health.rs"] {
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        assert!(!tokens(&text).iter().any(|t| t == "FightLog"), "{file}");
    }
}
