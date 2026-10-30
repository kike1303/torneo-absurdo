//! The name guard of the test databases (`support::db`). It lived in `support/db.rs`, which every
//! test binary compiles, so it ran once per binary and sat beside the single test of the log
//! capture binaries (see `tests/test_layout.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::db::is_test_name;

#[test]
fn test_names_are_validated() {
    assert!(is_test_name("torneo_test_0123456789abcdef0123456789abcdef"));
    assert!(!is_test_name("torneo"));
    assert!(!is_test_name(
        "torneo_test_0123456789ABCDEF0123456789abcdef"
    ));
    assert!(!is_test_name("torneo_test_x; DROP DATABASE torneo"));
}
