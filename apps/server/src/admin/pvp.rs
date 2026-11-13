//! `server pvp farm-report|exclude` (feature 021, phase 4, TA-64): the review of a closed PvP season
//! for farms of rating, and the exclusion of a user from a PvP leaderboard. The same rules as
//! `server season` (020, [`super::season`]): `--by` is an admin who is not banned, both write
//! `admin_audit` as `cli_attributed` with `torneo.actor = 'cli'`, each refusal prints its code and
//! writes nothing, and the logs carry only `event` and `error_kind` (ADR-0012 A8). There is no
//! endpoint: `cli.rs` is the only caller.
//!
//! `farm-report` reads one snapshot of the season and prints the signals of 021 "Números". They are
//! signals, never proof: the sanction stays manual (`exclude`). Users are named by their id, as in
//! 020, and the accounts that share an `ip_hash` by opaque labels (`ip_group: 1, 2…`): the values of
//! `ip_hash` never leave Postgres.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::io::Write as _;
use std::time::Duration;

use serde::Serialize;
use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

use super::season::{
    CONCENTRATION_THRESHOLD_BP, ExcludeReason, REVIEWED_TOP, SEASON_DEADLINE,
    SEASON_LOCK_TIMEOUT_MS, SeasonFailure, TOP_COUNTERPARTS,
};
use super::{CONNECT_TIMEOUT, parse_canonical_uuid};
use crate::fights::store;
use crate::seasons::api::parse_season;

/// Repeated pair: this many fights between the same two accounts in the season…
pub const PAIR_FIGHTS: usize = 10;
/// …or one defender is at least this share of the attacks of an attacker against users, in basis
/// points, once the attacker has [`PAIR_MIN_ATTACKS`] of them.
pub const PAIR_SHARE_BP: i64 = 5000;
pub const PAIR_MIN_ATTACKS: usize = 10;
/// Punching bag: never attacks, and loses this share of at least [`BAG_MIN_DEFENSES`] defenses.
pub const BAG_MIN_DEFENSES: usize = 8;
pub const BAG_LOSS_BP: i64 = 9000;
/// New one-way account: younger than 7 days at the end of the season or with fewer than
/// [`NEW_MIN_FIGHTS`] fights in it, it loses [`NEW_LOSS_BP`] against one account (at least
/// [`NEW_MIN_AGAINST`] fights) and wins [`NEW_REST_WIN_BP`] against the rest (as many at least).
pub const NEW_DAYS: i32 = 7;
pub const NEW_MIN_FIGHTS: usize = 30;
pub const NEW_MIN_AGAINST: usize = 5;
pub const NEW_LOSS_BP: i64 = 9000;
pub const NEW_REST_WIN_BP: i64 = 4000;
/// Sink and rebound: falls more than [`REBOUND_DROP`] in [`REBOUND_WINDOW`] rated fights and then
/// wins [`REBOUND_WIN_BP`] of the next as many.
pub const REBOUND_WINDOW: usize = 30;
pub const REBOUND_DROP: i32 = 200;
pub const REBOUND_WIN_BP: i64 = 8000;
/// Streak against the tournament (informative): at least this share of the attacks of the season,
/// from [`TOURNAMENT_MIN_ATTACKS`] attacks on.
pub const TOURNAMENT_SHARE_BP: i64 = 5000;
pub const TOURNAMENT_MIN_ATTACKS: usize = 10;

/// What a `server pvp` command does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PvpAction {
    FarmReport,
    Exclude { user: Uuid, reason: ExcludeReason },
}

/// `server pvp …`. `Debug` shows neither the season, nor the users, nor the admin (ADR-0012 A8).
#[derive(Clone, PartialEq, Eq)]
pub struct PvpCommand {
    pub season: i32,
    pub by: Uuid,
    pub action: PvpAction,
}

impl fmt::Debug for PvpCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let action = match self.action {
            PvpAction::FarmReport => "FarmReport",
            PvpAction::Exclude { .. } => "Exclude",
        };
        f.debug_struct("PvpCommand")
            .field("action", &action)
            .finish_non_exhaustive()
    }
}

impl PvpCommand {
    /// The arguments after `pvp`: `farm-report <season> --by <admin>` or `exclude <season> <user>
    /// --by <admin> --reason <reason>`, the flags in any order and none repeated.
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
        let (mut by, mut reason) = (None, None);
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
                _ => return None,
            }
        }
        let action = match *action {
            "farm-report" => PvpAction::FarmReport,
            "exclude" => PvpAction::Exclude {
                user: user?,
                reason: reason?,
            },
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
            PvpAction::FarmReport => "farm_report",
            PvpAction::Exclude { .. } => "exclude",
        }
    }
}

const fn refused(code: &'static str) -> SeasonFailure {
    SeasonFailure::Refused(code)
}

/// `server pvp …`: runs `command` in one transaction, prints what it says (the report, or one code)
/// and logs its event and nothing else. `cli.rs` is the only caller.
///
/// # Errors
/// [`SeasonFailure`].
pub async fn run_pvp(
    options: &PgConnectOptions,
    command: &PvpCommand,
) -> Result<(), SeasonFailure> {
    let outcome = execute_pvp(options, command, SEASON_DEADLINE).await;
    let mut stdout = std::io::stdout().lock();
    let name = command.name();
    match &outcome {
        Ok(text) => {
            let _ = writeln!(stdout, "{text}");
            tracing::info!(event = "pvp.admin_applied", command = name);
        }
        Err(SeasonFailure::Refused(code)) => {
            let _ = writeln!(stdout, "{code}");
            tracing::error!(
                event = "pvp.admin_refused",
                command = name,
                error_kind = *code
            );
        }
        Err(SeasonFailure::Failed(kind)) => {
            tracing::error!(
                event = "pvp.admin_failed",
                command = name,
                error_kind = *kind
            );
        }
    }
    outcome.map(|_| ())
}

/// [`run_pvp`] without the printing and the log, and with its deadline: what the tests call.
///
/// # Errors
/// [`SeasonFailure`].
pub async fn execute_pvp(
    options: &PgConnectOptions,
    command: &PvpCommand,
    deadline: Duration,
) -> Result<String, SeasonFailure> {
    tokio::time::timeout(deadline, apply(options, command))
        .await
        .unwrap_or(Err(SeasonFailure::Failed("timeout")))
}

async fn apply(options: &PgConnectOptions, command: &PvpCommand) -> Result<String, SeasonFailure> {
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
            let _ = tx.rollback().await;
            let _ = connection.close().await;
            return Err(failure);
        }
    };
    let _ = connection.close().await;
    Ok(text)
}

async fn write(tx: &mut PgConnection, command: &PvpCommand) -> Result<String, SeasonFailure> {
    if command.action == PvpAction::FarmReport {
        // One snapshot for the whole report. First statement.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("SET LOCAL torneo.actor = 'cli'")
        .execute(&mut *tx)
        .await?;
    store::set_lock_timeout(&mut *tx, SEASON_LOCK_TIMEOUT_MS).await?;
    let admin = sqlx::query(
        "SELECT 1 FROM users WHERE id = $1 AND role = 'admin' AND banned_at IS NULL FOR SHARE",
    )
    .bind(command.by)
    .fetch_optional(&mut *tx)
    .await?;
    if admin.is_none() {
        return Err(refused("pvp.not_admin"));
    }
    match command.action {
        PvpAction::FarmReport => farm_report(tx, command).await,
        PvpAction::Exclude { user, reason } => exclude(tx, command, user, reason).await,
    }
}

async fn audit(
    tx: &mut PgConnection,
    command: &PvpCommand,
    action: &'static str,
    payload: &serde_json::Value,
) -> Result<(), SeasonFailure> {
    sqlx::query(
        "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
         VALUES ('cli_attributed', $1, $2, $3, $4::jsonb)",
    )
    .bind(command.by)
    .bind(action)
    .bind(command.season.to_string())
    .bind(payload.to_string())
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn exclude(
    tx: &mut PgConnection,
    command: &PvpCommand,
    user: Uuid,
    reason: ExcludeReason,
) -> Result<String, SeasonFailure> {
    // The current season or a closed one: the leaderboards that exist.
    let exists: bool = sqlx::query_scalar(
        "SELECT $1 = (extract(year FROM now() AT TIME ZONE 'UTC') * 100 \
                      + extract(month FROM now() AT TIME ZONE 'UTC'))::int \
             OR EXISTS (SELECT 1 FROM pvp_seasons WHERE id = $1)",
    )
    .bind(command.season)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(refused("pvp.season_not_found"));
    }
    let botarga = sqlx::query("SELECT 1 FROM botargas WHERE user_id = $1")
        .bind(user)
        .fetch_optional(&mut *tx)
        .await?;
    if botarga.is_none() {
        return Err(refused("pvp.no_botarga"));
    }
    // The first exclusion stays as it was: its date, its admin and its reason.
    let inserted = sqlx::query(
        "INSERT INTO pvp_exclusions (season_id, user_id, excluded_by, reason) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (season_id, user_id) DO NOTHING",
    )
    .bind(command.season)
    .bind(user)
    .bind(command.by)
    .bind(reason.as_str())
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(refused("pvp.already_excluded"));
    }
    let payload = serde_json::json!({ "user_id": user.to_string(), "reason": reason.as_str() });
    audit(tx, command, "pvp_exclude", &payload).await?;
    Ok("pvp.exclude_applied".to_owned())
}

/// Who won a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Attacker,
    Defender,
    Draw,
}

/// One fight of the season, as the signals read it. `defender` is `None` against the tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeasonFight {
    pub attacker: Uuid,
    pub defender: Option<Uuid>,
    pub result: Outcome,
    /// The change of each side, as applied (each side has its own K).
    pub rating_delta: i32,
    pub defender_rating_delta: i32,
    pub rated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepeatedPair {
    /// The two accounts, in the order of their ids.
    pub users: [String; 2],
    pub fights: usize,
    /// The largest share, in basis points, that one of the two is of the attacks of the other
    /// against users; `None` when neither has [`PAIR_MIN_ATTACKS`].
    pub attacker_share_bp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PunchingBag {
    pub user_id: String,
    pub defenses: usize,
    pub losses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OneWay {
    pub user_id: String,
    pub counterpart: String,
    pub lost_bp: i64,
    pub rest_won_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rebound {
    pub user_id: String,
    /// The rated fight of the user (from 1, in order) where the fall starts.
    pub from_fight: usize,
    pub drop: i32,
    pub rebound_won_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TournamentStreak {
    pub user_id: String,
    pub attacks: usize,
    pub tournament: usize,
}

/// The signals that read only the fights (and which accounts are new).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Signals {
    pub repeated_pairs: Vec<RepeatedPair>,
    pub punching_bags: Vec<PunchingBag>,
    pub one_way_new: Vec<OneWay>,
    pub sink_and_rebound: Vec<Rebound>,
    pub tournament_streaks: Vec<TournamentStreak>,
}

/// `part · 10 000 / whole`, rounded down; 0 for an empty whole.
fn bp(part: usize, whole: usize) -> i64 {
    if whole == 0 {
        return 0;
    }
    let share = u128::try_from(part).unwrap_or(u128::MAX) * 10_000
        / u128::try_from(whole).unwrap_or(u128::MAX);
    i64::try_from(share).unwrap_or(i64::MAX)
}

/// The record of one account: per counterpart, (fights, won, lost), from its own side.
#[derive(Default)]
struct Record {
    attacks: usize,
    tournament: usize,
    defenses: usize,
    defenses_lost: usize,
    against: BTreeMap<Uuid, (usize, usize, usize)>,
    /// Its own change of rating in each rated fight, and whether it won it, in order.
    rated: Vec<(i32, bool)>,
}

/// The signals of 021 "Números" over the fights of a season, in the order of their ids. `new`: the
/// accounts younger than [`NEW_DAYS`] at the end of the season. Deterministic: same input, same
/// report.
#[must_use]
pub fn signals(fights: &[SeasonFight], new: &BTreeSet<Uuid>) -> Signals {
    let mut records: BTreeMap<Uuid, Record> = BTreeMap::new();
    let mut pairs: BTreeMap<(Uuid, Uuid), usize> = BTreeMap::new();
    // The attacks of each attacker against each defender, in one pass (never a scan per pair).
    let mut directed: BTreeMap<(Uuid, Uuid), usize> = BTreeMap::new();
    for f in fights {
        let attacker = records.entry(f.attacker).or_default();
        attacker.attacks += 1;
        let Some(defender_id) = f.defender else {
            attacker.tournament += 1;
            continue;
        };
        let (a_won, d_won) = match f.result {
            Outcome::Attacker => (true, false),
            Outcome::Defender => (false, true),
            Outcome::Draw => (false, false),
        };
        let entry = attacker.against.entry(defender_id).or_default();
        entry.0 += 1;
        entry.1 += usize::from(a_won);
        entry.2 += usize::from(d_won);
        if f.rated {
            attacker.rated.push((f.rating_delta, a_won));
        }
        let defender = records.entry(defender_id).or_default();
        defender.defenses += 1;
        defender.defenses_lost += usize::from(a_won);
        let entry = defender.against.entry(f.attacker).or_default();
        entry.0 += 1;
        entry.1 += usize::from(d_won);
        entry.2 += usize::from(a_won);
        if f.rated {
            defender.rated.push((f.defender_rating_delta, d_won));
        }
        *directed.entry((f.attacker, defender_id)).or_default() += 1;
        let key = if f.attacker < defender_id {
            (f.attacker, defender_id)
        } else {
            (defender_id, f.attacker)
        };
        *pairs.entry(key).or_default() += 1;
    }
    let mut out = Signals::default();

    // The share of an attacker's attacks against users that went to one defender.
    let share = |attacker: &Uuid, defender: &Uuid| -> Option<i64> {
        let r = records.get(attacker)?;
        let against_users = r.attacks - r.tournament;
        if against_users < PAIR_MIN_ATTACKS {
            return None;
        }
        let toward = directed.get(&(*attacker, *defender)).copied().unwrap_or(0);
        Some(bp(toward, against_users))
    };
    for ((x, y), count) in &pairs {
        let attacker_share_bp = share(x, y).max(share(y, x));
        if *count >= PAIR_FIGHTS || attacker_share_bp.is_some_and(|s| s >= PAIR_SHARE_BP) {
            out.repeated_pairs.push(RepeatedPair {
                users: [x.to_string(), y.to_string()],
                fights: *count,
                attacker_share_bp,
            });
        }
    }

    for (user, r) in &records {
        if r.attacks == 0
            && r.defenses >= BAG_MIN_DEFENSES
            && bp(r.defenses_lost, r.defenses) >= BAG_LOSS_BP
        {
            out.punching_bags.push(PunchingBag {
                user_id: user.to_string(),
                defenses: r.defenses,
                losses: r.defenses_lost,
            });
        }

        let total: usize = r.against.values().map(|(n, _, _)| n).sum();
        if new.contains(user) || total < NEW_MIN_FIGHTS {
            for (other, (n, _, lost)) in &r.against {
                if *n < NEW_MIN_AGAINST || bp(*lost, *n) < NEW_LOSS_BP {
                    continue;
                }
                let (rest_n, rest_won) = r
                    .against
                    .iter()
                    .filter(|(id, _)| *id != other)
                    .fold((0, 0), |(n, w), (_, (fn_, fw, _))| (n + fn_, w + fw));
                if rest_n >= NEW_MIN_AGAINST && bp(rest_won, rest_n) >= NEW_REST_WIN_BP {
                    out.one_way_new.push(OneWay {
                        user_id: user.to_string(),
                        counterpart: other.to_string(),
                        lost_bp: bp(*lost, *n),
                        rest_won_bp: bp(rest_won, rest_n),
                    });
                }
            }
        }

        if let Some(rebound) = rebound(&r.rated) {
            out.sink_and_rebound.push(Rebound {
                user_id: user.to_string(),
                ..rebound
            });
        }

        if r.attacks >= TOURNAMENT_MIN_ATTACKS && bp(r.tournament, r.attacks) >= TOURNAMENT_SHARE_BP
        {
            out.tournament_streaks.push(TournamentStreak {
                user_id: user.to_string(),
                attacks: r.attacks,
                tournament: r.tournament,
            });
        }
    }
    out
}

/// The first window of [`REBOUND_WINDOW`] rated fights that falls more than [`REBOUND_DROP`] and is
/// followed by [`REBOUND_WINDOW`] fights with [`REBOUND_WIN_BP`] won. `user_id` is left empty.
fn rebound(rated: &[(i32, bool)]) -> Option<Rebound> {
    let w = REBOUND_WINDOW;
    if rated.len() < 2 * w {
        return None;
    }
    (0..=rated.len() - 2 * w).find_map(|start| {
        let drop: i32 = rated[start..start + w].iter().map(|(d, _)| d).sum();
        let won = rated[start + w..start + 2 * w]
            .iter()
            .filter(|(_, won)| *won)
            .count();
        (drop < -REBOUND_DROP && bp(won, w) >= REBOUND_WIN_BP).then(|| Rebound {
            user_id: String::new(),
            from_fight: start + 1,
            drop: -drop,
            rebound_won_bp: bp(won, w),
        })
    })
}

/// The window of `auth_events` of a closed PvP season and the opaque label of every `ip_hash` that
/// two or more accounts used in it, by the first event of each. As in 020, from a day before the
/// season.
macro_rules! labels_cte {
    () => {
        "win AS (SELECT starts_at - interval '1 day' AS s, ends_at AS e FROM pvp_seasons \
           WHERE id = $1), \
         labels AS (SELECT a.ip_hash, dense_rank() OVER (ORDER BY min(a.id)) AS label \
           FROM auth_events a, win WHERE a.created_at >= win.s AND a.created_at < win.e \
           GROUP BY a.ip_hash HAVING count(DISTINCT a.user_id) >= 2)"
    };
}

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

/// The first of the leaderboard of a closed season, in its order (as `GET /api/pvp/leaderboard`).
const TOP_SQL: &str = "\
SELECT r.user_id, r.rating FROM pvp_season_ratings r JOIN users u ON u.id = r.user_id \
WHERE r.season_id = $1 AND r.rated_fights >= $2 AND u.banned_at IS NULL \
  AND NOT EXISTS (SELECT 1 FROM pvp_exclusions x WHERE x.season_id = $1 AND x.user_id = r.user_id) \
ORDER BY r.rating DESC, r.rated_fights DESC, r.user_id LIMIT $3";

#[derive(Serialize)]
struct Report {
    season: i32,
    coverage: &'static str,
    fights: usize,
    #[serde(flatten)]
    signals: Signals,
    counterpart_concentration: Concentration,
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
    rating: i32,
    /// The rating won in the season: the sum of its positive changes.
    gained: i64,
    /// The share of `gained` that came from the top counterparts, in basis points; `None` without
    /// a gain.
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
    gained: i64,
    ip_group: Option<i64>,
}

/// What `user` won from each counterpart in the rated fights: the sum of its positive changes.
fn gained_from(fights: &[SeasonFight], user: Uuid) -> BTreeMap<Uuid, i64> {
    let mut out = BTreeMap::new();
    for f in fights.iter().filter(|f| f.rated) {
        let Some(defender) = f.defender else {
            continue;
        };
        let (own, other) = if f.attacker == user {
            (f.rating_delta, defender)
        } else if defender == user {
            (f.defender_rating_delta, f.attacker)
        } else {
            continue;
        };
        if own > 0 {
            *out.entry(other).or_default() += i64::from(own);
        }
    }
    out
}

async fn farm_report(tx: &mut PgConnection, command: &PvpCommand) -> Result<String, SeasonFailure> {
    let season = command.season;
    // Only a closed season: its fights are all in, and its leaderboard is a snapshot.
    let closed = sqlx::query("SELECT 1 FROM pvp_seasons WHERE id = $1")
        .bind(season)
        .fetch_optional(&mut *tx)
        .await?;
    if closed.is_none() {
        return Err(refused("pvp.season_not_closed"));
    }
    let covered: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT min(created_at) FROM auth_events) \
                         <= s.starts_at - interval '1 day', false) \
         FROM pvp_seasons s WHERE s.id = $1",
    )
    .bind(season)
    .fetch_one(&mut *tx)
    .await?;
    let coverage = if covered { "full" } else { "partial" };

    let rows = sqlx::query(
        "SELECT attacker_id, defender_id, winner, rating_delta::int, rated, \
         defender_rating_delta::int FROM pvp_fights WHERE season_id = $1 ORDER BY id",
    )
    .bind(season)
    .fetch_all(&mut *tx)
    .await?;
    let fights = rows
        .iter()
        .map(|row| {
            let winner: Option<String> = row.try_get(2)?;
            Ok(SeasonFight {
                attacker: row.try_get(0)?,
                defender: row.try_get(1)?,
                result: match winner.as_deref() {
                    Some("a") => Outcome::Attacker,
                    Some(_) => Outcome::Defender,
                    None => Outcome::Draw,
                },
                rating_delta: row.try_get(3)?,
                defender_rating_delta: row.try_get(5)?,
                rated: row.try_get(4)?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let new: BTreeSet<Uuid> = sqlx::query_scalar(
        "SELECT u.id FROM users u, pvp_seasons s WHERE s.id = $1 \
         AND u.created_at > s.ends_at - make_interval(days => $2) \
         AND EXISTS (SELECT 1 FROM pvp_fights f WHERE f.season_id = $1 \
                     AND (f.attacker_id = u.id OR f.defender_id = u.id))",
    )
    .bind(season)
    .bind(NEW_DAYS)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let signals = signals(&fights, &new);

    let top_rows = sqlx::query(TOP_SQL)
        .bind(season)
        .bind(crate::seasons::SEASON_MIN_FIGHTS)
        .bind(i64::try_from(REVIEWED_TOP).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await?;
    let mut top = Vec::new();
    for (i, row) in top_rows.iter().enumerate() {
        let user: Uuid = row.try_get(0)?;
        let rating: i32 = row.try_get(1)?;
        let from = gained_from(&fights, user);
        let gained: i64 = from.values().sum();
        let mut ranked: Vec<(Uuid, i64)> = from.into_iter().collect();
        // The most first; among equals, by id.
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.truncate(TOP_COUNTERPARTS);
        let mut counterparts: Vec<Counterpart> = ranked
            .into_iter()
            .map(|(id, gained)| Counterpart {
                id,
                user_id: id.to_string(),
                gained,
                ip_group: None,
            })
            .collect();
        let others: Vec<Uuid> = counterparts.iter().map(|c| c.id).collect();
        let shared = sqlx::query(SHARED_GROUP_SQL)
            .bind(season)
            .bind(user)
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
        let given: i64 = counterparts.iter().map(|c| c.gained).sum();
        let concentration_bp = (gained > 0).then(|| {
            i64::try_from(i128::from(given) * 10_000 / i128::from(gained)).unwrap_or(i64::MAX)
        });
        let shares_ip_group = counterparts.iter().any(|c| c.ip_group.is_some());
        let flagged =
            shares_ip_group || concentration_bp.is_some_and(|bp| bp > CONCENTRATION_THRESHOLD_BP);
        top.push(Reviewed {
            rank: i + 1,
            user_id: user.to_string(),
            rating,
            gained,
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
        "fights": fights.len(),
        "repeated_pairs": signals.repeated_pairs.len(),
        "punching_bags": signals.punching_bags.len(),
        "one_way_new": signals.one_way_new.len(),
        "sink_and_rebound": signals.sink_and_rebound.len(),
        "tournament_streaks": signals.tournament_streaks.len(),
        "reviewed": top.len(),
        "flagged": flagged,
    });
    audit(tx, command, "pvp_review", &payload).await?;
    let report = Report {
        season,
        coverage,
        fights: fights.len(),
        signals,
        counterpart_concentration: Concentration {
            threshold_bp: CONCENTRATION_THRESHOLD_BP,
            top,
        },
    };
    serde_json::to_string(&report).map_err(|_| SeasonFailure::Failed("serialize"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn id(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn fight(attacker: u8, defender: Option<u8>, outcome: Outcome, delta: i32) -> SeasonFight {
        SeasonFight {
            attacker: id(attacker),
            defender: defender.map(id),
            result: outcome,
            rating_delta: delta,
            defender_rating_delta: -delta,
            rated: defender.is_some(),
        }
    }

    #[test]
    fn a_quiet_season_has_no_signal() {
        let fights: Vec<SeasonFight> = (0..6)
            .map(|i| fight(1 + i % 3, Some(4 + i % 2), Outcome::Attacker, 10))
            .collect();
        assert_eq!(signals(&fights, &BTreeSet::new()), Signals::default());
        assert_eq!(signals(&[], &BTreeSet::new()), Signals::default());
    }

    #[test]
    fn a_pair_that_fights_in_a_loop_is_repeated() {
        // Ten fights of 1 against 2: by the count and by the share.
        let fights: Vec<SeasonFight> = (0..10)
            .map(|_| fight(1, Some(2), Outcome::Attacker, 10))
            .collect();
        let s = signals(&fights, &BTreeSet::new());
        assert_eq!(s.repeated_pairs.len(), 1);
        assert_eq!(s.repeated_pairs[0].fights, 10);
        assert_eq!(s.repeated_pairs[0].attacker_share_bp, Some(10_000));
        // Nine fights: only the share of an attacker with ten attacks against users counts.
        let mut fights: Vec<SeasonFight> = (0..5)
            .map(|_| fight(1, Some(2), Outcome::Attacker, 10))
            .collect();
        fights.extend((0..5).map(|i| fight(1, Some(10 + i), Outcome::Attacker, 10)));
        let s = signals(&fights, &BTreeSet::new());
        assert_eq!(s.repeated_pairs.len(), 1);
        assert_eq!(s.repeated_pairs[0].attacker_share_bp, Some(5000));
        fights.pop();
        assert!(signals(&fights, &BTreeSet::new()).repeated_pairs.is_empty());
    }

    #[test]
    fn a_punching_bag_never_attacks_and_loses_its_defenses() {
        let mut fights: Vec<SeasonFight> = (0..8)
            .map(|i| fight(1 + i, Some(20), Outcome::Attacker, 10))
            .collect();
        let s = signals(&fights, &BTreeSet::new());
        assert_eq!(
            s.punching_bags,
            [PunchingBag {
                user_id: id(20).to_string(),
                defenses: 8,
                losses: 8
            }]
        );
        // One attack of its own takes it out.
        fights.push(fight(20, None, Outcome::Defender, 0));
        assert!(signals(&fights, &BTreeSet::new()).punching_bags.is_empty());
    }

    #[test]
    fn a_new_account_that_only_loses_to_one_is_one_way() {
        // 30 loses five times to 1, and wins five of six against the rest.
        let mut fights: Vec<SeasonFight> = (0..5)
            .map(|_| fight(30, Some(1), Outcome::Defender, -10))
            .collect();
        fights.extend((0..6).map(|i| {
            let outcome = if i == 0 {
                Outcome::Defender
            } else {
                Outcome::Attacker
            };
            fight(30, Some(40 + i), outcome, 10)
        }));
        let s = signals(&fights, &BTreeSet::new());
        assert_eq!(s.one_way_new.len(), 1);
        assert_eq!(s.one_way_new[0].user_id, id(30).to_string());
        assert_eq!(s.one_way_new[0].counterpart, id(1).to_string());
        assert_eq!(s.one_way_new[0].lost_bp, 10_000);
        assert_eq!(s.one_way_new[0].rest_won_bp, 8333);
    }

    #[test]
    fn a_fall_and_a_rebound_are_found() {
        // 30 defeats of −10 (−300), then 30 victories.
        let mut fights: Vec<SeasonFight> = (0..30)
            .map(|_| fight(5, Some(6), Outcome::Defender, -10))
            .collect();
        fights.extend((0..30).map(|i| fight(5, Some(7 + i % 5), Outcome::Attacker, 10)));
        let s = signals(&fights, &BTreeSet::new());
        let rebound = s
            .sink_and_rebound
            .iter()
            .find(|r| r.user_id == id(5).to_string())
            .unwrap();
        assert_eq!(rebound.from_fight, 1);
        assert_eq!(rebound.drop, 300);
        assert_eq!(rebound.rebound_won_bp, 10_000);
        // A fall of exactly 200 is not more than 200.
        let fights: Vec<SeasonFight> = (0..60)
            .map(|i| {
                if i < 30 {
                    fight(5, Some(6), Outcome::Defender, if i < 20 { -10 } else { 0 })
                } else {
                    fight(5, Some(7), Outcome::Attacker, 10)
                }
            })
            .collect();
        assert!(
            signals(&fights, &BTreeSet::new())
                .sink_and_rebound
                .iter()
                .all(|r| r.user_id != id(5).to_string())
        );
    }

    #[test]
    fn half_the_attacks_against_the_tournament_is_a_streak() {
        let mut fights: Vec<SeasonFight> = (0..5)
            .map(|_| fight(9, None, Outcome::Attacker, 0))
            .collect();
        fights.extend((0..5).map(|i| fight(9, Some(50 + i), Outcome::Attacker, 10)));
        let s = signals(&fights, &BTreeSet::new());
        assert_eq!(
            s.tournament_streaks,
            [TournamentStreak {
                user_id: id(9).to_string(),
                attacks: 10,
                tournament: 5
            }]
        );
    }

    #[test]
    fn the_gain_counts_only_what_was_won() {
        let fights = [
            fight(1, Some(2), Outcome::Attacker, 20),
            fight(2, Some(1), Outcome::Attacker, 15),
            fight(3, Some(1), Outcome::Defender, -8),
            fight(1, None, Outcome::Attacker, 0),
        ];
        let from = gained_from(&fights, id(1));
        assert_eq!(from.get(&id(2)), Some(&20));
        assert_eq!(from.get(&id(3)), Some(&8));
        assert_eq!(from.len(), 2);
    }
}
