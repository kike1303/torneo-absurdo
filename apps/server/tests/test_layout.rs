//! The layout rule of the in-process log capture (plan-c deviation 7, nit of #55): a test
//! binary that uses the `logs` module of `support` has exactly one test. `tracing`'s callsite interest cache is
//! per process and the capture's dispatcher is per thread, so a second test in the same binary
//! makes the capture miss events silently, and the canary checks (absence assertions) would then
//! pass in vacuum.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Whether `source` uses the in-process capture. The needles are split so this file does not
/// match itself.
fn uses_log_capture(source: &str) -> bool {
    let module = ["support::", "logs"].concat();
    let call = ["logs::", "run("].concat();
    source.contains(&module) || source.contains(&call)
}

/// The tests a source declares: every attribute whose path ends in `test` (`#[test]`,
/// `#[tokio::test]`, `#[tokio::test(flavor = …)]`, `#[sqlx::test]`), plus one per `proptest` macro
/// block (it expands to at least one test).
fn test_count(source: &str) -> usize {
    let attributes = source
        .lines()
        .map(str::trim_start)
        .filter_map(|line| line.strip_prefix("#["))
        .filter(|inner| {
            let path = inner.split(['(', ']']).next().unwrap_or_default().trim();
            path.rsplit("::").next() == Some("test")
        })
        .count();
    let proptest = ["proptest", "!"].concat();
    attributes + source.matches(&proptest).count()
}

fn test_files() -> Vec<(PathBuf, String)> {
    rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"))
}

fn rust_files(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect();
    files.sort();
    files
}

#[test]
fn every_log_capture_binary_has_exactly_one_test() {
    let files = test_files();
    assert!(files.len() > 20, "{}", files.len());
    // `support/` is compiled into every binary that declares `mod support;`, so a test there
    // runs beside the single test of a capture binary. None may live there.
    let support = rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support"));
    assert!(support.len() > 5, "{}", support.len());
    for (path, text) in &support {
        assert_eq!(
            test_count(text),
            0,
            "{} is shared by every test binary and must declare no test",
            path.display()
        );
    }
    let capturing: Vec<_> = files
        .iter()
        .filter(|(_, text)| uses_log_capture(text))
        .collect();
    // Never vacuous: the two binaries of plan-c deviation 7 are still there.
    for name in ["logs_account.rs", "oauth_provider_failures.rs"] {
        assert!(
            capturing.iter().any(|(path, _)| path.ends_with(name)),
            "{name} no longer uses the capture"
        );
    }
    for (path, text) in capturing {
        assert_eq!(
            test_count(text),
            1,
            "{} uses the in-process log capture and must hold exactly one test",
            path.display()
        );
    }

    // The counter sees each form, and the detector sees each way of calling the capture.
    let attr = |s: &str| ["#[", s, "]\nfn x() {}\n"].concat();
    for (source, expected) in [
        (attr("test"), 1),
        (attr("tokio::test"), 1),
        (attr("tokio::test(flavor = \"multi_thread\")"), 1),
        (attr("sqlx::test"), 1),
        (format!("{}{}", attr("test"), attr("tokio::test")), 2),
        (["proptest", "! { }"].concat(), 1),
        (attr("cfg(test)"), 0),
        (attr("allow(dead_code)"), 0),
    ] {
        assert_eq!(test_count(&source), expected, "{source}");
    }
    for source in [
        ["let logs = support::", "logs::", "run(\"info\", f);"].concat(),
        ["use support::", "logs;"].concat(),
        [
            "use support::{db, logs};\nlet l = logs::",
            "run(\"info\", f);",
        ]
        .concat(),
    ] {
        assert!(uses_log_capture(&source), "{source}");
    }
    assert!(!uses_log_capture("use support::db;"));
}
