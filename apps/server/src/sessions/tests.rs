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

/// 009 plan decision 2: the windows of the SQL are exactly the documented constants, each in the
/// fragment it belongs to, and nothing else of the rule names a window.
#[test]
fn the_windows_of_the_sql_are_the_constants() {
    let window = |minutes: i64| format!("interval '{minutes} minutes'");
    let step_up = window(STEP_UP_WINDOW_MINUTES);
    let link = window(LINK_CALLBACK_WINDOW_MINUTES);
    let admin = window(RECENT_ADMIN_WINDOW_MINUTES);
    assert_eq!(
        (
            STEP_UP_WINDOW_MINUTES,
            LINK_CALLBACK_WINDOW_MINUTES,
            RECENT_ADMIN_WINDOW_MINUTES
        ),
        (10, 20, 60)
    );
    for sql in [
        const_format_valid::BY_HASH,
        const_format_valid::BY_ID,
        const_format_valid::ANY_ID,
    ] {
        assert_eq!(sql.matches(&step_up).count(), 1, "{sql}");
        assert_eq!(sql.matches(&admin).count(), 1, "{sql}");
        assert!(!sql.contains(&link), "{sql}");
    }
    let view = const_format_valid::AUTH_VIEW;
    assert_eq!(view.matches(&step_up).count(), 1, "{view}");
    assert!(!view.contains(&link) && !view.contains(&admin), "{view}");
    let lock = const_format_valid::LOCK_AUTH;
    assert_eq!(lock.matches(&step_up).count(), 1, "{lock}");
    assert_eq!(lock.matches(&link).count(), 1, "{lock}");
    assert!(!lock.contains(&admin), "{lock}");
    // Every boolean of the rule is NULL-safe: a session of before 009 is "no", not an error.
    for sql in [const_format_valid::BY_HASH, lock] {
        assert_eq!(sql.matches("COALESCE(").count(), 2, "{sql}");
    }
}

/// 009 RF14 (plan, step 6): the row of the session stays locked until the transaction ends, so
/// a logout that arrives during an unlink waits for it.
#[test]
fn lock_auth_locks_the_row_of_the_session() {
    assert!(
        const_format_valid::LOCK_AUTH.ends_with(" FOR UPDATE OF s"),
        "{}",
        const_format_valid::LOCK_AUTH
    );
}

/// RNF of logs (009): no id of a session, a user or an OAuth account in a `Debug`.
#[test]
fn debug_never_shows_the_ids_of_the_authentication() {
    let account = OauthAccountId::new(4_242_424_242);
    let user = uuid::Uuid::new_v4();
    let session = SessionId(uuid::Uuid::new_v4());
    let info = SessionInfo {
        id: session,
        user_id: user,
        role: protocol::Role::User,
        step_up: true,
        recent_admin: false,
    };
    let locked = LockedAuth {
        user_id: user,
        step_up: true,
        link_window: true,
        account: Some(account),
    };
    let view = AuthView {
        step_up: true,
        account: Some(account),
        linked: vec![(protocol::Provider::Google, account)],
    };
    for shown in [
        format!("{account:?}"),
        format!("{info:?}"),
        format!("{locked:?}"),
        format!("{view:?}"),
        format!("{:?}", Authentication::Fresh(account)),
        format!("{:?}", Authentication::KeptFrom(session)),
    ] {
        assert!(!shown.contains("4242424242"), "{shown}");
        assert!(!shown.contains(&user.to_string()), "{shown}");
        assert!(!shown.contains(&user.simple().to_string()), "{shown}");
        assert!(!shown.contains(&session.0.to_string()), "{shown}");
    }
    let token = SessionToken::generate().unwrap();
    let secret = token.as_str().to_owned();
    let issued = format!(
        "{:?}",
        Issued {
            id: session,
            token,
            max_age_secs: 1,
        }
    );
    assert!(!issued.contains(&secret), "{issued}");
    assert!(!issued.contains(&session.0.to_string()), "{issued}");
    assert!(format!("{info:?}").contains("step_up: true"));
    assert!(format!("{locked:?}").contains("link_window: true"));
}
