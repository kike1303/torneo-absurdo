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

const NAMES: [&str; 5] = [
    "simulate",
    "canonical_json",
    "FightLog",
    "Seed",
    "fight_secrets",
];

/// The first form that breaks RF24 in this text, outside `fights/sealed.rs`.
fn violation(text: &str) -> Option<String> {
    let tokens = tokens(text);
    let at = |i: usize| tokens.get(i).map(String::as_str);
    for (i, token) in tokens.iter().enumerate() {
        if NAMES.contains(&token.as_str()) {
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
