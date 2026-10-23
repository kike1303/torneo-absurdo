// Build script output goes through cargo's protocol on stdout and errors to stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

include!("src/version_rule.rs");

fn main() {
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    println!("cargo:rerun-if-changed=migrations");
    if let Some(value) = std::env::var_os("APP_VERSION") {
        let valid = value.to_str().is_some_and(is_valid_app_version);
        if !valid {
            eprintln!(
                "APP_VERSION must match ^(v\\d+\\.\\d+\\.\\d+|sha-[0-9a-f]{{7}}|dev)$ (RF-B10)"
            );
            std::process::exit(1);
        }
    }
}
