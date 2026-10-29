// Unit tests of `sessions` (008 RF-B8, RF-B10). Included from sessions/mod.rs.

use sha2::Digest;

use super::*;

#[test]
fn tokens_are_256_bits_of_base64url_and_only_their_hash_is_kept() {
    let token = SessionToken::generate().unwrap();
    assert_eq!(token.as_str().len(), 43);
    assert!(
        token
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
    let expected: [u8; 32] = sha2::Sha256::digest(token.as_str().as_bytes()).into();
    assert_eq!(token.hash().0, expected);
    assert_ne!(SessionToken::generate().unwrap().as_str(), token.as_str());
    assert_eq!(TokenHash::of(token.as_str()), token.hash());
}

#[test]
fn debug_never_shows_tokens_hashes_or_ids() {
    let token = SessionToken::generate().unwrap();
    let id = SessionId(uuid::Uuid::new_v4());
    for shown in [
        format!("{token:?}"),
        format!("{:?}", token.hash()),
        format!("{id:?}"),
    ] {
        assert!(!shown.contains(token.as_str()), "{shown}");
        assert!(!shown.contains(&id.0.to_string()), "{shown}");
        assert!(shown.ends_with("(..)"), "{shown}");
    }
}

#[test]
fn user_agent_families_come_from_a_closed_set() {
    let cases = [
        (
            "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 Chrome/131.0 Safari/537.36 Edg/131.0",
            UaFamily::Edge,
        ),
        (
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0 Safari/537.36",
            UaFamily::Chrome,
        ),
        (
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) AppleWebKit/605.1.15 CriOS/131.0 Mobile/15E148 Safari/604.1",
            UaFamily::Chrome,
        ),
        (
            "Mozilla/5.0 (Macintosh) Gecko/20100101 Firefox/133.0",
            UaFamily::Firefox,
        ),
        (
            "Mozilla/5.0 (iPhone) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1",
            UaFamily::Safari,
        ),
        ("curl/8.0", UaFamily::Other),
        ("", UaFamily::Other),
    ];
    for (ua, family) in cases {
        assert_eq!(UaFamily::of(Some(ua)), family, "{ua}");
    }
    assert_eq!(UaFamily::of(None), UaFamily::Other);
    for family in [
        UaFamily::Chrome,
        UaFamily::Firefox,
        UaFamily::Safari,
        UaFamily::Edge,
        UaFamily::Other,
    ] {
        assert!(["chrome", "firefox", "safari", "edge", "other"].contains(&family.as_str()));
    }
}

/// ADR-0008 D3, T10: the only `DELETE FROM sessions` lives in this module.
#[test]
fn sessions_are_deleted_only_here() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let here = src.join("sessions");
    let needle = ["DELETE FROM", " SESSIONS"].concat();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.starts_with(&here) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap().to_ascii_uppercase();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                !text.contains(&needle),
                "{} deletes sessions",
                path.display()
            );
        }
    }
}

#[test]
fn the_absolute_expiry_depends_on_the_role() {
    assert_eq!(max_age_secs(protocol::Role::User), 30 * 24 * 3600);
    assert_eq!(max_age_secs(protocol::Role::Admin), 12 * 3600);
}
