//! The admin CLI (ADR-0008 D9, D10; 008 RF-C6, plan-c decision 10):
//! `server admin grant|revoke|ban|unban <user_id>` (`ban` with `--reason <text>`).
//!
//! It reads only `DATABASE_URL` and `RUST_LOG` (an error in the config of `serve` never stops a
//! ban), opens one connection (no pool, no retries, 10 s to connect) and runs **one**
//! transaction: `SET LOCAL torneo.actor = 'cli'` (the actor `users_privileged_guard` accepts),
//! lock the user, change `role` or `banned_at`, revoke every session (its `NOTIFY` closes the
//! sockets of the serving process) and insert one row of `admin_audit`. It never runs the
//! migrations.
//!
//! A new source of logs (ADR-0012 A8): its events carry only `event` and, on a failure,
//! `error_kind`. Neither the `user_id` nor the reason ever reach `tracing`, and their `Debug` is
//! redacted. **The only SQL of the server that writes `role` or `banned_at` is in this file**,
//! checked by the source test at the end.

use std::ffi::OsString;
use std::fmt;
use std::time::Duration;

use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use crate::config::{self, ConfigError};
use crate::sessions::{self, RevokeFilter};

/// Longest `--reason`, in characters.
pub const MAX_REASON_CHARS: usize = 500;
/// Bound of the connection to the database.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAction {
    Grant,
    Revoke,
    Ban,
    Unban,
}

impl AdminAction {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "grant" => Some(Self::Grant),
            "revoke" => Some(Self::Revoke),
            "ban" => Some(Self::Ban),
            "unban" => Some(Self::Unban),
            _ => None,
        }
    }

    /// `admin_audit.action`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Revoke => "revoke",
            Self::Ban => "ban",
            Self::Unban => "unban",
        }
    }

    /// The event of a command that committed.
    pub const fn applied_event(self) -> &'static str {
        match self {
            Self::Grant => "admin.grant_applied",
            Self::Revoke => "admin.revoke_applied",
            Self::Ban => "admin.ban_applied",
            Self::Unban => "admin.unban_applied",
        }
    }
}

/// The reason of a ban: trimmed, 1 to [`MAX_REASON_CHARS`] characters, no control character.
/// Kept in `admin_audit`, never in a log; `Debug` hides it.
#[derive(Clone, PartialEq, Eq)]
pub struct Reason(String);

impl Reason {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let chars = text.chars().count();
        ((1..=MAX_REASON_CHARS).contains(&chars) && !text.chars().any(char::is_control))
            .then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Reason(..)")
    }
}

/// One command of the admin CLI. `Debug` shows the action only.
#[derive(Clone, PartialEq, Eq)]
pub struct AdminCommand {
    pub action: AdminAction,
    pub user: Uuid,
    /// `Some` exactly for `ban`.
    pub reason: Option<Reason>,
}

impl fmt::Debug for AdminCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdminCommand")
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

impl AdminCommand {
    /// The arguments after `admin`: `grant|revoke|unban <user_id>` or
    /// `ban <user_id> --reason <text>`, nothing else.
    pub fn parse(args: &[OsString]) -> Option<Self> {
        let words: Vec<&str> = args.iter().map(|a| a.to_str()).collect::<Option<_>>()?;
        let (action, user, reason) = match words.as_slice() {
            [action, user] => (AdminAction::parse(action)?, user, None),
            [action, user, flag, reason] if *flag == "--reason" => (
                AdminAction::parse(action)?,
                user,
                Some(Reason::parse(reason)?),
            ),
            _ => return None,
        };
        // `--reason` with `ban` and only with it.
        if (action == AdminAction::Ban) != reason.is_some() {
            return None;
        }
        Some(Self {
            action,
            user: Uuid::parse_str(user).ok()?,
            reason,
        })
    }
}

/// What the CLI reads from the environment.
pub struct AdminConfig {
    pub database: PgConnectOptions,
    pub rust_log: String,
}

impl AdminConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Result<Self, ConfigError> {
        let text = |var: &'static str| -> Result<Option<String>, ConfigError> {
            lookup(var)
                .map(|raw| raw.into_string().map_err(|_| ConfigError::Invalid { var }))
                .transpose()
        };
        const DATABASE_URL: &str = "DATABASE_URL";
        let database = text(DATABASE_URL)?
            .as_deref()
            .and_then(config::parse_database_url)
            .ok_or(ConfigError::Invalid { var: DATABASE_URL })?;
        const RUST_LOG: &str = "RUST_LOG";
        let rust_log = config::parse_rust_log(text(RUST_LOG)?)
            .ok_or(ConfigError::Invalid { var: RUST_LOG })?;
        Ok(Self { database, rust_log })
    }
}

/// Why a command did not apply. Codes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdminError {
    #[error("the user does not exist")]
    UserNotFound,
    #[error("the database failed")]
    Db(&'static str),
}

impl From<sqlx::Error> for AdminError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(crate::db::error_kind(&error))
    }
}

/// Runs `command` in one transaction, then logs its event (never its arguments).
pub async fn run(options: &PgConnectOptions, command: &AdminCommand) -> Result<(), AdminError> {
    let outcome = execute(options, command).await;
    match outcome {
        Ok(()) => tracing::info!(event = command.action.applied_event()),
        Err(AdminError::UserNotFound) => tracing::error!(event = "admin.user_not_found"),
        Err(AdminError::Db(kind)) => tracing::error!(event = "admin.failed", error_kind = kind),
    }
    outcome
}

async fn execute(options: &PgConnectOptions, command: &AdminCommand) -> Result<(), AdminError> {
    let mut connection = tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(options))
        .await
        .map_err(|_| AdminError::Db("timeout"))??;
    let mut tx = connection.begin().await?;
    // First statement: the actor of `users_privileged_guard`, for this transaction only.
    sqlx::query("SET LOCAL torneo.actor = 'cli'")
        .execute(&mut *tx)
        .await?;
    let found = sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(command.user)
        .fetch_optional(&mut *tx)
        .await?;
    if found.is_none() {
        tx.rollback().await?;
        return Err(AdminError::UserNotFound);
    }
    let change = match command.action {
        AdminAction::Grant => "UPDATE users SET role = 'admin' WHERE id = $1",
        AdminAction::Revoke => "UPDATE users SET role = 'user' WHERE id = $1",
        // A second ban keeps the date of the first.
        AdminAction::Ban => "UPDATE users SET banned_at = COALESCE(banned_at, now()) WHERE id = $1",
        AdminAction::Unban => "UPDATE users SET banned_at = NULL WHERE id = $1",
    };
    sqlx::query(change)
        .bind(command.user)
        .execute(&mut *tx)
        .await?;
    sessions::revoke(&mut tx, RevokeFilter::AllOfUser(command.user)).await?;
    let payload = match &command.reason {
        Some(reason) => serde_json::json!({ "reason": reason.as_str() }),
        None => serde_json::json!({}),
    };
    sqlx::query(
        "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
         VALUES ('cli', NULL, $1, $2, $3::jsonb)",
    )
    .bind(command.action.as_str())
    .bind(command.user.to_string())
    .bind(payload.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let _ = connection.close().await;
    Ok(())
}

/// RF-C6 (T9): the only SQL that writes `role` or `banned_at` lives in this file (plan-c
/// decision 10). The second barrier is `users_privileged_guard` in the database.
#[cfg(test)]
mod source_rules {
    use std::path::{Path, PathBuf};

    /// The source as tokens: line continuations of the literals removed, lowercase, the double
    /// quotes (escaped or not, and those of raw strings) removed, `,`, `;` and `=` as
    /// separators, `(` and `)` as tokens of their own.
    pub(super) fn tokens(source: &str) -> Vec<String> {
        let mut text = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            // `\` and a newline (with the indentation after it) inside a literal.
            if c == '\\' && chars.peek() == Some(&'\n') {
                chars.next();
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                text.push(' ');
                continue;
            }
            text.push(c);
        }
        let text = text
            .to_lowercase()
            .replace("r#\"", "\"")
            .replace("\"#", "\"")
            .replace("r\"", "\"")
            .replace("\\\"", "")
            .replace('"', "");
        let mut out = Vec::new();
        let mut current = String::new();
        let flush = |current: &mut String, out: &mut Vec<String>| {
            if !current.is_empty() {
                out.push(std::mem::take(current));
            }
        };
        for c in text.chars() {
            match c {
                ',' | ';' | '=' => flush(&mut current, &mut out),
                '(' | ')' => {
                    flush(&mut current, &mut out);
                    out.push(c.to_string());
                }
                c if c.is_whitespace() => flush(&mut current, &mut out),
                c => current.push(c),
            }
        }
        flush(&mut current, &mut out);
        out
    }

    /// `[<schema>.]users`.
    fn is_users(token: &str) -> bool {
        token == "users"
            || token.rsplit_once('.').is_some_and(|(schema, table)| {
                table == "users"
                    && !schema.is_empty()
                    && schema.chars().all(|c| c.is_alphanumeric() || c == '_')
            })
    }

    fn is_privileged(token: &str) -> bool {
        ["role", "banned_at"]
            .iter()
            .any(|column| token == *column || token.ends_with(&format!(".{column}")))
    }

    fn get(tokens: &[String], i: usize) -> &str {
        tokens.get(i).map_or("", String::as_str)
    }

    /// From `start` to a stop word, or to the end of the literal: a `)` that closes the call, a
    /// method call (`.bind`) or the end of the file.
    fn span(tokens: &[String], start: usize, stops: &[&str]) -> Vec<String> {
        let mut depth = 0_i32;
        let mut out = Vec::new();
        for token in tokens.iter().skip(start) {
            match token.as_str() {
                "(" => depth += 1,
                ")" => depth -= 1,
                t if depth == 0 && stops.contains(&t) => break,
                t if t.starts_with('.') || t.contains('{') || t.contains('}') => break,
                _ => {}
            }
            if depth < 0 {
                break;
            }
            out.push(token.clone());
        }
        out
    }

    /// The tokens of a parenthesized list starting at `start` (`start` is the `(`).
    fn list(tokens: &[String], start: usize) -> Vec<String> {
        let mut depth = 0_i32;
        let mut out = Vec::new();
        for token in tokens.iter().skip(start) {
            match token.as_str() {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            out.push(token.clone());
            if depth == 0 {
                break;
            }
        }
        out
    }

    /// The tokens each statement that writes `users` can write: the `set` of an `update`, the
    /// column list of an `insert` (all of it without a list), the rest of a `merge`, and the
    /// `set` of every `on conflict … do update`.
    pub(super) fn writes(tokens: &[String]) -> Vec<Vec<String>> {
        let mut found = Vec::new();
        for i in 0..tokens.len() {
            match get(tokens, i) {
                "update" if i == 0 || get(tokens, i - 1) != "do" => {
                    let mut j = i + 1;
                    if get(tokens, j) == "only" {
                        j += 1;
                    }
                    if !is_users(get(tokens, j)) {
                        continue;
                    }
                    j += 1;
                    if get(tokens, j) == "as" {
                        j += 2;
                    } else if get(tokens, j) != "set" {
                        j += 1;
                    }
                    if get(tokens, j) == "set" {
                        found.push(span(tokens, j + 1, &["where", "from", "returning"]));
                    }
                }
                "insert" if get(tokens, i + 1) == "into" && is_users(get(tokens, i + 2)) => {
                    let mut j = i + 3;
                    if get(tokens, j) == "as" {
                        j += 2;
                    } else if get(tokens, j) != "("
                        && !["values", "select", "default"].contains(&get(tokens, j))
                    {
                        j += 1;
                    }
                    if get(tokens, j) == "(" {
                        found.push(list(tokens, j));
                    } else {
                        // Without a column list an insert writes every column.
                        found.push(vec!["role".to_owned()]);
                    }
                }
                "merge" if get(tokens, i + 1) == "into" && is_users(get(tokens, i + 2)) => {
                    found.push(span(tokens, i + 3, &[]));
                }
                "do" if get(tokens, i + 1) == "update" && get(tokens, i + 2) == "set" => {
                    found.push(span(tokens, i + 3, &["where", "returning"]));
                }
                _ => {}
            }
        }
        found
    }

    pub(super) fn writes_privileged(source: &str) -> bool {
        writes(&tokens(source))
            .iter()
            .any(|span| span.iter().any(|t| is_privileged(t)))
    }

    fn sources() -> Vec<(PathBuf, String)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.push((path, text));
                }
            }
        }
        out
    }

    #[test]
    fn only_the_admin_cli_writes_role_or_banned_at() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/admin.rs");
        let files = sources();
        assert!(files.len() > 30, "{}", files.len());
        let actor = ["torneo", ".actor"].concat();
        for (path, text) in &files {
            if *path == here {
                continue;
            }
            assert!(
                !writes_privileged(text),
                "{} writes role or banned_at",
                path.display()
            );
            assert!(
                !text.to_lowercase().contains(&actor),
                "{} names the CLI actor",
                path.display()
            );
        }
        // Never vacuous: this file (its code, not these tests) still has both writes.
        let text = std::fs::read_to_string(&here).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap();
        let spans = writes(&tokens(code));
        for column in ["role", "banned_at"] {
            assert!(
                spans.iter().any(|s| s.iter().any(|t| t == column)),
                "admin.rs no longer writes {column}"
            );
        }
        assert!(code.to_lowercase().contains(&actor));
    }

    /// Files of `src/` allowed to build SQL from text. None: every value goes by `bind`.
    const DYNAMIC_SQL_ALLOWED: &[&str] = &[];

    /// Whether `source` builds SQL from text: sqlx's assert-safe wrapper anywhere, a leaked
    /// `String` or `Box` anywhere (`.leak(`, `Box::leak`, `String::leak`: a runtime
    /// `&'static str`, which sqlx 0.9 accepts as SQL), or a `format!` as the SQL of a `query*`
    /// call (008 plan-b decision 7; QA on phase C; #55). This is what would let a dynamic
    /// `UPDATE users SET role …` past the token check above. With sqlx 0.9 the `format!` form
    /// does not compile; it stays in case a later version takes a `String` again.
    pub(super) fn builds_sql_from_text(source: &str) -> bool {
        let leaks = [[".", "leak("].concat(), ["::", "leak("].concat()];
        if leaks.iter().any(|needle| source.contains(needle.as_str())) {
            return true;
        }
        let wrapper = ["assert", "sqlsafe"].concat();
        let tokens = tokens(source);
        tokens.iter().enumerate().any(|(i, token)| {
            if token.contains(&wrapper) {
                return true;
            }
            let call = token.rsplit("::").next().unwrap_or_default();
            let is_query =
                ["query", "query_as", "query_scalar", "query_with", "raw_sql"].contains(&call);
            is_query
                && get(&tokens, i + 1) == "("
                && get(&tokens, i + 2)
                    .trim_start_matches('&')
                    .starts_with("format!")
        })
    }

    #[test]
    fn no_sql_of_the_server_is_built_from_text() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/admin.rs");
        for (path, text) in sources() {
            // This file: its code, not the examples of these tests.
            let text = if path == here {
                text.split("#[cfg(test)]")
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            } else {
                text
            };
            let allowed = DYNAMIC_SQL_ALLOWED.iter().any(|name| path.ends_with(name));
            assert!(
                allowed || !builds_sql_from_text(&text),
                "{} builds SQL from text",
                path.display()
            );
        }
    }

    #[test]
    fn the_dynamic_sql_check_sees_each_form() {
        let wrapper = ["sqlx::Assert", "SqlSafe"].concat();
        for source in [
            format!("sqlx::query({wrapper}(format!(\"UPDATE users SET {{c}} = $1\")))"),
            format!("let q = {wrapper}(text);"),
            "sqlx::query(&format!(\"SELECT {} FROM users\", c))".to_owned(),
            "sqlx::query_as(format!(\"SELECT 1\"))".to_owned(),
            "query_scalar(format!(\"SELECT 1\"))".to_owned(),
            "sqlx::raw_sql(&format!(\"SET x = {}\", v))".to_owned(),
            // sqlx 0.9 only takes a `&'static str` (or the wrapper): a leaked `String` is one
            // (security-reviewer and architect on #55).
            "sqlx::query(format!(\"UPDATE users SET {c} = $1\").leak())".to_owned(),
            "let sql: &'static str = text.leak();".to_owned(),
            "let sql = Box::leak(text.into_boxed_str());".to_owned(),
            "let sql = std::boxed::Box::leak(b);".to_owned(),
            "let sql = String::leak(text);".to_owned(),
        ] {
            assert!(builds_sql_from_text(&source), "{source}");
        }
        for source in [
            "sqlx::query(\"UPDATE users SET locale = $1 WHERE id = $2\").bind(locale)",
            "let text = format!(\"/cuenta/{}\", section);",
            "sqlx::query(change).bind(command.user)",
        ] {
            assert!(!builds_sql_from_text(source), "{source}");
        }
    }

    #[test]
    fn the_normalizer_sees_every_form_of_a_write() {
        for sql in [
            r#"sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1")"#,
            r#"sqlx::query("update users u set u.role = 'admin'")"#,
            r#"sqlx::query("UPDATE users AS u SET u.banned_at = now()")"#,
            r#"sqlx::query("UPDATE public.users SET role = $1")"#,
            r#"sqlx::query("update public.users u set u.role = $1 where u.id = $2")"#,
            r#"sqlx::query("UPDATE \"users\" SET \"role\" = $1")"#,
            r#"sqlx::query("UPDATE \"public\".\"users\" SET banned_at = $1")"#,
            r#"sqlx::query(r"UPDATE ONLY users SET role='admin'")"#,
            "sqlx::query(\"UPDATE users \\\n         SET banned_at = now()\")",
            r#"sqlx::query(concat!("UPDATE ", "users", " SET role = 'x'"))"#,
            r#"sqlx::query("INSERT INTO users(display_name, role) VALUES ($1, $2)")"#,
            r#"sqlx::query("insert into users (display_name, banned_at) values ($1, now())")"#,
            r#"sqlx::query("INSERT INTO users VALUES ($1, $2, $3)")"#,
            r#"sqlx::query("INSERT INTO users AS u (display_name, role) VALUES ($1, $2)")"#,
            r#"sqlx::query("MERGE INTO users u USING x ON true WHEN MATCHED THEN UPDATE SET role = 'admin'")"#,
            r#"sqlx::query("INSERT INTO oauth_accounts (a) VALUES ($1) ON CONFLICT (a) DO UPDATE SET role = 'admin'")"#,
        ] {
            assert!(writes_privileged(sql), "{sql}");
        }
        for sql in [
            r#"sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2").bind(role)"#,
            r#"sqlx::query("UPDATE users SET locale = $1 WHERE role = 'admin'")"#,
            r#"sqlx::query("UPDATE users SET theme = $1 RETURNING role")"#,
            r#"sqlx::query("INSERT INTO users (display_name, display_name_key) VALUES ($1, $2)")"#,
            r#"sqlx::query("UPDATE sessions SET last_seen_at = now()")"#,
            r#"sqlx::query("SELECT role, banned_at FROM users")"#,
            r#"sqlx::query("INSERT INTO auth_events (user_id, kind) VALUES ($1, 'role')")"#,
        ] {
            assert!(!writes_privileged(sql), "{sql}");
        }
    }
}
