//! `next` after a login (ADR-0008 D2): only a relative path, never another origin.

use protocol::Locale;

/// `next` if it matches `^/(?![/\\])[^\x00-\x1f]*$` and has no other control character
/// either: DEL, which a `Location` header cannot carry, and the C1 controls. Otherwise `/`.
/// Written by hand: the `regex` crate has no lookahead.
pub fn sanitize(next: Option<&str>) -> String {
    match next {
        Some(path) if is_local_path(path) => path.to_owned(),
        _ => "/".to_owned(),
    }
}

/// The language of a `next` (TA-35): its first path segment, compared exactly, as one of the
/// fixed values. Nothing of `next` itself ever leaves this function.
pub fn locale_of(next: &str) -> Option<Locale> {
    let path = next.strip_prefix('/')?;
    match path.split(['/', '?', '#']).next()? {
        "en" => Some(Locale::En),
        "es" => Some(Locale::Es),
        _ => None,
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
    fn the_locale_is_an_exact_first_segment() {
        for (next, expected) in [
            ("/es", Some(Locale::Es)),
            ("/es/", Some(Locale::Es)),
            ("/es/welcome?next=%2Fen%2F", Some(Locale::Es)),
            ("/es?a=1", Some(Locale::Es)),
            ("/es#a", Some(Locale::Es)),
            ("/en/account", Some(Locale::En)),
            ("/", None),
            ("", None),
            ("es/welcome", None),
            ("/esp", None),
            ("/ES/welcome", None),
            ("/%65s/welcome", None),
            ("/es%2Fwelcome", None),
            ("/es\\evil.example", None),
            ("//es/", None),
            ("/fr/welcome", None),
            ("/cuenta/es/", None),
        ] {
            assert_eq!(locale_of(next), expected, "{next}");
        }
    }

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

    /// Paths that look like a `next`, with DEL, C0 and C1 control characters mixed in (a plain
    /// `.*` almost never produces them).
    fn next_like() -> impl Strategy<Value = String> {
        let piece = prop_oneof![
            4 => "[a-z/?=#.]{0,4}",
            1 => Just("\u{7f}".to_owned()),
            1 => Just("\u{85}".to_owned()),
            1 => Just("\u{9f}".to_owned()),
            1 => Just("\t".to_owned()),
            1 => Just("\\".to_owned()),
            1 => ".",
        ];
        (prop::bool::ANY, prop::collection::vec(piece, 0..6)).prop_map(|(slash, pieces)| {
            format!("{}{}", if slash { "/" } else { "" }, pieces.concat())
        })
    }

    proptest! {
        #[test]
        fn the_result_is_always_a_local_path(input in proptest::option::of(next_like())) {
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
