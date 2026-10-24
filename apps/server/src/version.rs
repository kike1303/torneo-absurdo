/// `APP_VERSION` fixed at compile time (`--build-arg` in the release image), `dev` otherwise.
/// `build.rs` rejects a value that does not match the rule in `version_rule.rs`.
pub const APP_VERSION: &str = match option_env!("APP_VERSION") {
    Some(version) => version,
    None => "dev",
};
