//! `server season farm-report|exclude|close` (feature 020, phase B; ADR-0008 D8): the review of a
//! season for farms, the exclusion of a user from its ranking and its close. Run in the container as
//! `server fight void` is, with the same configuration and the same sort of connection. There is no
//! endpoint: `cli.rs` is the only caller.
//!
//! The three share the rules of 020: `--by` is an admin who is not banned, they write with
//! `actor = 'cli_attributed'` and `torneo.actor = 'cli'`, and each refusal leaves its code on stdout
//! and writes nothing, since everything happens in one transaction that a refusal rolls back.
//!
//! **This file holds the only SQL that writes a `season_review`** (the source check of
//! `tests/season_cli.rs`), and `admin_audit_season_review_guard` is the second barrier. `close`
//! trusts the newest review, so nothing else may fake one.
//!
//! `farm-report` groups the accounts by `ip_hash` with opaque labels (`ip_group: 1, 2…`), numbered in
//! the database by the first event of each hash: the values of `ip_hash` never leave Postgres, so
//! they reach neither stdout, nor `payload`, nor `tracing`. Its logs, as those of `fight void`, carry
//! only `event` and `error_kind` (ADR-0012 A8).

use std::ffi::OsString;
use std::fmt;
use std::io::Write as _;
use std::time::Duration;

use serde::Serialize;
use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

use super::{CONNECT_TIMEOUT, parse_canonical_uuid};
use crate::fights::store;
use crate::seasons::{self, api::parse_season, board};

/// How long a command waits for a row (the season, or a stat a settlement holds).
pub const SEASON_LOCK_TIMEOUT_MS: u64 = 5000;
/// The longest a whole command can take, as `VOID_DEADLINE`: a database that stops answering must
/// end in a failure, not in a process that hangs.
pub const SEASON_DEADLINE: Duration = Duration::from_secs(120);
/// The users of the leaderboard whose counterparts `farm-report` measures (D8).
pub const REVIEWED_TOP: usize = 20;
/// The counterparts of each that count for the concentration (D8).
pub const TOP_COUNTERPARTS: usize = 5;
/// Above this share of the `RankedProfit`, in basis points, a user is marked (D8: 50 %).
pub const CONCENTRATION_THRESHOLD_BP: i64 = 5000;

/// Why an admin excludes a user (020): a closed list, so the audit has codes and never text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcludeReason {
    FarmSignals,
    SharedIpOppositeSides,
    CounterpartConcentration,
}

impl ExcludeReason {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "farm_signals" => Some(Self::FarmSignals),
            "shared_ip_opposite_sides" => Some(Self::SharedIpOppositeSides),
            "counterpart_concentration" => Some(Self::CounterpartConcentration),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FarmSignals => "farm_signals",
            Self::SharedIpOppositeSides => "shared_ip_opposite_sides",
            Self::CounterpartConcentration => "counterpart_concentration",
        }
    }
}

/// What a `server season` command does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SeasonAction {
    FarmReport,
    Exclude { user: Uuid, reason: ExcludeReason },
    Close { accept_partial: bool },
}

/// `server season …`. `Debug` shows neither the season, nor the users, nor the admin (ADR-0012 A8).
#[derive(Clone, PartialEq, Eq)]
pub struct SeasonCommand {
    pub season: i32,
    pub by: Uuid,
    pub action: SeasonAction,
}

impl fmt::Debug for SeasonCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let action = match self.action {
            SeasonAction::FarmReport => "FarmReport",
            SeasonAction::Exclude { .. } => "Exclude",
            SeasonAction::Close { .. } => "Close",
        };
        f.debug_struct("SeasonCommand")
            .field("action", &action)
            .finish_non_exhaustive()
    }
}

impl SeasonCommand {
    /// The arguments after `season`: the action, its positionals, then its flags in any order and
    /// none repeated. Any other flag is invalid.
    pub fn parse(args: &[OsString]) -> Option<Self> {
        let words: Vec<&str> = args.iter().map(|a| a.to_str()).collect::<Option<_>>()?;
        let (action, rest) = words.split_first()?;
        let (season, mut rest) = rest.split_first()?;
        let season = parse_season(season)?;
        let user = if *action == "exclude" {
            let (user, tail) = rest.split_first()?;
            rest = tail;
            Some(parse_canonical_uuid(user)?)
        } else {
            None
        };
        let (mut by, mut reason, mut accept_partial) = (None, None, false);
        while let Some((flag, tail)) = rest.split_first() {
            match *flag {
                "--by" if by.is_none() => {
                    let (value, tail) = tail.split_first()?;
                    by = Some(parse_canonical_uuid(value)?);
                    rest = tail;
                }
                "--reason" if *action == "exclude" && reason.is_none() => {
                    let (value, tail) = tail.split_first()?;
                    reason = Some(ExcludeReason::parse(value)?);
                    rest = tail;
                }
                "--accept-partial" if *action == "close" && !accept_partial => {
                    accept_partial = true;
                    rest = tail;
                }
                _ => return None,
            }
        }
        let action = match *action {
            "farm-report" => SeasonAction::FarmReport,
            "exclude" => SeasonAction::Exclude {
                user: user?,
                reason: reason?,
            },
            "close" => SeasonAction::Close { accept_partial },
            _ => return None,
        };
        Some(Self {
            season,
            by: by?,
            action,
        })
    }

    const fn name(&self) -> &'static str {
        match self.action {
            SeasonAction::FarmReport => "farm_report",
            SeasonAction::Exclude { .. } => "exclude",
            SeasonAction::Close { .. } => "close",
        }
    }
}

/// Why a command did not apply: a refusal, with its code, decided something; a failure of the
/// database or of the connection decided nothing, and can be tried again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SeasonFailure {
    #[error("the season command was refused")]
    Refused(&'static str),
    #[error("the season command failed")]
    Failed(&'static str),
}

impl From<sqlx::Error> for SeasonFailure {
    fn from(error: sqlx::Error) -> Self {
        Self::Failed(crate::db::error_kind(&error))
    }
}

const fn refused(code: &'static str) -> SeasonFailure {
    SeasonFailure::Refused(code)
}

/// `server season …`: runs `command` in one transaction, prints what it says (the report, or one
/// code) and logs its event and nothing else. `cli.rs` is the only caller.
///
/// # Errors
/// [`SeasonFailure`].
pub async fn run_season(
    options: &PgConnectOptions,
    command: &SeasonCommand,
) -> Result<(), SeasonFailure> {
    let outcome = execute_season(options, command, SEASON_DEADLINE).await;
    let mut stdout = std::io::stdout().lock();
    let name = command.name();
    match &outcome {
        // Not `tracing`: as the line of `fight void`. The commit is done, so a failed write changes
        // nothing; a report lost this way is printed again by running it again.
        Ok(text) => {
            let _ = writeln!(stdout, "{text}");
            tracing::info!(event = "season.applied", command = name);
        }
        Err(SeasonFailure::Refused(code)) => {
            let _ = writeln!(stdout, "{code}");
            tracing::error!(event = "season.refused", command = name, error_kind = *code);
        }
        Err(SeasonFailure::Failed(kind)) => {
            tracing::error!(event = "season.failed", command = name, error_kind = *kind);
        }
    }
    outcome.map(|_| ())
}

/// [`run_season`] without the printing and the log, and with its deadline: what the tests call. The
/// text is what the command prints: the JSON of the report, or the code of what it applied.
///
/// # Errors
/// [`SeasonFailure`].
pub async fn execute_season(
    options: &PgConnectOptions,
    command: &SeasonCommand,
    deadline: Duration,
) -> Result<String, SeasonFailure> {
    tokio::time::timeout(deadline, apply(options, command))
        .await
        .unwrap_or(Err(SeasonFailure::Failed("timeout")))
}

async fn apply(
    options: &PgConnectOptions,
    command: &SeasonCommand,
) -> Result<String, SeasonFailure> {
    let mut connection = tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(options))
        .await
        .map_err(|_| SeasonFailure::Failed("timeout"))??;
    let mut tx = connection.begin().await?;
    let written = write(&mut tx, command).await;
    let text = match written {
        Ok(text) => {
            tx.commit().await?;
            text
        }
        Err(failure) => {
            // Without a `ROLLBACK` that arrives, the end of the connection reverts it too.
            let _ = tx.rollback().await;
            let _ = connection.close().await;
            return Err(failure);
        }
    };
    let _ = connection.close().await;
    Ok(text)
}

async fn write(tx: &mut PgConnection, command: &SeasonCommand) -> Result<String, SeasonFailure> {
    if command.action == SeasonAction::FarmReport {
        // One snapshot for the whole report: every section sees the same season. First statement.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("SET LOCAL torneo.actor = 'cli'")
        .execute(&mut *tx)
        .await?;
    store::set_lock_timeout(&mut *tx, SEASON_LOCK_TIMEOUT_MS).await?;
    let admin =
        sqlx::query("SELECT 1 FROM users WHERE id = $1 AND role = 'admin' AND banned_at IS NULL")
            .bind(command.by)
            .fetch_optional(&mut *tx)
            .await?;
    if admin.is_none() {
        return Err(refused("season.not_admin"));
    }
    match command.action {
        SeasonAction::FarmReport => farm_report(tx, command).await,
        SeasonAction::Exclude { user, reason } => exclude(tx, command, user, reason).await,
        SeasonAction::Close { accept_partial } => close(tx, command, accept_partial).await,
    }
}

/// The row of a season, under the lock of each command.
struct SeasonRow {
    closed: bool,
    ended: bool,
}

#[derive(Clone, Copy)]
enum Lock {
    None,
    Share,
    Update,
}

async fn season_row(
    tx: &mut PgConnection,
    season: i32,
    lock: Lock,
) -> Result<SeasonRow, SeasonFailure> {
    let sql = match lock {
        Lock::None => "SELECT closed_at IS NOT NULL, ends_at <= now() FROM seasons WHERE id = $1",
        Lock::Share => {
            "SELECT closed_at IS NOT NULL, ends_at <= now() FROM seasons WHERE id = $1 FOR SHARE"
        }
        Lock::Update => {
            "SELECT closed_at IS NOT NULL, ends_at <= now() FROM seasons WHERE id = $1 FOR UPDATE"
        }
    };
    let row = sqlx::query(sql)
        .bind(season)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(refused("season.not_found"))?;
    Ok(SeasonRow {
        closed: row.try_get(0)?,
        ended: row.try_get(1)?,
    })
}

/// The fights of a season still pending (020): with `closes_at` in it, and unsettled or settled
/// without a mark in `season_tallies`.
const PENDING_SQL: &str = "\
SELECT count(*) FROM seasons s JOIN fights f \
  ON f.closes_at >= s.starts_at AND f.closes_at < s.ends_at \
WHERE s.id = $1 AND (f.state <> 'settled' \
  OR NOT EXISTS (SELECT 1 FROM season_tallies t WHERE t.fight_id = f.id))";

async fn refuse_pending(tx: &mut PgConnection, season: i32) -> Result<(), SeasonFailure> {
    let pending: i64 = sqlx::query_scalar(PENDING_SQL)
        .bind(season)
        .fetch_one(&mut *tx)
        .await?;
    if pending > 0 {
        return Err(refused("season.pending_fights"));
    }
    Ok(())
}

async fn audit(
    tx: &mut PgConnection,
    command: &SeasonCommand,
    action: &'static str,
    payload: &serde_json::Value,
) -> Result<i64, SeasonFailure> {
    let id = sqlx::query_scalar(
        "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
         VALUES ('cli_attributed', $1, $2, $3, $4::jsonb) RETURNING id",
    )
    .bind(command.by)
    .bind(action)
    .bind(command.season.to_string())
    .bind(payload.to_string())
    .fetch_one(&mut *tx)
    .await?;
    Ok(id)
}

/// The window of `auth_events` of a season and the opaque label of every `ip_hash` that two or
/// more accounts used in it, by the first event of each (`ip_group`). Shared by the queries of one
/// report, in one snapshot, so a label means the same in all its sections. The window starts a day
/// before the season: the bets of a fight that closes in its first minutes were placed in the last
/// ones of the season before.
macro_rules! labels_cte {
    () => {
        "win AS (SELECT starts_at - interval '1 day' AS s, ends_at AS e FROM seasons WHERE id = $1), \
         labels AS (SELECT a.ip_hash, dense_rank() OVER (ORDER BY min(a.id)) AS label \
           FROM auth_events a, win WHERE a.created_at >= win.s AND a.created_at < win.e \
           GROUP BY a.ip_hash HAVING count(DISTINCT a.user_id) >= 2)"
    };
}

/// D8, list 1: the fights of the season where accounts of one `ip_group` bet on opposite sides, by
/// the `ip_hash` of each bet.
const OPPOSITE_SIDES_SQL: &str = concat!(
    "WITH ",
    labels_cte!(),
    " SELECT b.fight_id, l.label, array_agg(DISTINCT b.user_id ORDER BY b.user_id) \
     FROM seasons s JOIN fights f ON f.closes_at >= s.starts_at AND f.closes_at < s.ends_at \
     JOIN bets b ON b.fight_id = f.id \
     JOIN auth_events e ON e.bet_id = b.id AND e.kind = 'bet' \
     JOIN labels l ON l.ip_hash = e.ip_hash \
     WHERE s.id = $1 \
     GROUP BY b.fight_id, l.label \
     HAVING count(DISTINCT b.side) = 2 AND count(DISTINCT b.user_id) >= 2 \
     ORDER BY b.fight_id, l.label"
);

/// D8, list 2: what each loser `j` gave to the `RankedProfit` of `$2`, over its fights of the season
/// with a ranked gain: the gain `p` of each fight, attributed in proportion to `stake_j / L_c`,
/// rounded down. Only stakes with `ranked_stake` count, as in `L_c`.
const ATTRIBUTION_SQL: &str = "\
WITH won AS ( \
  SELECT st.fight_id, sum(st.ranked_profit) AS p \
  FROM seasons s JOIN fights f ON f.closes_at >= s.starts_at AND f.closes_at < s.ends_at \
  JOIN bet_settlements st ON st.fight_id = f.id \
  WHERE s.id = $1 AND st.user_id = $2 AND outcome = 'won' \
  GROUP BY st.fight_id HAVING sum(st.ranked_profit) > 0), \
lost AS ( \
  SELECT st.fight_id, st.user_id, b.stake FROM bet_settlements st \
  JOIN bets b ON b.id = st.bet_id JOIN won w ON w.fight_id = st.fight_id \
  WHERE outcome = 'lost' AND b.ranked_stake), \
lc AS (SELECT fight_id, sum(stake) AS l FROM lost GROUP BY fight_id) \
SELECT lost.user_id, sum(floor(w.p::numeric * lost.stake / lc.l))::bigint AS given \
FROM lost JOIN won w USING (fight_id) JOIN lc USING (fight_id) \
WHERE lost.user_id <> $2 \
GROUP BY lost.user_id ORDER BY given DESC, lost.user_id LIMIT $3";

/// The lowest `ip_group` that `$2` shares with each of `$3` in the season, if any.
const SHARED_GROUP_SQL: &str = concat!(
    "WITH ",
    labels_cte!(),
    " SELECT other.user_id, min(l.label) \
     FROM labels l, win \
     JOIN auth_events mine ON mine.user_id = $2 \
     JOIN auth_events other ON other.user_id = ANY($3) \
     WHERE mine.ip_hash = l.ip_hash AND other.ip_hash = l.ip_hash \
       AND mine.created_at >= win.s AND mine.created_at < win.e \
       AND other.created_at >= win.s AND other.created_at < win.e \
     GROUP BY other.user_id"
);

#[derive(Serialize)]
struct Report {
    season: i32,
    coverage: &'static str,
    fights: i64,
    shared_ip_opposite_sides: Vec<OppositeSides>,
    counterpart_concentration: Concentration,
}

#[derive(Serialize)]
struct OppositeSides {
    fight_id: i32,
    ip_group: i64,
    users: Vec<String>,
}

#[derive(Serialize)]
struct Concentration {
    threshold_bp: i64,
    top: Vec<Reviewed>,
}

#[derive(Serialize)]
struct Reviewed {
    rank: usize,
    user_id: String,
    ranked_profit: i64,
    /// The share of the `RankedProfit` given by the top counterparts, in basis points; `None` when
    /// the `RankedProfit` is not positive and there is no share to speak of.
    concentration_bp: Option<i64>,
    shares_ip_group: bool,
    flagged: bool,
    counterparts: Vec<Counterpart>,
}

#[derive(Serialize)]
struct Counterpart {
    #[serde(skip)]
    id: Uuid,
    user_id: String,
    given: i64,
    ip_group: Option<i64>,
}

/// `top * 10 000 / profit`, rounded down; `None` with a profit that is not positive.
fn share_bp(top: i64, profit: i64) -> Option<i64> {
    (profit > 0).then(|| {
        let share = i128::from(top) * 10_000 / i128::from(profit);
        i64::try_from(share).unwrap_or(i64::MAX)
    })
}

async fn farm_report(
    tx: &mut PgConnection,
    command: &SeasonCommand,
) -> Result<String, SeasonFailure> {
    let season = command.season;
    let row = season_row(tx, season, Lock::None).await?;
    if !row.ended {
        return Err(refused("season.not_ended"));
    }
    refuse_pending(tx, season).await?;
    // The oldest event left after the retention: the season is covered only from it on.
    let covered: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT min(created_at) FROM auth_events) <= s.starts_at, false) \
         FROM seasons s WHERE s.id = $1",
    )
    .bind(season)
    .fetch_one(&mut *tx)
    .await?;
    let coverage = if covered { "full" } else { "partial" };
    let fights: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM seasons s JOIN fights f \
         ON f.closes_at >= s.starts_at AND f.closes_at < s.ends_at WHERE s.id = $1",
    )
    .bind(season)
    .fetch_one(&mut *tx)
    .await?;

    let rows = sqlx::query(OPPOSITE_SIDES_SQL)
        .bind(season)
        .fetch_all(&mut *tx)
        .await?;
    let opposite = rows
        .iter()
        .map(|row| {
            Ok(OppositeSides {
                fight_id: row.try_get(0)?,
                ip_group: row.try_get(1)?,
                users: row
                    .try_get::<Vec<Uuid>, _>(2)?
                    .iter()
                    .map(Uuid::to_string)
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    let mut top = Vec::new();
    for (i, user) in board::ranked(tx, season, REVIEWED_TOP)
        .await?
        .into_iter()
        .enumerate()
    {
        let rows = sqlx::query(ATTRIBUTION_SQL)
            .bind(season)
            .bind(user.user_id)
            .bind(i64::try_from(TOP_COUNTERPARTS).unwrap_or(i64::MAX))
            .fetch_all(&mut *tx)
            .await?;
        let mut counterparts = rows
            .iter()
            .map(|row| {
                let id: Uuid = row.try_get(0)?;
                Ok(Counterpart {
                    id,
                    user_id: id.to_string(),
                    given: row.try_get(1)?,
                    ip_group: None,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let others: Vec<Uuid> = counterparts.iter().map(|c| c.id).collect();
        let shared = sqlx::query(SHARED_GROUP_SQL)
            .bind(season)
            .bind(user.user_id)
            .bind(&others)
            .fetch_all(&mut *tx)
            .await?;
        for row in &shared {
            let other: Uuid = row.try_get(0)?;
            let group: i64 = row.try_get(1)?;
            if let Some(c) = counterparts.iter_mut().find(|c| c.id == other) {
                c.ip_group = Some(group);
            }
        }
        let given: i64 = counterparts.iter().map(|c| c.given).sum();
        let concentration_bp = share_bp(given, user.ranked_profit);
        let shares_ip_group = counterparts.iter().any(|c| c.ip_group.is_some());
        let flagged =
            shares_ip_group || concentration_bp.is_some_and(|bp| bp > CONCENTRATION_THRESHOLD_BP);
        top.push(Reviewed {
            rank: i + 1,
            user_id: user.user_id.to_string(),
            ranked_profit: user.ranked_profit,
            concentration_bp,
            shares_ip_group,
            flagged,
            counterparts,
        });
    }

    let flagged: Vec<&str> = top
        .iter()
        .filter(|r| r.flagged)
        .map(|r| r.user_id.as_str())
        .collect();
    let payload = serde_json::json!({
        "coverage": coverage,
        "fights": fights,
        "shared_ip_opposite_sides": opposite.len(),
        "reviewed": top.len(),
        "flagged": flagged,
    });
    audit(tx, command, "season_review", &payload).await?;
    let report = Report {
        season,
        coverage,
        fights,
        shared_ip_opposite_sides: opposite,
        counterpart_concentration: Concentration {
            threshold_bp: CONCENTRATION_THRESHOLD_BP,
            top,
        },
    };
    serde_json::to_string(&report).map_err(|_| SeasonFailure::Failed("serialize"))
}

async fn exclude(
    tx: &mut PgConnection,
    command: &SeasonCommand,
    user: Uuid,
    reason: ExcludeReason,
) -> Result<String, SeasonFailure> {
    // `FOR SHARE`: a `close` under way finishes first, and this one then sees it closed.
    let row = season_row(tx, command.season, Lock::Share).await?;
    if row.closed {
        return Err(refused("season.closed"));
    }
    let excluded: Option<bool> = sqlx::query_scalar(
        "SELECT excluded_at IS NOT NULL FROM season_stats \
         WHERE season_id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(command.season)
    .bind(user)
    .fetch_optional(&mut *tx)
    .await?;
    match excluded {
        None => return Err(refused("season.no_stats")),
        // The first exclusion stays as it was: its date, its admin and its reason.
        Some(true) => return Err(refused("season.already_excluded")),
        Some(false) => {}
    }
    sqlx::query(
        "UPDATE season_stats SET excluded_at = now(), excluded_by = $3, excluded_reason = $4 \
         WHERE season_id = $1 AND user_id = $2",
    )
    .bind(command.season)
    .bind(user)
    .bind(command.by)
    .bind(reason.as_str())
    .execute(&mut *tx)
    .await?;
    let payload = serde_json::json!({ "user_id": user.to_string(), "reason": reason.as_str() });
    audit(tx, command, "season_exclude", &payload).await?;
    Ok("season.exclude_applied".to_owned())
}

/// The settled fights of a season without a mark, for `close` to sum under its lock.
const UNTALLIED_SQL: &str = "\
SELECT f.id FROM seasons s JOIN fights f \
  ON f.closes_at >= s.starts_at AND f.closes_at < s.ends_at \
WHERE s.id = $1 AND f.state = 'settled' \
  AND NOT EXISTS (SELECT 1 FROM season_tallies t WHERE t.fight_id = f.id) \
ORDER BY f.id";

/// The newest review that `close` can use: of this season, since its end and of the last 48 h.
const REVIEW_SQL: &str = "\
SELECT a.id, a.payload->>'coverage' FROM admin_audit a JOIN seasons s ON s.id = $1 \
WHERE a.action = 'season_review' AND a.target = $1::text \
  AND a.at >= s.ends_at AND a.at >= now() - interval '48 hours' \
ORDER BY a.at DESC, a.id DESC LIMIT 1";

async fn close(
    tx: &mut PgConnection,
    command: &SeasonCommand,
    accept_partial: bool,
) -> Result<String, SeasonFailure> {
    let season = command.season;
    // The lock of the season comes first, before any of `season_stats`: a settlement takes the same
    // row (`FOR KEY SHARE`, by the foreign key of `season_tallies`) before its stats too, so the two
    // never wait on each other in a cycle.
    let row = season_row(tx, season, Lock::Update).await?;
    if row.closed {
        return Err(refused("season.already_closed"));
    }
    if !row.ended {
        return Err(refused("season.not_ended"));
    }
    let untallied: Vec<i32> = sqlx::query_scalar(UNTALLIED_SQL)
        .bind(season)
        .fetch_all(&mut *tx)
        .await?;
    let mut tallied = 0_u64;
    for fight in untallied {
        if seasons::tally(&mut *tx, fight).await? {
            tallied += 1;
        }
    }
    refuse_pending(tx, season).await?;
    let review = sqlx::query(REVIEW_SQL)
        .bind(season)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(refused("season.no_review"))?;
    let review_id: i64 = review.try_get(0)?;
    let coverage: Option<String> = review.try_get(1)?;
    let partial = coverage.as_deref() != Some("full");
    if partial && !accept_partial {
        return Err(refused("season.partial_review"));
    }
    let closed =
        sqlx::query("UPDATE seasons SET closed_at = now() WHERE id = $1 AND closed_at IS NULL")
            .bind(season)
            .execute(&mut *tx)
            .await?;
    if closed.rows_affected() == 0 {
        return Err(refused("season.already_closed"));
    }
    let payload = serde_json::json!({
        "review_id": review_id,
        "coverage": if partial { "partial" } else { "full" },
        "accept_partial": accept_partial,
        "tallied": tallied,
    });
    audit(tx, command, "season_close", &payload).await?;
    Ok("season.close_applied".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_share_rounds_down_and_needs_a_profit() {
        assert_eq!(share_bp(1, 3), Some(3333));
        assert_eq!(share_bp(50, 100), Some(5000));
        assert_eq!(share_bp(0, 100), Some(0));
        assert_eq!(share_bp(10, 0), None);
        assert_eq!(share_bp(10, -5), None);
        assert_eq!(share_bp(i64::MAX, 1), Some(i64::MAX));
    }
}
