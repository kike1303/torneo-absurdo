//! The admin CLI (ADR-0008 D9, D10; 008 RF-C6, plan-c decision 10):
//! `server admin grant|revoke|ban|unban <user_id>` (`ban` with `--reason <text>`).
//!
//! It reads only `DATABASE_URL` and `RUST_LOG` (an error in the config of `serve` never stops a
//! ban), opens one connection (no pool, no retries, 10 s to connect) and runs **one**
//! transaction: `SET LOCAL torneo.actor = 'cli'` (the actor `users_privileged_guard` accepts),
//! lock the user, change `role` (with `admin_account_cutoff`, 009 RF17) or `banned_at`, revoke
//! every session (its `NOTIFY` closes the sockets of the serving process) and insert one row of
//! `admin_audit`. It never runs the migrations.
//!
//! A new source of logs (ADR-0012 A8): its events carry only `event` and, on a failure,
//! `error_kind`. Neither the `user_id` nor the reason ever reach `tracing`, and their `Debug` is
//! redacted. **The only SQL of the server that writes `role`, `banned_at` or
//! `admin_account_cutoff` is in this file**, checked by the source test at the end. After a
//! `grant`, one line of fixed text on stdout names the providers within the cutoff (RF17): not a
//! log, and never an id.

use std::ffi::OsString;
use std::fmt;
use std::io::Write as _;
use std::time::Duration;

use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

use crate::config::{self, ConfigError};
use crate::sessions::{self, RevokeFilter};

/// Longest `--reason`, in characters.
pub const MAX_REASON_CHARS: usize = 500;
/// Bound of the connection to the database.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// `server admin --help` and `server admin <action> --help` (009 RF17, plan decision 7). Text of
/// an operator tool, not of the UI: in English like the code, and `docs/architecture.md` says the
/// same in Spanish.
pub const HELP: &str = "\
server admin: the admin CLI. It reads only DATABASE_URL and RUST_LOG.

  server admin grant <user_id>                make the user an admin
  server admin revoke <user_id>               take admin away from the user
  server admin ban <user_id> --reason <text>  ban the user
  server admin unban <user_id>                lift the ban

Every command revokes all the sessions of the user.

When to run `grant` again:
  - after migrating to 009: no admin has a cutoff yet, so no session passes as admin;
  - after linking a new provider as an admin: it gives no admin until another `grant`.

Before running `grant`, review the linked providers in /account: everything linked until that
moment starts giving admin. `grant` prints the providers within the cutoff. If it shows one you
do not recognize, run `revoke`, remove it from /account and run `grant` again.
";

/// Whether the arguments after `admin` ask for [`HELP`]: `--help` or `<action> --help`.
pub fn is_help(args: &[OsString]) -> bool {
    match args {
        [flag] => flag == "--help",
        [action, flag] => {
            flag == "--help" && action.to_str().and_then(AdminAction::parse).is_some()
        }
        _ => false,
    }
}

/// The line of a `grant`, with the providers in link order.
fn providers_line(providers: &[String]) -> String {
    let names = if providers.is_empty() {
        "none".to_owned()
    } else {
        providers.join(", ")
    };
    format!("providers within the admin cutoff: {names}")
}

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
        // 009 RF17: every account linked until now gives admin, and none linked later.
        AdminAction::Grant => {
            "UPDATE users SET role = 'admin', admin_account_cutoff = \
             (SELECT max(id) FROM oauth_accounts WHERE user_id = $1) WHERE id = $1"
        }
        AdminAction::Revoke => {
            "UPDATE users SET role = 'user', admin_account_cutoff = NULL WHERE id = $1"
        }
        // A second ban keeps the date of the first.
        AdminAction::Ban => "UPDATE users SET banned_at = COALESCE(banned_at, now()) WHERE id = $1",
        AdminAction::Unban => "UPDATE users SET banned_at = NULL WHERE id = $1",
    };
    sqlx::query(change)
        .bind(command.user)
        .execute(&mut *tx)
        .await?;
    let within = if command.action == AdminAction::Grant {
        let rows = sqlx::query(
            "SELECT provider FROM oauth_accounts WHERE user_id = $1 \
             AND id <= (SELECT admin_account_cutoff FROM users WHERE id = $1) ORDER BY id",
        )
        .bind(command.user)
        .fetch_all(&mut *tx)
        .await?;
        let providers: Vec<String> = rows
            .iter()
            .map(|row| row.try_get(0))
            .collect::<Result<_, _>>()?;
        Some(providers)
    } else {
        None
    };
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
    // Not `tracing`: RF17 says stdout, never a log. Before the event of `run`. The commit is done,
    // so a failed write changes nothing.
    if let Some(providers) = within {
        let _ = writeln!(std::io::stdout().lock(), "{}", providers_line(&providers));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_of_providers_has_only_their_names() {
        assert_eq!(
            providers_line(&["google".to_owned(), "dev".to_owned()]),
            "providers within the admin cutoff: google, dev"
        );
        assert_eq!(
            providers_line(&[]),
            "providers within the admin cutoff: none"
        );
    }
}

/// RF-C6 (T9): the only SQL that writes `role`, `banned_at` or `admin_account_cutoff` lives in
/// this file (plan-c decision 10, 009 RF3). The second barrier is `users_privileged_guard` in the
/// database. 009 RF1: nothing rewrites the link order of `oauth_accounts`. TA-33: only
/// `sessions/` writes what a session authenticated with, and no migration switches a guard off.
#[cfg(test)]
mod source_rules {
    use std::path::{Path, PathBuf};

    /// The source as tokens: line continuations of the literals removed, lowercase, the double
    /// quotes (escaped or not, and those of raw strings) removed, `,` and `;` as separators, `(`,
    /// `)` and `=` as tokens of their own.
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
                ',' | ';' => flush(&mut current, &mut out),
                '(' | ')' | '=' => {
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

    /// `[<schema>.]<table>`.
    fn is_table(token: &str, table: &str) -> bool {
        token == table
            || token.rsplit_once('.').is_some_and(|(schema, name)| {
                name == table
                    && !schema.is_empty()
                    && schema.chars().all(|c| c.is_alphanumeric() || c == '_')
            })
    }

    /// `[<alias>.]<column>`, for one of `columns`.
    fn is_column(token: &str, columns: &[&str]) -> bool {
        columns
            .iter()
            .any(|column| token == *column || token.ends_with(&format!(".{column}")))
    }

    fn is_privileged(token: &str) -> bool {
        is_column(token, &["role", "banned_at", "admin_account_cutoff"])
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
                // A brace ends the Rust around the literal; `'{}'` is a value of the SQL.
                t if t.starts_with('.')
                    || (!t.contains('\'') && (t.contains('{') || t.contains('}'))) =>
                {
                    break;
                }
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

    /// What `writes` reports for an insert without a column list. Not a possible token: it has a
    /// space.
    const EVERY_COLUMN: &str = "every column";

    /// The tokens each statement that writes `table` can write: the `set` of an `update`, the
    /// column list of an `insert` (all of it without a list, and only with `inserts`), the rest
    /// of a `merge`, and the `set` of every `on conflict … do update`.
    pub(super) fn writes(tokens: &[String], table: &str, inserts: bool) -> Vec<Vec<String>> {
        let mut found = Vec::new();
        for i in 0..tokens.len() {
            match get(tokens, i) {
                "update" if i == 0 || get(tokens, i - 1) != "do" => {
                    let mut j = i + 1;
                    if get(tokens, j) == "only" {
                        j += 1;
                    }
                    if !is_table(get(tokens, j), table) {
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
                "insert"
                    if inserts
                        && get(tokens, i + 1) == "into"
                        && is_table(get(tokens, i + 2), table) =>
                {
                    let mut j = i + 3;
                    if get(tokens, j) == "as" {
                        j += 2;
                    } else if get(tokens, j) != "("
                        && !["values", "select", "default"].contains(&get(tokens, j))
                    {
                        j += 1;
                    }
                    // `insert into t (select …)` has no column list either.
                    if get(tokens, j) == "("
                        && !["select", "values", "with"].contains(&get(tokens, j + 1))
                    {
                        found.push(list(tokens, j));
                    } else {
                        // Without a column list an insert writes every column.
                        found.push(vec![EVERY_COLUMN.to_owned()]);
                    }
                }
                "merge" if get(tokens, i + 1) == "into" => {
                    let mut j = i + 2;
                    if get(tokens, j) == "only" {
                        j += 1;
                    }
                    if is_table(get(tokens, j), table) {
                        found.push(span(tokens, j + 1, &[]));
                    }
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
        writes(&tokens(source), "users", true)
            .iter()
            .any(|span| span.iter().any(|t| t == EVERY_COLUMN || is_privileged(t)))
    }

    /// What a span of `writes` assigns: the left side of each `=` of a `set`, so a value read on
    /// the right side does not count. An insert list and a merge (it can also insert, and only it
    /// has `using`) count whole.
    fn assigned(span: &[String]) -> Vec<String> {
        if !span.iter().any(|t| t == "=") || span.iter().any(|t| t == "using") {
            return span.to_vec();
        }
        let mut out = Vec::new();
        let mut depth = 0_i32;
        for (i, token) in span.iter().enumerate() {
            match token.as_str() {
                "(" => depth += 1,
                ")" => depth -= 1,
                "=" if depth == 0 && i > 0 => {
                    if span[i - 1] == ")" {
                        // `set (a, b) = …`.
                        let open = span[..i].iter().rposition(|t| t == "(").unwrap_or(0);
                        out.extend_from_slice(&span[open..i]);
                    } else {
                        out.push(span[i - 1].clone());
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// What a session authenticated with (009 RF2). A write elsewhere that "renews" it would let
    /// a stolen session reauthenticate itself.
    const SESSION_AUTH: [&str; 3] = ["authenticated_at", "auth_account_id", "auth_link_seq"];

    fn writes_session_auth(source: &str) -> bool {
        writes(&tokens(source), "sessions", true)
            .iter()
            .any(|span| {
                assigned(span)
                    .iter()
                    .any(|t| t == EVERY_COLUMN || is_column(t, &SESSION_AUTH))
            })
    }

    /// The database barriers of 009 RF1 and RF3 (`TA005` and `TA004`), with the version of the
    /// migration that creates each one.
    const GUARD_TRIGGERS: [(&str, i64); 2] = [
        ("users_privileged_guard", 20261029100000),
        ("oauth_accounts_identity_guard", 20261030000000),
    ];

    /// What takes a guard away when its statement names it.
    const WEAKENS_A_NAMED_GUARD: [&str; 8] = [
        "drop trigger",
        "disable trigger",
        "replica trigger",
        "alter trigger",
        "or replace trigger",
        // A body of `RETURN NEW` is a guard switched off: each new body is declared below.
        "or replace function",
        "drop function",
        "drop routine",
    ];

    /// What takes every guard away without naming one. On any table: it would also switch off
    /// the triggers of the ledger. The space ends the word: `user` is not `users_privileged_guard`.
    /// `torneo.actor` is what lets a write past `users_privileged_guard`: only the CLI sets it, and
    /// the bodies of the guard may read it. `alter role … set` falls under `set torneo.actor`.
    const WEAKENS_EVERY_GUARD: [&str; 10] = [
        "disable trigger all ",
        "disable trigger user ",
        "session_replication_role",
        "pg_trigger",
        "drop schema",
        "drop owned",
        "set torneo.actor",
        "set local torneo.actor",
        "set session torneo.actor",
        "set_config ( torneo.actor",
    ];

    /// The SQL without its comments. String literals stay whole: `'/api/*'` and `'--'` open
    /// nothing, and a statement run through `execute '…'` stays visible.
    fn without_sql_comments(sql: &str) -> String {
        let mut out = String::with_capacity(sql.len());
        let mut rest = sql;
        while let Some(c) = rest.chars().next() {
            let comment = [("--", "\n"), ("/*", "*/")]
                .into_iter()
                .find(|(open, _)| rest.starts_with(open));
            if let Some((_, end)) = comment {
                out.push(' ');
                rest = rest.split_once(end).map_or("", |(_, after)| after);
            } else if let Some(literal) = rest.strip_prefix('\'') {
                let len = literal.find('\'').map_or(literal.len(), |end| end + 1);
                out.push('\'');
                out.push_str(&literal[..len]);
                rest = &literal[len..];
            } else {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        out
    }

    /// The text of a migration as the rules read it: no comments, and no quotes of literals, so
    /// `tokens` also sees what a `do` block runs through `execute '…'`. `$` stays as a token: a
    /// bare `do` from `$do$` would read the next `update` as a `do update`.
    fn migration_sql(text: &str) -> String {
        without_sql_comments(text)
            .replace('\'', " ")
            .replace('$', " $ ")
    }

    /// Both readings of a migration: with the literals open (`execute '…'`), and with them closed
    /// (`'{}'` or `'from'` as a value does not end a `set`).
    fn migration_writes_session_auth(text: &str) -> bool {
        writes_session_auth(&migration_sql(text))
            || writes_session_auth(&without_sql_comments(text))
    }

    /// Each `(form, target)` of the SQL of a migration that switches a guard off. By statement and
    /// by substring, so a schema, quotes, `if exists` or a `do` block change nothing; a false
    /// positive only asks for a look.
    fn weakens_a_guard(sql: &str) -> Vec<(&'static str, &'static str)> {
        let mut found = Vec::new();
        for statement in migration_sql(sql).split(';') {
            let text = tokens(statement).join(" ") + " ";
            for (guard, _) in GUARD_TRIGGERS {
                if text.contains(guard) {
                    for form in WEAKENS_A_NAMED_GUARD {
                        if text.contains(form) {
                            found.push((form, guard));
                        }
                    }
                }
            }
            for form in WEAKENS_EVERY_GUARD {
                if text.contains(form) {
                    found.push((form.trim_end(), "every trigger"));
                }
            }
        }
        found
    }

    /// `(version, form, target)` of what a migration may do to a guard, in the order of the file.
    /// Only 009: the guard also had to refuse `admin_account_cutoff`, which took a new body and,
    /// as `UPDATE OF` can not be altered, a drop and a create in the same migration, which runs in
    /// one transaction. A later migration needs its own line here, and a review.
    const GUARD_CHANGES_ALLOWED: [(i64, &str, &str); 2] = [
        (
            20261030000000,
            "or replace function",
            "users_privileged_guard",
        ),
        (20261030000000, "drop trigger", "users_privileged_guard"),
    ];

    /// `(version, file name, text)` of every migration.
    fn migrations() -> Vec<(i64, String, String)> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                let version: i64 = name.split('_').next().unwrap().parse().unwrap();
                (version, name, std::fs::read_to_string(&path).unwrap())
            })
            .collect()
    }

    /// What 009 RF1 forbids over the link order of `oauth_accounts`, if `source` has any of it:
    /// writing `id` or `user_id` (an `insert` may name `user_id`: a new account is no rewrite),
    /// `overriding system value`, `setval`, `alter sequence`, or altering the column `id`.
    pub(super) fn rewrites_the_link_order(source: &str) -> Option<&'static str> {
        let tokens = tokens(source);
        let identity = ["id", "user_id"];
        if writes(&tokens, "oauth_accounts", false)
            .iter()
            .any(|span| span.iter().any(|t| is_column(t, &identity)))
        {
            return Some("a write of id or user_id");
        }
        for i in 0..tokens.len() {
            let token = get(&tokens, i);
            if token == "overriding" && get(&tokens, i + 1) == "system" {
                return Some("overriding system");
            }
            if is_column(token, &["setval"]) {
                return Some("setval");
            }
            if token == "alter" && get(&tokens, i + 1) == "sequence" {
                return Some("alter sequence");
            }
            if token == "alter" && get(&tokens, i + 1) == "table" {
                let mut j = i + 2;
                if get(&tokens, j) == "only" {
                    j += 1;
                }
                if is_table(get(&tokens, j), "oauth_accounts") && get(&tokens, j + 1) == "alter" {
                    let mut k = j + 2;
                    if get(&tokens, k) == "column" {
                        k += 1;
                    }
                    if get(&tokens, k) == "id" {
                        return Some("alter column id");
                    }
                }
            }
        }
        None
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
        let spans = writes(&tokens(code), "users", true);
        for column in ["role", "banned_at", "admin_account_cutoff"] {
            assert!(
                spans.iter().any(|s| s.iter().any(|t| t == column)),
                "admin.rs no longer writes {column}"
            );
        }
        assert!(code.to_lowercase().contains(&actor));
    }

    /// The migration of 009 is the only one allowed to number `oauth_accounts` (RF1).
    const LAST_ALLOWED_MIGRATION: i64 = 20261030000000;

    /// 009 RF1, with `oauth_accounts_identity_guard` as the second barrier: outside the migration
    /// of 009, nothing in `src/` or a later migration rewrites the link order.
    #[test]
    fn only_migrations_up_to_009_renumber_oauth_accounts() {
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
            assert_eq!(rewrites_the_link_order(&text), None, "{}", path.display());
        }
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut own = None;
        for entry in std::fs::read_dir(migrations).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let version: i64 = name.split('_').next().unwrap().parse().unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            if version == LAST_ALLOWED_MIGRATION {
                own = Some(text);
            } else if version > LAST_ALLOWED_MIGRATION {
                assert_eq!(rewrites_the_link_order(&text), None, "{name}");
            }
        }
        // Never vacuous: the migration of 009 exists, still has its `setval`, and this rule sees it.
        let own = own.expect("the migration of 009 is gone");
        assert!(tokens(&own).iter().any(|t| t == "setval"));
        assert!(rewrites_the_link_order(&own).is_some());
    }

    #[test]
    fn the_rule_of_the_link_order_sees_every_form() {
        for (sql, what) in [
            (
                r#"sqlx::query("UPDATE oauth_accounts SET user_id = $1")"#,
                "a write of id or user_id",
            ),
            (
                r#"sqlx::query("update public.oauth_accounts a set a.id = default")"#,
                "a write of id or user_id",
            ),
            (
                r#"sqlx::query("UPDATE \"oauth_accounts\" SET \"user_id\" = $1")"#,
                "a write of id or user_id",
            ),
            (
                r#"sqlx::query("UPDATE ONLY oauth_accounts SET id = DEFAULT")"#,
                "a write of id or user_id",
            ),
            (
                r#"sqlx::query("INSERT INTO oauth_accounts (provider) VALUES ($1) ON CONFLICT (provider) DO UPDATE SET user_id = $2")"#,
                "a write of id or user_id",
            ),
            (
                r#"sqlx::query("INSERT INTO oauth_accounts (id, provider) OVERRIDING SYSTEM VALUE VALUES ($1, $2)")"#,
                "overriding system",
            ),
            (r#"sqlx::query("SELECT setval('x', 1)")"#, "setval"),
            (
                r#"sqlx::query("SELECT pg_catalog.setval($1, 1)")"#,
                "setval",
            ),
            (
                "ALTER SEQUENCE oauth_accounts_id_seq RESTART WITH 1",
                "alter sequence",
            ),
            (
                "ALTER TABLE oauth_accounts ALTER COLUMN id RESTART WITH 1",
                "alter column id",
            ),
            (
                "alter table only public.oauth_accounts alter id drop identity",
                "alter column id",
            ),
            (
                "ALTER TABLE oauth_accounts ALTER id SET GENERATED BY DEFAULT",
                "alter column id",
            ),
        ] {
            assert_eq!(rewrites_the_link_order(sql), Some(what), "{sql}");
        }
        for sql in [
            r#"sqlx::query("INSERT INTO oauth_accounts (provider, provider_user_id, user_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING RETURNING id")"#,
            r#"sqlx::query("UPDATE oauth_accounts SET created_at = now() WHERE user_id = $1")"#,
            r#"sqlx::query("SELECT id, user_id FROM oauth_accounts WHERE provider = $1")"#,
            r#"sqlx::query("DELETE FROM oauth_accounts WHERE id = $1 RETURNING id")"#,
            r#"sqlx::query("UPDATE sessions SET auth_account_id = NULL WHERE id = $1")"#,
            r#"sqlx::query("UPDATE users SET locale = $1 WHERE id = $2")"#,
            "ALTER TABLE sessions ALTER COLUMN id SET NOT NULL",
            "ALTER TABLE oauth_accounts ADD COLUMN x bigint",
            ".layer(SetResponseHeaderLayer::overriding(",
        ] {
            assert_eq!(rewrites_the_link_order(sql), None, "{sql}");
        }
    }

    /// TA-33: only `sessions/` writes what a session authenticated with, in `src/` and in the
    /// migrations. Reading it is free.
    #[test]
    fn only_the_sessions_module_writes_the_session_authentication() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let (own, here) = (root.join("src/sessions"), root.join("src/admin.rs"));
        let mut written = Vec::new();
        for (path, text) in sources() {
            if path.starts_with(&own) {
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                written.extend(
                    writes(&tokens(code), "sessions", true)
                        .iter()
                        .flat_map(|s| assigned(s)),
                );
                continue;
            }
            // This file: its code, not the examples of these tests.
            let code = if path == here {
                text.split("#[cfg(test)]").next().unwrap_or_default()
            } else {
                text.as_str()
            };
            assert!(
                !writes_session_auth(code),
                "{} writes the authentication of a session",
                path.display()
            );
        }
        for (_, name, text) in migrations() {
            assert!(
                !migration_writes_session_auth(&text),
                "{name} writes the authentication of a session"
            );
        }
        // Never vacuous: `sessions/` still writes all three, and this rule sees it.
        for column in SESSION_AUTH {
            assert!(
                written.iter().any(|t| t == column),
                "sessions/ no longer writes {column}"
            );
        }
    }

    #[test]
    fn the_rule_of_the_session_authentication_sees_every_form() {
        for sql in [
            r#"sqlx::query("UPDATE sessions SET authenticated_at = now() WHERE id = $1")"#,
            r#"sqlx::query("update sessions set last_seen_at=now(),authenticated_at=now()")"#,
            r#"sqlx::query("UPDATE sessions s SET s.auth_account_id = $1")"#,
            r#"sqlx::query("UPDATE sessions AS s SET s.auth_link_seq = $1 RETURNING id")"#,
            r#"sqlx::query("UPDATE ONLY public.sessions SET authenticated_at = now()")"#,
            r#"sqlx::query("UPDATE \"sessions\" SET \"authenticated_at\" = now()")"#,
            r#"sqlx::query("UPDATE sessions SET (last_seen_at, authenticated_at) = (now(), now())")"#,
            "sqlx::query(\"UPDATE sessions \\\n         SET authenticated_at = now()\")",
            r#"sqlx::query(concat!("UPDATE sessions", " SET auth_link_seq = 1"))"#,
            r#"sqlx::query("WITH r AS (UPDATE sessions SET authenticated_at = now() RETURNING id) SELECT id FROM r")"#,
            r#"sqlx::query("INSERT INTO sessions (token_hash, authenticated_at) VALUES ($1, now())")"#,
            r#"sqlx::query("INSERT INTO public.sessions(token_hash, auth_account_id) SELECT $1, $2")"#,
            r#"sqlx::query("INSERT INTO sessions VALUES ($1, $2, $3)")"#,
            r#"sqlx::query("INSERT INTO sessions SELECT * FROM sessions")"#,
            r#"sqlx::query("INSERT INTO sessions (SELECT * FROM sessions)")"#,
            r#"sqlx::query("UPDATE sessions SET ip_hash = '{}', authenticated_at = now()")"#,
            r#"sqlx::query("INSERT INTO sessions (token_hash) VALUES ($1) ON CONFLICT (token_hash) DO UPDATE SET authenticated_at = now()")"#,
            r#"sqlx::query("MERGE INTO sessions s USING x ON true WHEN MATCHED THEN UPDATE SET authenticated_at = now()")"#,
            r#"sqlx::query("MERGE INTO ONLY sessions s USING x ON true WHEN NOT MATCHED THEN INSERT (auth_link_seq) VALUES (1)")"#,
        ] {
            assert!(writes_session_auth(sql), "{sql}");
        }
        for sql in [
            "DO $$ BEGIN EXECUTE 'UPDATE sessions SET authenticated_at = now()'; END $$;",
            "DO $do$ BEGIN EXECUTE 'INSERT INTO sessions (auth_link_seq) VALUES (1)'; END $do$;",
            "/* '--' */ UPDATE sessions SET auth_account_id = NULL; -- it's over",
            "DO $do$ BEGIN PERFORM 1; END $do$;\nUPDATE sessions SET authenticated_at = now();",
            "UPDATE sessions SET ip_hash = '{}', authenticated_at = now();",
            "UPDATE sessions SET ip_hash = 'from x', authenticated_at = now();",
        ] {
            assert!(migration_writes_session_auth(sql), "{sql}");
        }
        for sql in [
            r#"sqlx::query("SELECT authenticated_at, auth_account_id, auth_link_seq FROM sessions")"#,
            r#"sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE auth_account_id = $1")"#,
            r#"sqlx::query("UPDATE sessions SET last_seen_at = now() RETURNING authenticated_at")"#,
            r#"sqlx::query("UPDATE sessions SET expires_at = authenticated_at + interval '1 day'")"#,
            r#"sqlx::query("UPDATE sessions SET expires_at = (SELECT max(s.authenticated_at) FROM sessions s)")"#,
            r#"sqlx::query("UPDATE users u SET theme = s.auth_link_seq FROM sessions s WHERE s.user_id = u.id")"#,
            r#"sqlx::query("INSERT INTO auth_events (user_id, kind) SELECT s.user_id, s.auth_account_id FROM sessions s")"#,
            r#"sqlx::query("INSERT INTO sessions (token_hash) VALUES ($1) RETURNING authenticated_at")"#,
            r#"sqlx::query("INSERT INTO sessions (token_hash, user_id) SELECT $1, o.authenticated_at FROM sessions o")"#,
            // Split: `sessions_are_deleted_only_here` reads this file too.
            concat!(
                r#"sqlx::query("DELETE FROM"#,
                r#" sessions WHERE authenticated_at < $1")"#
            ),
            r#"sqlx::query("UPDATE users SET locale = $1 WHERE id = $2")"#,
        ] {
            assert!(!writes_session_auth(sql), "{sql}");
        }
    }

    /// TA-33: no migration takes away `users_privileged_guard` or the trigger of `TA005`, beyond
    /// `GUARD_CHANGES_ALLOWED`.
    #[test]
    fn no_migration_switches_a_guard_off() {
        let all = migrations();
        for (version, name, text) in &all {
            let allowed: Vec<_> = GUARD_CHANGES_ALLOWED
                .iter()
                .filter(|(allowed, _, _)| allowed == version)
                .map(|(_, form, target)| (*form, *target))
                .collect();
            assert_eq!(weakens_a_guard(text), allowed, "{name}");
        }
        // Never vacuous: each guard is created where this file says, and the one drop allowed is
        // followed by its create.
        let normalized = |version: i64| {
            let (_, _, text) = all.iter().find(|(v, _, _)| *v == version).unwrap();
            tokens(&without_sql_comments(text)).join(" ")
        };
        for (guard, created) in GUARD_TRIGGERS {
            assert!(
                normalized(created).contains(&format!("create trigger {guard}")),
                "{guard} is not created in {created}"
            );
        }
        for (version, form, target) in GUARD_CHANGES_ALLOWED {
            if form != "drop trigger" {
                continue;
            }
            let text = normalized(version);
            let dropped = text.rfind(&format!("{form} {target}")).unwrap();
            let created = text.rfind(&format!("create trigger {target}")).unwrap();
            assert!(dropped < created, "{version} leaves {target} dropped");
        }
    }

    #[test]
    fn the_rule_of_the_guards_sees_every_form() {
        let (users, oauth) = ("users_privileged_guard", "oauth_accounts_identity_guard");
        for (sql, form, target) in [
            (
                "DROP TRIGGER users_privileged_guard ON users;",
                "drop trigger",
                users,
            ),
            (
                "drop trigger if exists \"oauth_accounts_identity_guard\" on public.oauth_accounts",
                "drop trigger",
                oauth,
            ),
            (
                "DROP\n  TRIGGER /* why */ users_privileged_guard -- why\n ON users",
                "drop trigger",
                users,
            ),
            (
                "DO $$ BEGIN EXECUTE 'DROP TRIGGER oauth_accounts_identity_guard ON oauth_accounts'; END $$;",
                "drop trigger",
                oauth,
            ),
            (
                "ALTER TABLE users DISABLE TRIGGER users_privileged_guard",
                "disable trigger",
                users,
            ),
            (
                "alter table only public.oauth_accounts add column x int, disable trigger oauth_accounts_identity_guard",
                "disable trigger",
                oauth,
            ),
            (
                "ALTER TABLE users ENABLE REPLICA TRIGGER users_privileged_guard",
                "replica trigger",
                users,
            ),
            (
                "ALTER TRIGGER users_privileged_guard ON users RENAME TO x",
                "alter trigger",
                users,
            ),
            (
                "CREATE OR REPLACE TRIGGER users_privileged_guard BEFORE DELETE ON users FOR EACH ROW EXECUTE FUNCTION f()",
                "or replace trigger",
                users,
            ),
            (
                "DROP FUNCTION users_privileged_guard() CASCADE",
                "drop function",
                users,
            ),
            (
                "DROP FUNCTION IF EXISTS f(), public.oauth_accounts_identity_guard CASCADE",
                "drop function",
                oauth,
            ),
            (
                "DROP ROUTINE users_privileged_guard CASCADE",
                "drop routine",
                users,
            ),
            (
                "ALTER TABLE users DISABLE TRIGGER ALL",
                "disable trigger all",
                "every trigger",
            ),
            (
                "ALTER TABLE oauth_accounts DISABLE TRIGGER USER",
                "disable trigger user",
                "every trigger",
            ),
            (
                "SET LOCAL session_replication_role = replica",
                "session_replication_role",
                "every trigger",
            ),
            (
                "SELECT set_config('session_replication_role', 'replica', true)",
                "session_replication_role",
                "every trigger",
            ),
            (
                "DO $$ BEGIN EXECUTE 'ALTER TABLE users DISABLE TRIGGER ALL'; END $$;",
                "disable trigger all",
                "every trigger",
            ),
            (
                "SELECT '/api/*'; DROP TRIGGER users_privileged_guard ON users;",
                "drop trigger",
                users,
            ),
            (
                "SELECT '--'; DROP TRIGGER users_privileged_guard ON users;",
                "drop trigger",
                users,
            ),
            (
                "-- it's gone\nDROP TRIGGER oauth_accounts_identity_guard ON oauth_accounts;",
                "drop trigger",
                oauth,
            ),
            (
                "CREATE OR REPLACE FUNCTION users_privileged_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END; $$;",
                "or replace function",
                users,
            ),
            (
                "UPDATE pg_trigger SET tgenabled = 'D'",
                "pg_trigger",
                "every trigger",
            ),
            ("DROP SCHEMA public CASCADE", "drop schema", "every trigger"),
            (
                "DROP OWNED BY CURRENT_USER CASCADE",
                "drop owned",
                "every trigger",
            ),
            (
                "SET LOCAL torneo.actor = 'cli'",
                "set local torneo.actor",
                "every trigger",
            ),
            (
                "SET torneo.actor TO 'cli'",
                "set torneo.actor",
                "every trigger",
            ),
            (
                "ALTER ROLE CURRENT_USER SET torneo.actor = 'cli'",
                "set torneo.actor",
                "every trigger",
            ),
            (
                "SELECT set_config('torneo.actor', 'cli', true)",
                "set_config ( torneo.actor",
                "every trigger",
            ),
        ] {
            assert_eq!(weakens_a_guard(sql), vec![(form, target)], "{sql}");
        }
        for sql in [
            "CREATE TRIGGER users_privileged_guard BEFORE INSERT ON users FOR EACH ROW EXECUTE FUNCTION users_privileged_guard();",
            "CREATE FUNCTION users_privileged_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF current_setting('torneo.actor', true) = 'cli' THEN RETURN NEW; END IF; END; $$;",
            "DROP TRIGGER other ON users; ALTER TABLE users ADD COLUMN users_privileged_guard_note text;",
            "ALTER TABLE ledger_entries DISABLE TRIGGER other;",
            "-- DROP TRIGGER users_privileged_guard ON users\n/* ALTER TABLE users DISABLE TRIGGER ALL */ SELECT 1;",
        ] {
            assert_eq!(weakens_a_guard(sql), vec![], "{sql}");
        }
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
            r#"sqlx::query("UPDATE users SET admin_account_cutoff = $1 WHERE id = $2")"#,
            r#"sqlx::query("update users u set u.admin_account_cutoff = null")"#,
            r#"sqlx::query("INSERT INTO users (display_name, admin_account_cutoff) VALUES ($1, 1)")"#,
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
            r#"sqlx::query("INSERT INTO users (SELECT * FROM x)")"#,
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
