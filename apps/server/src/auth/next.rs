//! `next` after a login (ADR-0008 D2): only a relative path, never another origin.

/// `next` if it matches `^/(?![/\\])[^\x00-\x1f]*$` and has no other control character
/// either: DEL, which a `Location` header cannot carry, and the C1 controls. Otherwise `/`.
/// Written by hand: the `regex` crate has no lookahead.
pub fn sanitize(next: Option<&str>) -> String {
    match next {
        Some(path) if is_local_path(path) => path.to_owned(),
        _ => "/".to_owned(),
    }
}

fn is_local_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.first() == Some(&b'/')
        && !matches!(bytes.get(1), Some(b'/' | b'\\'))
        && !path.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn table() {
        for (input, expected) in [
            (None, "/"),
            (Some(""), "/"),
            (Some("/"), "/"),
            (Some("/es/onboarding"), "/es/onboarding"),
            (Some("/es/cuenta?tab=1#x"), "/es/cuenta?tab=1#x"),
            (Some("/a/b//c"), "/a/b//c"),
            (Some("https://evil.example"), "/"),
            (Some("//evil.example"), "/"),
            (Some("/\\evil.example"), "/"),
            (Some("evil"), "/"),
            (Some("/a\u{0}b"), "/"),
            (Some("/a\nb"), "/"),
            (Some("/a\tb"), "/"),
            (Some("/a\u{1f}b"), "/"),
            // DEL and the C1 controls are control characters too.
            (Some("/a\u{7f}b"), "/"),
            (Some("/a\u{85}b"), "/"),
            (Some("/a\u{9f}b"), "/"),
            (Some(" /a"), "/"),
            (Some("/é"), "/é"),
        ] {
            assert_eq!(sanitize(input), expected, "{input:?}");
        }
    }

    proptest! {
        #[test]
        fn the_result_is_always_a_local_path(input in proptest::option::of(".*")) {
            let next = sanitize(input.as_deref());
            prop_assert!(next.starts_with('/'));
            prop_assert!(!next.starts_with("//") && !next.starts_with("/\\"));
            prop_assert!(!next.chars().any(char::is_control));
            // It always makes a valid Location (checked before the login commits).
            prop_assert!(axum::http::HeaderValue::from_str(&next).is_ok());
            if let Some(input) = input.as_deref() {
                prop_assert!(next == "/" || next == input);
            }
        }
    }
}
