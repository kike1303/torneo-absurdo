//! Fights for the tests of 010, on the route the database admits (worry 1 of the spec): a row with
//! the specs and the seed the test chooses, and the runner with a manual clock from there. So the
//! tests force a KO, a decision or a draw with the golden vectors of the engine, without a line of
//! test code in the binary.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::{CharacterSpec, FightEvent, FightLog, Ruleset, Seed, canonical_spec_json, simulate};
use protocol::FightState;
use server::app::{AppState, Settings};
use server::fights::release::EventSink;
use server::fights::snapshot::{max_duration_ms, max_hp, snapshot_sha256};
use server::fights::state::FightRow;
use server::fights::store::{self, NewFight};
use server::fights::{FightClock, FightSettings, FightsConfig, FightsShared, RunnerTimings};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::app::{AlwaysUp, RunningApp};
use super::db::TestDb;

pub const CADENCE: i64 = 600_000;
pub const WINDOW: i64 = 450_000;
pub const PRE_LIVE: i64 = 30_000;
/// A multiple of [`CADENCE`], far from today.
pub const T0: i64 = 1_800_000_000_000;

/// The root of the repo.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The real characters, and the default schedule.
pub fn config() -> FightsConfig {
    FightsConfig {
        cadence_ms: CADENCE,
        window_ms: WINDOW,
        pre_live_ms: PRE_LIVE,
        content_dir: repo_root().join("content/characters"),
    }
}

/// A directory that does not exist: `fights.roster_invalid` and no new rows, so a test sees only
/// its own fights.
pub fn config_without_roster() -> FightsConfig {
    FightsConfig {
        content_dir: PathBuf::from("/nonexistent/content/characters"),
        ..config()
    }
}

/// A spec of `crates/engine/tests/golden`: `striker`, `tank`, `trickster` or `turtle`.
pub fn golden_spec(name: &str) -> CharacterSpec {
    let path = repo_root().join(format!("crates/engine/tests/golden/spec-{name}.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The seed `i` of the golden vectors of the engine.
pub fn golden_seed(i: u32) -> [u8; 32] {
    let mut seed = *b"torneo-absurdo-engine-golden-v1!";
    for (byte, x) in seed.iter_mut().zip(i.to_le_bytes()) {
        *byte ^= x;
    }
    seed
}

/// The vectors of `crates/engine/tests/golden.rs` that the tests use.
pub const KO: (&str, &str, u32) = ("trickster", "tank", 1);
pub const DRAW: (&str, &str, u32) = ("turtle", "turtle", 118);

/// A fight of a golden vector, and what the engine says of it.
pub struct Vector {
    pub a: CharacterSpec,
    pub b: CharacterSpec,
    pub seed: [u8; 32],
    pub log: FightLog,
}

impl Vector {
    pub fn of((a, b, seed): (&str, &str, u32)) -> Self {
        let (a, b) = (golden_spec(a), golden_spec(b));
        let seed = golden_seed(seed);
        let ruleset = Ruleset::for_version(1).unwrap();
        let log = simulate(&Seed::from_bytes(seed), &a, &b, &ruleset).unwrap();
        Self { a, b, seed, log }
    }

    pub fn canonical(&self) -> Vec<u8> {
        engine::canonical_json(&self.log).unwrap()
    }

    pub fn end_t(&self) -> i64 {
        i64::from(self.log.events.last().unwrap().t())
    }
}

/// The hours of the fight of the slot `opens_at`, with the default schedule.
pub const fn hours(opens_at: i64) -> (i64, i64) {
    (opens_at + WINDOW, opens_at + WINDOW + PRE_LIVE)
}

/// A row as the runner would announce it, with these specs.
pub async fn announce(pool: &PgPool, opens_at: i64, a: &CharacterSpec, b: &CharacterSpec) -> i32 {
    announce_with(pool, opens_at, a, b, |_| {}).await
}

/// The fields of a row, to corrupt them.
pub struct RawFight {
    pub ruleset_version: i16,
    pub spec_a: Vec<u8>,
    pub spec_b: Vec<u8>,
    pub snapshot_sha256: Vec<u8>,
    pub slug_a: String,
    pub slug_b: String,
    pub max_hp_a: i32,
    pub max_hp_b: i32,
}

/// Like [`announce`], with `change` applied to the fields.
pub async fn announce_with(
    pool: &PgPool,
    opens_at: i64,
    a: &CharacterSpec,
    b: &CharacterSpec,
    change: impl FnOnce(&mut RawFight),
) -> i32 {
    let ruleset = Ruleset::for_version(1).unwrap();
    let spec_a = canonical_spec_json(a).unwrap();
    let spec_b = canonical_spec_json(b).unwrap();
    let mut raw = RawFight {
        ruleset_version: 1,
        snapshot_sha256: snapshot_sha256(1, &spec_a, &spec_b).to_vec(),
        spec_a,
        spec_b,
        slug_a: a.slug.clone(),
        slug_b: b.slug.clone(),
        max_hp_a: i32::from(max_hp(&ruleset, a).unwrap()),
        max_hp_b: i32::from(max_hp(&ruleset, b).unwrap()),
    };
    change(&mut raw);
    let (closes_at, live_at) = hours(opens_at);
    store::insert(
        pool,
        &NewFight {
            opens_at,
            closes_at,
            live_at,
            ruleset_version: raw.ruleset_version,
            spec_a: &raw.spec_a,
            spec_b: &raw.spec_b,
            snapshot_sha256: &raw.snapshot_sha256,
            slug_a: &raw.slug_a,
            slug_b: &raw.slug_b,
            max_hp_a: raw.max_hp_a,
            max_hp_b: raw.max_hp_b,
            time_limit_ms: i32::try_from(ruleset.time_limit_ms()).unwrap(),
            max_duration_ms: i32::try_from(max_duration_ms(&ruleset).unwrap()).unwrap(),
            matchup_round: 1,
            roster_sha256: &[1; 32],
        },
    )
    .await
    .unwrap()
    .unwrap()
}

/// `scheduled` → `betting_open` with this seed: the legal route of worry 1.
pub async fn open_with(pool: &PgPool, id: i32, seed: &[u8; 32], at: i64) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO fight_secrets (fight_id, seed) VALUES ($1, $2)")
        .bind(id)
        .bind(seed.as_slice())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE fights SET state = 'betting_open', seed_commit = $2, \
         opened_at = timestamptz 'epoch' + $3::bigint * interval '1 millisecond' WHERE id = $1",
    )
    .bind(id)
    .bind(Sha256::digest(seed).as_slice())
    .bind(at)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// A fight of `vector`, open with its seed.
pub async fn open_vector(pool: &PgPool, opens_at: i64, vector: &Vector) -> i32 {
    let id = announce(pool, opens_at, &vector.a, &vector.b).await;
    open_with(pool, id, &vector.seed, opens_at).await;
    id
}

/// Legal SQL: `betting_open` → `betting_closed`.
pub async fn close_by_sql(pool: &PgPool, id: i32, at: i64) {
    sqlx::query(
        "UPDATE fights SET state = 'betting_closed', \
         closed_at = timestamptz 'epoch' + $2::bigint * interval '1 millisecond' WHERE id = $1",
    )
    .bind(id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
}

/// Legal SQL: `betting_closed` → `live`.
pub async fn live_by_sql(pool: &PgPool, id: i32, started_at: i64) {
    sqlx::query(
        "UPDATE fights SET state = 'live', \
         live_started_at = timestamptz 'epoch' + $2::bigint * interval '1 millisecond' \
         WHERE id = $1",
    )
    .bind(id)
    .bind(started_at)
    .execute(pool)
    .await
    .unwrap();
}

/// What only the CLI of TA-27 will write (RF59), in the caller's transaction: a voided final, its
/// seed revealed and its secret gone.
pub async fn void_by_sql(conn: &mut sqlx::PgConnection, id: i32, seed: &[u8; 32], reason: &str) {
    sqlx::query(
        "UPDATE fights SET state = 'settled', settled_at = now(), void_reason = $2, \
         revealed_seed = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(reason)
    .bind(seed.as_slice())
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query("DELETE FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .unwrap();
}

/// Removes the secret of a fight, as only someone with the credential can: with the guard off.
pub async fn lose_secret(pool: &PgPool, id: i32) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE fight_secrets DISABLE TRIGGER fight_secrets_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE fight_secrets ENABLE TRIGGER fight_secrets_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// A secret that is not the one of the commit, as only someone with the credential can write it.
pub async fn replace_secret(pool: &PgPool, id: i32, seed: &[u8; 32]) {
    lose_secret(pool, id).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE fight_secrets DISABLE TRIGGER fight_secrets_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO fight_secrets (fight_id, seed) VALUES ($1, $2)")
        .bind(id)
        .bind(seed.as_slice())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE fight_secrets ENABLE TRIGGER fight_secrets_guard")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// The times of a test: a lock and a retry that do not make it wait.
pub fn quick_timings() -> RunnerTimings {
    RunnerTimings {
        lock_retry_ms: 50,
        heartbeat_ms: 100,
        heartbeat_timeout_ms: 3000,
        settle_backoff_min_ms: 1000,
        lock_timeout_ms: 200,
        db_backoff_min_ms: 50,
        db_backoff_max_ms: 200,
        panic_backoff_ms: 50,
        ..RunnerTimings::default()
    }
}

/// A manual clock at `now` and the quick times.
pub fn settings_at(now: i64) -> (FightSettings, FightClock) {
    let clock = FightClock::manual(now);
    (
        FightSettings {
            clock: clock.clone(),
            timings: quick_timings(),
            ..FightSettings::default()
        },
        clock,
    )
}

/// A runner that a test can stop or kill.
pub struct Running {
    stop: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl Running {
    /// As the process would on a signal, or dying: no state changes, and a transaction reverts.
    pub async fn stop(mut self) {
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub fn spawn_runner(
    pool: &PgPool,
    shared: &Arc<FightsShared>,
    config: FightsConfig,
    settings: FightSettings,
) -> Running {
    let stop = CancellationToken::new();
    let task = tokio::spawn(server::fights::runner::run(
        pool.clone(),
        shared.clone(),
        config,
        settings,
        stop.clone(),
    ));
    Running {
        stop,
        task: Some(task),
    }
}

/// A shared state and a runner without a roster.
pub fn start(pool: &PgPool, settings: &FightSettings) -> (Arc<FightsShared>, Running) {
    let shared = Arc::new(FightsShared::new(settings.clock.clone()));
    let running = spawn_runner(pool, &shared, config_without_roster(), settings.clone());
    (shared, running)
}

/// Polls `condition` for up to 60 s: only a failing test waits that long, and a loaded machine does
/// not make a passing one fail.
pub async fn wait_for<F, Fut>(what: &str, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if condition().await {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub async fn row(pool: &PgPool, id: i32) -> FightRow {
    store::by_id(pool, id).await.unwrap().unwrap()
}

pub async fn wait_state(pool: &PgPool, id: i32, state: FightState) {
    wait_for(state.as_str(), || async {
        row(pool, id).await.state == state
    })
    .await;
}

/// Takes the runner's fight `id`, already opened, to `settled` with the manual clock.
pub async fn drive_to_settled(pool: &PgPool, clock: &FightClock, id: i32) {
    let fight = row(pool, id).await;
    clock.set(clock.now_ms().max(fight.closes_at));
    wait_state(pool, id, FightState::BettingClosed).await;
    clock.set(clock.now_ms().max(fight.live_at));
    wait_state(pool, id, FightState::Live).await;
    let started = row(pool, id).await.live_started_at.unwrap();
    // Past any end and any settle delay.
    clock.set(clock.now_ms().max(started + 100_000));
    wait_state(pool, id, FightState::Settled).await;
}

/// Until the fight reaches `state`, moving the clock forward meanwhile: the hour of a retry depends
/// on when the runner saw the failure, which a test cannot know.
pub async fn wait_state_advancing(pool: &PgPool, clock: &FightClock, id: i32, state: FightState) {
    wait_for(state.as_str(), || async {
        clock.advance(2000);
        row(pool, id).await.state == state
    })
    .await;
}

/// How many rows of `fight_secrets` a fight has.
pub async fn secrets_of(pool: &PgPool, id: i32) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fight_secrets WHERE fight_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The app in process on `db`, with its shared state. It programs no fights by itself.
pub async fn start_app(
    db: &TestDb,
    vars: &[(&str, &str)],
    settings: &Settings,
) -> (RunningApp, PgPool, Arc<FightsShared>) {
    let url = db.url();
    let mut all = vec![("DATABASE_URL", url.as_str())];
    all.extend_from_slice(vars);
    let config = super::config::config(&all);
    let pool = db.migrated_pool().await;
    let state = AppState::from_config(&config, settings, pool.clone(), Arc::new(AlwaysUp)).unwrap();
    let shared = state.fights.clone();
    (super::app::start(state).await, pool, shared)
}

/// A sink that remembers every batch.
#[derive(Default)]
pub struct RecordingSink {
    pub batches: Mutex<Vec<(i32, usize, Vec<FightEvent>)>>,
}

impl RecordingSink {
    pub fn events(&self) -> Vec<FightEvent> {
        self.batches
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, _, events)| events.clone())
            .collect()
    }

    pub fn starts(&self) -> Vec<usize> {
        self.batches
            .lock()
            .unwrap()
            .iter()
            .map(|(_, from, _)| *from)
            .collect()
    }
}

impl EventSink for RecordingSink {
    fn publish(&self, fight_id: i32, from_index: usize, events: &[FightEvent]) {
        self.batches
            .lock()
            .unwrap()
            .push((fight_id, from_index, events.to_vec()));
    }
}
