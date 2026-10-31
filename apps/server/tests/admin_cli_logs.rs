//! 008 RF-C6 and ADR-0012 A8: the admin CLI is a new source of logs. Its events carry only
//! `event` (and `error_kind`), never the `user_id` nor the reason, in the three log levels, on
//! success, with a user that does not exist and with a command line that does not parse.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use support::bin;
use support::db::TestDb;
use uuid::Uuid;

const LEVELS: [&str; 3] = ["info", "debug", "debug,sqlx=trace"];
const PROVIDERS_LINE: &str = "providers within the admin cutoff: ";

async fn run(args: &[&str], envs: &[(&str, &str)]) -> bin::Finished {
    bin::spawn(args, envs).finish(Duration::from_secs(20)).await
}

/// No line of `finished` names the user (in any form) or the reason.
fn assert_clean(finished: &bin::Finished, user: Uuid, reason: &str, case: &str) {
    let forms = [
        user.to_string(),
        user.simple().to_string(),
        user.to_string().to_uppercase(),
        user.as_bytes().iter().map(|b| format!("{b:02x}")).collect(),
    ];
    for line in finished.stdout.iter().chain(&finished.stderr) {
        for form in &forms {
            assert!(!line.contains(form.as_str()), "{case}: the user in {line}");
        }
        assert!(!line.contains(reason), "{case}: the reason in {line}");
        assert!(!line.contains("c4n4ry"), "{case}: {line}");
    }
    assert!(finished.stderr.is_empty(), "{case}: {:?}", finished.stderr);
}

/// The line of providers of a `grant` is not a log (009 RF17): every other line is JSON.
fn events(finished: &bin::Finished) -> Vec<serde_json::Value> {
    finished
        .stdout
        .iter()
        .filter(|line| !line.starts_with(PROVIDERS_LINE))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// An event of the CLI (no `target`) has `event`, `level`, `ts` and at most an `error_kind`.
fn assert_own_events_have_only_their_fields(lines: &[serde_json::Value], case: &str) {
    for line in lines {
        let own = line.get("target").is_none();
        let mut keys: Vec<_> = line.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        if own {
            assert!(
                keys == ["event", "level", "ts"] || keys == ["error_kind", "event", "level", "ts"],
                "{case}: {line}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cli_never_logs_the_user_nor_the_reason() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let url = db.url();
    let (user, _) = support::app::user_with_session(&pool).await;
    let missing = Uuid::new_v4();
    let reason = format!("c4n4ry reason {}", Uuid::new_v4().simple());
    for level in LEVELS {
        let envs = [("DATABASE_URL", url.as_str()), ("RUST_LOG", level)];
        let user_text = user.to_string();
        let missing_text = missing.to_string();
        for (args, code, event, subject) in [
            (
                vec![
                    "admin",
                    "ban",
                    user_text.as_str(),
                    "--reason",
                    reason.as_str(),
                ],
                0,
                "admin.ban_applied",
                user,
            ),
            (
                vec!["admin", "unban", user_text.as_str()],
                0,
                "admin.unban_applied",
                user,
            ),
            (
                vec!["admin", "grant", user_text.as_str()],
                0,
                "admin.grant_applied",
                user,
            ),
            (
                vec!["admin", "revoke", user_text.as_str()],
                0,
                "admin.revoke_applied",
                user,
            ),
            (
                vec![
                    "admin",
                    "ban",
                    missing_text.as_str(),
                    "--reason",
                    reason.as_str(),
                ],
                1,
                "admin.user_not_found",
                missing,
            ),
            // A command line that does not parse: exit 2, and the message repeats nothing.
            (
                vec![
                    "admin",
                    "ban",
                    user_text.as_str(),
                    "--reason",
                    reason.as_str(),
                    "extra",
                ],
                2,
                "cli.invalid",
                user,
            ),
        ] {
            let case = format!("{level} {event}");
            let finished = run(&args, &envs).await;
            assert_eq!(
                finished.status.code(),
                Some(code),
                "{case}: {:?}",
                finished.stdout
            );
            assert_clean(&finished, subject, &reason, &case);
            let lines = events(&finished);
            assert!(
                lines.iter().any(|l| l["event"] == event),
                "{case}: {lines:?}"
            );
            assert_own_events_have_only_their_fields(&lines, &case);
        }
    }
}

/// 009 RF17: the line of providers has only their names: no `user_id` and no `provider_user_id`
/// in stdout or stderr, in any log level, and no event has more than its fields.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grant_with_two_providers_prints_only_their_names() {
    bin::warm_up().await;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let url = db.url();
    let (user, _) = support::app::user_with_session(&pool).await;
    let subjects = [
        ("google", format!("g-c4n4ry-{}", Uuid::new_v4().simple())),
        ("discord", format!("d-c4n4ry-{}", Uuid::new_v4().simple())),
    ];
    for (provider, subject) in &subjects {
        sqlx::query(
            "INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ($1, $2, $3)",
        )
        .bind(provider)
        .bind(subject)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    }
    let user_text = user.to_string();
    for level in LEVELS {
        let envs = [("DATABASE_URL", url.as_str()), ("RUST_LOG", level)];
        let finished = run(&["admin", "grant", user_text.as_str()], &envs).await;
        assert_eq!(finished.status.code(), Some(0), "{level}");
        assert_clean(&finished, user, "no reason here", level);
        let line = finished
            .stdout
            .iter()
            .find_map(|l| l.strip_prefix(PROVIDERS_LINE))
            .unwrap();
        assert_eq!(line, "google, discord", "{level}");
        for (_, subject) in &subjects {
            for out in finished.stdout.iter().chain(&finished.stderr) {
                assert!(!out.contains(subject.as_str()), "{level}: {out}");
            }
        }
        let lines = events(&finished);
        assert!(
            lines.iter().any(|l| l["event"] == "admin.grant_applied"),
            "{level}: {lines:?}"
        );
        assert_own_events_have_only_their_fields(&lines, level);
    }
}
