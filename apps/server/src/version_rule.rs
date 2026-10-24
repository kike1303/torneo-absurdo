// Rule for `APP_VERSION` (RF-B10): `vX.Y.Z`, `sha-<7 lowercase hex>` or `dev`.
// Dependency-free and without inner attributes or doc comments, because `build.rs` pulls it in
// with `include!` to reject an invalid `APP_VERSION` at compile time (plan-b decision 15).

/// Whether `value` matches `^(v\d+\.\d+\.\d+|sha-[0-9a-f]{7}|dev)$`.
pub fn is_valid_app_version(value: &str) -> bool {
    if value == "dev" {
        return true;
    }
    if let Some(hex) = value.strip_prefix("sha-") {
        return hex.len() == 7
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    }
    if let Some(numbers) = value.strip_prefix('v') {
        let parts: Vec<&str> = numbers.split('.').collect();
        return parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_valid_app_version;

    #[test]
    fn accepts_the_three_forms() {
        for value in [
            "v1.2.3",
            "v0.0.1",
            "v10.20.30",
            "sha-abcdef0",
            "sha-0123456",
            "dev",
        ] {
            assert!(is_valid_app_version(value), "{value}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for value in [
            "v1.2",
            "v1.2.3.4",
            "1.2.3",
            "v1.2.x",
            "v1..3",
            "sha-ABCDEF0",
            "sha-abcdef",
            "sha-abcdef01",
            "sha-abcdefg",
            "latest",
            "",
            "dev ",
            "Dev",
        ] {
            assert!(!is_valid_app_version(value), "{value:?}");
        }
    }
}
