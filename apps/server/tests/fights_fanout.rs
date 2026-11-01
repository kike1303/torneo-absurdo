//! 010 RNF "Carga" and worries 7 and 8: 2000 sockets in local, RSS and p99 of delivery. Ignored: it
//! does not run in `just check` nor in CI, it reports numbers and it does not block. These are numbers
//! of a laptop: page sizes differ, `ps` does not see the buffers of the kernel and loopback has no
//! limit of link, so the proof that counts is the one of TA-8.
//!
//! One scenario per process, so what one leaves in the allocator does not count for the next:
//!
//! ```text
//! ulimit -n 8192  # at least 5000
//! bin=$(cargo test --release -p server --test fights_fanout --no-run --message-format=json \
//!   | jq -r 'select(.executable != null and .target.name == "fights_fanout").executable')
//! for s in A B C D D2 E F1 F2 F3 F4; do
//!   DATABASE_URL=… FANOUT_SCENARIO=$s "$bin" --ignored --exact fanout_server --nocapture
//! done
//! FANOUT_READ_BUFFER=131072 FANOUT_SCENARIO=A … # DP-B1: the default of tungstenite
//! ```
//!
//! The server runs here, with the allocator of `main.rs` and one worker (the server has 1 vCPU), and
//! the clients in a copy of this process, so they do not count in its RSS. The hub is fed through its
//! sink, on a fight that went `live` with its start far ahead, so its own release stays silent: that
//! is the only way to test 418 events, which no golden vector reaches.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

mod support;

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use engine::FightEvent;
use futures_util::{SinkExt, StreamExt};
use mimalloc::MiMalloc;
use protocol::FightState;
use serde_json::{Value, json};
use server::app::Settings;
use server::fights::release::EventSink;
use server::fights::{FightSettings, FightsShared, RunnerTimings};
use server::ws::{WsConfig, WsState};
use sqlx::PgPool;
use support::app::{ORIGIN, RunningApp, TRUST_LOOPBACK};
use support::db::TestDb;
use support::fights::{self as f, DRAW, Vector, wait_state};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const SOCKETS: usize = 2000;
const BOUND_EVENTS: usize = 418;
/// How long a socket that asks for the whole fight stays to receive it.
const HOLD_PAGES: Duration = Duration::from_millis(400);
/// What a stuck socket can have in the kernel: the whole log (amendment DP-B9).
const STUCK_LOG_BYTES: usize = 47_000;

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

fn ip_of(n: usize) -> String {
    format!("10.{}.{}.{}", (n >> 16) & 255, (n >> 8) & 255, n & 255)
}

fn percentile(sorted: &[i64], p: usize) -> i64 {
    if sorted.is_empty() {
        return -1;
    }
    sorted[(sorted.len() - 1) * p / 100]
}

// --- The clients, in their own process ------------------------------------------------------------

#[derive(Default)]
struct Stats {
    connected: AtomicU64,
    closed: AtomicU64,
    rate_limited: AtomicU64,
    failed: AtomicU64,
    syncs: AtomicU64,
    bytes: Arc<AtomicU64>,
    latencies: Mutex<Vec<i64>>,
    hashes: Mutex<HashMap<usize, u64>>,
}

/// What a socket does after connecting.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Viewer,
    /// Subscribes and unsubscribes one channel, each `every_ms`, `burst` times in a row.
    Alternate {
        every_ms: u64,
        burst: u32,
    },
}

fn subscribe_text(channel: &str) -> String {
    json!({"type": "subscribe", "channel": channel}).to_string()
}

fn unsubscribe_text(channel: &str) -> String {
    json!({"type": "unsubscribe", "channel": channel}).to_string()
}

fn account(stats: &Stats, index: usize, text: &str, hash: &mut u64) {
    stats
        .bytes
        .fetch_add(u64::try_from(text.len()).unwrap(), Ordering::Relaxed);
    if text.starts_with(r#"{"type":"fight_events""#) {
        // By hand: parsing 47 KB 2000 times would be measuring the clients.
        let needle = r#""server_time":""#;
        if let Some(at) = text.rfind(needle) {
            let rest = &text[at + needle.len()..];
            if let Some(sent) = rest.split('"').next().and_then(|t| t.parse::<i64>().ok()) {
                stats.latencies.lock().unwrap().push(now_ms() - sent);
            }
        }
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        *hash = hash.wrapping_mul(31).wrapping_add(hasher.finish());
        stats.hashes.lock().unwrap().insert(index, *hash);
    } else if text.starts_with(r#"{"type":"fight_sync""#) {
        stats.syncs.fetch_add(1, Ordering::Relaxed);
    }
}

async fn run_socket(
    addr: SocketAddr,
    index: usize,
    channels: Vec<String>,
    kind: Kind,
    stats: Arc<Stats>,
    stop: CancellationToken,
) {
    let ip = ip_of(index);
    let Ok(mut client) =
        support::ws::connect_with(addr, ORIGIN, &[("x-forwarded-for", ip.as_str())]).await
    else {
        stats.failed.fetch_add(1, Ordering::Relaxed);
        return;
    };
    stats.connected.fetch_add(1, Ordering::Relaxed);
    let mut hash = 0_u64;
    let alternate = match kind {
        Kind::Alternate { .. } => channels.first().cloned(),
        _ => None,
    };
    if alternate.is_none() {
        for channel in &channels {
            let _ = client
                .send(Message::Text(subscribe_text(channel).into()))
                .await;
        }
    }
    let (every, burst) = match kind {
        Kind::Alternate { every_ms, burst } => (Duration::from_millis(every_ms), burst.max(1)),
        _ => (Duration::from_secs(3600), 1),
    };
    // Evenly over a period, unless they go all at once.
    let stagger = if burst > 1 {
        Duration::from_secs(2)
    } else {
        every.mul_f64(f64::from(u32::try_from(index % 1000).unwrap()) / 1000.0)
    };
    // Subscribed for long enough to receive its pages: unsubscribing at once would stop them.
    let mut cycle = tokio::time::Instant::now() + stagger;
    let mut next = cycle;
    let mut subscribed = false;
    let mut left = burst;
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            () = tokio::time::sleep_until(next), if alternate.is_some() => {
                let channel = alternate.as_deref().unwrap_or_default();
                if subscribed {
                    let _ = client.send(Message::Text(unsubscribe_text(channel).into())).await;
                    subscribed = false;
                    left -= 1;
                    if left == 0 {
                        left = burst;
                        cycle += every;
                        next = cycle;
                    } else {
                        next = tokio::time::Instant::now() + Duration::from_millis(50);
                    }
                } else {
                    let _ = client.send(Message::Text(subscribe_text(channel).into())).await;
                    subscribed = true;
                    next = tokio::time::Instant::now() + HOLD_PAGES;
                }
            }
            read = client.next() => match read {
                Some(Ok(Message::Text(text))) => account(&stats, index, text.as_str(), &mut hash),
                Some(Ok(Message::Close(frame))) => {
                    if frame.is_some_and(|frame| frame.reason.as_str() == "ws.rate_limited") {
                        stats.rate_limited.fetch_add(1, Ordering::Relaxed);
                    }
                    stats.closed.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    stats.closed.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            },
        }
    }
}

/// A socket that subscribes and never reads again, with the smallest buffer the system gives.
async fn stuck_socket(addr: SocketAddr, index: usize, channel: String, stats: Arc<Stats>) {
    let connected = async {
        let socket = TcpSocket::new_v4().ok()?;
        socket.set_recv_buffer_size(2048).ok()?;
        let stream = socket.connect(addr).await.ok()?;
        let mut request =
            tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
                format!("ws://{addr}/ws"),
            )
            .ok()?;
        let headers = request.headers_mut();
        headers.insert("origin", ORIGIN.parse().ok()?);
        headers.insert("x-forwarded-for", ip_of(index).parse().ok()?);
        let (mut client, _) = tokio_tungstenite::client_async(request, stream)
            .await
            .ok()?;
        client
            .send(Message::Text(subscribe_text(&channel).into()))
            .await
            .ok()?;
        Some(client)
    }
    .await;
    if let Some(client) = connected {
        stats.connected.fetch_add(1, Ordering::Relaxed);
        // Held, not read, until the process ends.
        std::future::pending::<()>().await;
        drop(client);
    } else {
        stats.failed.fetch_add(1, Ordering::Relaxed);
    }
}

/// Reconnects from the same IP at the pace of the limit of upgrades: each connection asks for its
/// three `fight_sync` and goes.
async fn reconnecting(
    addr: SocketAddr,
    index: usize,
    channel: String,
    stats: Arc<Stats>,
    stop: CancellationToken,
) {
    let ip = ip_of(index);
    let mut hash = 0;
    while !stop.is_cancelled() {
        if let Ok(mut client) =
            support::ws::connect_with(addr, ORIGIN, &[("x-forwarded-for", ip.as_str())]).await
        {
            stats.connected.fetch_add(1, Ordering::Relaxed);
            // Subscribed long enough to receive its pages, the three of its window.
            for _ in 0..3 {
                let _ = client
                    .send(Message::Text(subscribe_text(&channel).into()))
                    .await;
                let until = tokio::time::Instant::now() + HOLD_PAGES;
                while let Ok(Some(Ok(message))) =
                    tokio::time::timeout_at(until, client.next()).await
                {
                    if let Message::Text(text) = message {
                        account(&stats, index, text.as_str(), &mut hash);
                    }
                }
                let _ = client
                    .send(Message::Text(unsubscribe_text(&channel).into()))
                    .await;
            }
            drop(client);
        }
        tokio::time::sleep(Duration::from_millis(2100)).await;
    }
}

/// The clients, driven line by line by the server process.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a part of fanout_server"]
async fn fanout_clients() {
    let Ok(control) = std::env::var("FANOUT_CONTROL") else {
        return;
    };
    let stream = TcpStream::connect(&control).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let mut groups: HashMap<String, (Arc<Stats>, CancellationToken)> = HashMap::new();
    let total_bytes = Arc::new(AtomicU64::new(0));
    let per_second: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let (total, samples) = (total_bytes.clone(), per_second.clone());
        tokio::spawn(async move {
            let mut last = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let now = total.load(Ordering::Relaxed);
                samples.lock().unwrap().push(now - last);
                last = now;
            }
        });
    }

    while let Some(line) = lines.next_line().await.unwrap() {
        let command: Value = serde_json::from_str(&line).unwrap();
        let group = command["group"].as_str().unwrap_or_default().to_owned();
        let answer = match command["cmd"].as_str().unwrap() {
            "connect" => {
                let addr: SocketAddr = command["addr"].as_str().unwrap().parse().unwrap();
                let count = usize::try_from(command["count"].as_u64().unwrap()).unwrap();
                let first = usize::try_from(command["first"].as_u64().unwrap()).unwrap();
                let channels: Vec<String> = command["channels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap().to_owned())
                    .collect();
                let (stats, stop) = groups.entry(group).or_insert_with(|| {
                    (
                        Arc::new(Stats {
                            bytes: total_bytes.clone(),
                            ..Stats::default()
                        }),
                        CancellationToken::new(),
                    )
                });
                let kind = command["kind"].as_str().unwrap();
                let every_ms = command["every_ms"].as_u64().unwrap_or(0);
                let burst = u32::try_from(command["burst"].as_u64().unwrap_or(1)).unwrap();
                let permits = Arc::new(tokio::sync::Semaphore::new(64));
                for index in first..first + count {
                    let (stats, stop) = (stats.clone(), stop.clone());
                    let channels = channels.clone();
                    let channel = channels.first().cloned().unwrap_or_default();
                    // Not all the handshakes at once: the backlog of the listener is finite.
                    let permit = permits.clone().acquire_owned().await.unwrap();
                    let task_stats = stats.clone();
                    match kind {
                        "stuck" => {
                            tokio::spawn(stuck_socket(addr, index, channel, task_stats));
                        }
                        "reconnect" => {
                            tokio::spawn(reconnecting(addr, index, channel, task_stats, stop));
                        }
                        _ => {
                            let kind = if kind == "alternate" {
                                Kind::Alternate { every_ms, burst }
                            } else {
                                Kind::Viewer
                            };
                            tokio::spawn(run_socket(addr, index, channels, kind, task_stats, stop));
                        }
                    }
                    drop(permit);
                    if index % 16 == 0 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                }
                json!({"ok": true})
            }
            "report" => {
                let (stats, _) = groups.get(&group).unwrap();
                let mut latencies = stats.latencies.lock().unwrap().clone();
                latencies.sort_unstable();
                let hashes: HashSet<u64> = stats.hashes.lock().unwrap().values().copied().collect();
                json!({
                    "connected": stats.connected.load(Ordering::Relaxed),
                    "closed": stats.closed.load(Ordering::Relaxed),
                    "rate_limited": stats.rate_limited.load(Ordering::Relaxed),
                    "failed": stats.failed.load(Ordering::Relaxed),
                    "syncs": stats.syncs.load(Ordering::Relaxed),
                    "samples": latencies.len(),
                    "p50_ms": percentile(&latencies, 50),
                    "p99_ms": percentile(&latencies, 99),
                    "max_ms": latencies.last().copied().unwrap_or(-1),
                    "distinct_streams": hashes.len(),
                })
            }
            "bytes" => {
                let mut samples = per_second.lock().unwrap();
                let taken = std::mem::take(&mut *samples);
                let sum: u64 = taken.iter().sum();
                let count = u64::try_from(taken.len().max(1)).unwrap();
                json!({
                    "mean_bytes_per_s": sum / count,
                    "peak_bytes_per_s": taken.iter().max().copied().unwrap_or(0),
                })
            }
            // What came while connecting is not what is measured: those pages carry an older hour.
            "reset" => {
                let (stats, _) = groups.get(&group).unwrap();
                stats.latencies.lock().unwrap().clear();
                json!({"ok": true})
            }
            "stop" => {
                if let Some((_, stop)) = groups.get(&group) {
                    stop.cancel();
                }
                json!({"ok": true})
            }
            _ => break,
        };
        write
            .write_all(format!("{answer}\n").as_bytes())
            .await
            .unwrap();
    }
    std::process::exit(0);
}

// --- The server --------------------------------------------------------------------------------

struct Control {
    lines: tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    write: tokio::net::tcp::OwnedWriteHalf,
}

impl Control {
    async fn ask(&mut self, command: Value) -> Value {
        self.write
            .write_all(format!("{command}\n").as_bytes())
            .await
            .unwrap();
        let line = self.lines.next_line().await.unwrap().unwrap();
        serde_json::from_str(&line).unwrap()
    }

    async fn connect(
        &mut self,
        group: &str,
        addr: SocketAddr,
        count: usize,
        first: usize,
        extra: Value,
    ) {
        let mut command = json!({
            "cmd": "connect",
            "group": group,
            "addr": addr.to_string(),
            "count": count,
            "first": first,
            "kind": "viewer",
        });
        for (key, value) in extra.as_object().unwrap() {
            command[key] = value.clone();
        }
        self.ask(command).await;
    }

    async fn report(&mut self, group: &str) -> Value {
        self.ask(json!({"cmd": "report", "group": group})).await
    }
}

/// The resident memory of this process, from outside it, sampled to keep the peak.
struct Rss {
    current: Arc<AtomicU64>,
    peak: Arc<AtomicU64>,
}

impl Rss {
    fn start() -> Self {
        let current = Arc::new(AtomicU64::new(0));
        let peak = Arc::new(AtomicU64::new(0));
        let (c, p) = (current.clone(), peak.clone());
        tokio::spawn(async move {
            loop {
                if let Some(kib) = read_rss().await {
                    c.store(kib, Ordering::Relaxed);
                    p.fetch_max(kib, Ordering::Relaxed);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        Self { current, peak }
    }

    async fn now_mib(&self) -> f64 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        mib(self.current.load(Ordering::Relaxed))
    }

    fn peak_mib(&self) -> f64 {
        mib(self.peak.load(Ordering::Relaxed))
    }
}

fn mib(kib: u64) -> f64 {
    u32::try_from(kib).map_or(f64::MAX, f64::from) / 1024.0
}

async fn read_rss() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = tokio::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn longest_events(vector: &Vector, count: usize) -> Vec<FightEvent> {
    let longest = vector
        .log
        .events
        .iter()
        .max_by_key(|event| serde_json::to_string(event).unwrap().len())
        .unwrap();
    vec![longest.clone(); count]
}

struct Ctx {
    app: RunningApp,
    pool: PgPool,
    shared: Arc<FightsShared>,
    sockets: Arc<WsState>,
    id: i32,
    vector: Vector,
    control: Control,
    rss: Rss,
}

impl Ctx {
    fn publish(&self, from: usize, events: &[FightEvent]) {
        self.shared.hub().publish(self.id, from, events);
    }

    /// Until `done`, or a limit. What it returns says which.
    async fn wait(&mut self, group: &str, limit: Duration, done: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + limit;
        loop {
            let report = self.control.report(group).await;
            if done(&report) || Instant::now() >= deadline {
                return report;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn final_of_the_fight(&self) {
        let mut tx = self.pool.begin().await.unwrap();
        f::void_by_sql(&mut tx, self.id, &self.vector.seed, "engine_error").await;
        tx.commit().await.unwrap();
    }
}

async fn setup(db: &TestDb, config: WsConfig) -> Ctx {
    let vector = Vector::of(DRAW);
    let timings = RunnerTimings {
        reread_ms: 1000,
        ..f::quick_timings()
    };
    let settings = FightSettings {
        timings,
        ..FightSettings::default()
    };
    let app_settings = Settings {
        fights: settings.clone(),
        ws: config,
        ..Settings::default()
    };
    let (app, pool, shared, sockets) = f::start_app_ws(db, &[TRUST_LOOPBACK], &app_settings).await;
    // On air, with its start far ahead: its own release stays silent (R4).
    let opens_at = now_ms() - 3_600_000;
    let id = f::open_vector(&pool, opens_at, &vector).await;
    let (closes_at, _) = f::hours(opens_at);
    f::close_by_sql(&pool, id, closes_at).await;
    f::live_by_sql(&pool, id, now_ms() + 3_600_000).await;
    tokio::spawn({
        let (pool, shared) = (pool.clone(), shared.clone());
        async move {
            let runner = f::spawn_runner(&pool, &shared, f::config_without_roster(), settings);
            std::future::pending::<()>().await;
            drop(runner);
        }
    });
    wait_state(&pool, id, FightState::Live).await;
    let hub = shared.hub().clone();
    f::wait_for("the hub", || async { hub.knows(id) }).await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = listener.local_addr().unwrap().to_string();
    let exe = std::env::current_exe().unwrap();
    let mut child = tokio::process::Command::new(exe)
        .args(["--ignored", "--exact", "fanout_clients", "--nocapture"])
        .env("FANOUT_CONTROL", control_addr)
        .stdout(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    let (read, write) = stream.into_split();
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ctx {
        app,
        pool,
        shared,
        sockets,
        id,
        vector,
        control: Control {
            lines: BufReader::new(read).lines(),
            write,
        },
        rss: Rss::start(),
    }
}

fn print(scenario: &str, report: &Value) {
    println!("FANOUT {scenario} {report}");
}

async fn viewers(ctx: &mut Ctx, count: usize, channels: Value) -> Value {
    let addr = ctx.app.addr;
    ctx.control
        .connect("viewers", addr, count, 1, json!({"channels": channels}))
        .await;
    let expected = u64::try_from(count).unwrap();
    ctx.wait("viewers", Duration::from_secs(120), |r| {
        r["connected"].as_u64().unwrap() + r["failed"].as_u64().unwrap() >= expected
    })
    .await
}

fn fight_channel(id: i32) -> Value {
    json!([format!("fight:{id}")])
}

/// A. 2000 sockets in silence (worry 7), and the socket 2001.
async fn scenario_a(ctx: &mut Ctx, read_buffer: usize) {
    let base = ctx.rss.now_mib().await;
    let channels = json!(["schedule", format!("fight:{}", ctx.id)]);
    let connected = viewers(ctx, SOCKETS, channels).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let with_sockets = ctx.rss.now_mib().await;
    let over = support::ws::connect_with(
        ctx.app.addr,
        ORIGIN,
        &[("x-forwarded-for", ip_of(SOCKETS + 100).as_str())],
    )
    .await;
    let over = match over {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            let body = response.body().as_deref().unwrap_or_default().to_vec();
            json!({"status": response.status().as_u16(), "body": String::from_utf8_lossy(&body)})
        }
        Err(_) => json!("another error"),
        Ok(_) => json!("accepted"),
    };
    print(
        "A",
        &json!({
            "read_buffer": read_buffer,
            "connected": connected["connected"],
            "failed": connected["failed"],
            "rss_base_mib": base,
            "rss_mib": with_sockets,
            "rss_peak_mib": ctx.rss.peak_mib(),
            "kib_per_socket": (with_sockets - base) * 1024.0 / 2000.0,
            "socket_2001": over,
        }),
    );
}

/// B. The golden `draw`, at its pace, with what every socket receives.
async fn scenario_b(ctx: &mut Ctx) {
    let channels = fight_channel(ctx.id);
    let connected = viewers(ctx, SOCKETS, channels).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let events = ctx.vector.log.events.clone();
    let start = Instant::now();
    let mut from = 0;
    let mut batches = 0;
    while from < events.len() {
        let t = events[from].t();
        let to = from + events[from..].iter().take_while(|e| e.t() == t).count();
        let at = Duration::from_millis(u64::from(t));
        tokio::time::sleep(at.saturating_sub(start.elapsed())).await;
        ctx.publish(from, &events[from..to]);
        from = to;
        batches += 1;
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let report = ctx.control.report("viewers").await;
    print(
        "B",
        &json!({
            "connected": connected["connected"],
            "batches": batches,
            "events": events.len(),
            "rss_mib": ctx.rss.now_mib().await,
            "rss_peak_mib": ctx.rss.peak_mib(),
            "report": report,
        }),
    );
}

/// C and D. Everyone receives the final `fight_sync`: what stays after it.
async fn scenario_final(ctx: &mut Ctx, name: &str, events: Vec<FightEvent>) {
    let channels = fight_channel(ctx.id);
    viewers(ctx, SOCKETS, channels).await;
    let base = ctx.rss.now_mib().await;
    ctx.publish(0, &events);
    tokio::time::sleep(Duration::from_secs(3)).await;
    ctx.final_of_the_fight().await;
    let started = Instant::now();
    let expected = u64::try_from(2 * SOCKETS).unwrap();
    // The one of subscribing and the final.
    let report = ctx
        .wait("viewers", Duration::from_secs(60), |r| {
            r["syncs"].as_u64().unwrap() >= expected
        })
        .await;
    let took = started.elapsed().as_millis();
    tokio::time::sleep(Duration::from_secs(5)).await;
    let hub = ctx.shared.hub().held(ctx.id);
    print(
        name,
        &json!({
            "events": events.len(),
            "hub_held": hub,
            "rss_before_mib": base,
            "rss_after_mib": ctx.rss.now_mib().await,
            "rss_peak_mib": ctx.rss.peak_mib(),
            "ms_to_deliver": took,
            "report": report,
        }),
    );
}

/// D2. 2000 sockets that subscribe when the 418 events are already out: everyone goes through the pages.
async fn scenario_d2(ctx: &mut Ctx) {
    let events = longest_events(&ctx.vector, BOUND_EVENTS);
    ctx.publish(0, &events);
    let base = ctx.rss.now_mib().await;
    let channels = fight_channel(ctx.id);
    let started = Instant::now();
    let connected = viewers(ctx, SOCKETS, channels).await;
    let expected = u64::try_from(SOCKETS).unwrap();
    let report = ctx
        .wait("viewers", Duration::from_secs(60), |r| {
            r["syncs"].as_u64().unwrap() >= expected
        })
        .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let bytes = ctx.control.ask(json!({"cmd": "bytes"})).await;
    print(
        "D2",
        &json!({
            "events": events.len(),
            "connected": connected["connected"],
            "failed": connected["failed"],
            "hub_held": ctx.shared.hub().held(ctx.id),
            "rss_before_mib": base,
            "rss_after_mib": ctx.rss.now_mib().await,
            "rss_peak_mib": ctx.rss.peak_mib(),
            "ms_total": started.elapsed().as_millis(),
            "bytes": bytes,
            "report": report,
        }),
    );
}

/// E. 2000 sockets that do not read, with one notice after another.
async fn scenario_e(ctx: &mut Ctx) {
    let free = ctx.sockets.available_permits();
    let addr = ctx.app.addr;
    let channels = fight_channel(ctx.id);
    ctx.control
        .connect(
            "stuck",
            addr,
            SOCKETS,
            1,
            json!({"kind": "stuck", "channels": channels}),
        )
        .await;
    let expected = u64::try_from(SOCKETS).unwrap();
    let connected = ctx
        .wait("stuck", Duration::from_secs(120), |r| {
            r["connected"].as_u64().unwrap() + r["failed"].as_u64().unwrap() >= expected
        })
        .await;
    let base = ctx.rss.now_mib().await;
    let batch = longest_events(&ctx.vector, BOUND_EVENTS);
    let mut from = 0;
    let mut rounds = 0;
    let started = Instant::now();
    let mut first_close = None;
    let mut most_open = 0;
    // Half a second per batch, until they are closed or the pong timeout would do it.
    while Instant::now() < started + Duration::from_secs(50) {
        ctx.publish(from, &batch);
        from += batch.len();
        rounds += 1;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let open = free.saturating_sub(ctx.sockets.available_permits());
        most_open = most_open.max(open);
        if open < SOCKETS && first_close.is_none() {
            first_close = Some(started.elapsed().as_millis());
        }
        if open == 0 {
            break;
        }
    }
    let open_at_end = free.saturating_sub(ctx.sockets.available_permits());
    let peak = ctx.rss.peak_mib();
    // Not the size of a page: a stuck socket can have the whole log in the kernel (DP-B9).
    let unconfirmed = f64::from(u32::try_from(SOCKETS * STUCK_LOG_BYTES / 1024).unwrap()) / 1024.0;
    print(
        "E",
        &json!({
            "connected": connected["connected"],
            "failed": connected["failed"],
            "rounds": rounds,
            "most_open": most_open,
            "ms_to_first_close": first_close,
            "ms_to_last_close": (open_at_end == 0).then(|| started.elapsed().as_millis()),
            "open_at_end": open_at_end,
            "rss_before_mib": base,
            "rss_peak_mib": peak,
            "unconfirmed_mib": unconfirmed,
            "rss_peak_plus_unconfirmed_mib": peak + unconfirmed,
            "hub_held": ctx.shared.hub().held(ctx.id),
        }),
    );
}

/// F. Sockets that keep asking for the whole fight, and the ones that only watch.
async fn scenario_f(ctx: &mut Ctx, name: &str) {
    let addr = ctx.app.addr;
    let channel = format!("fight:{}", ctx.id);
    let batch = longest_events(&ctx.vector, BOUND_EVENTS);
    ctx.publish(0, &batch);
    let mut from = batch.len();
    let viewers_report = viewers(ctx, 1000, json!([channel.clone()])).await;
    let channels = json!([channel]);
    let alternating = match name {
        // At the limit of DP-B7, evenly.
        "F1" | "F4" => {
            json!({"kind": "alternate", "every_ms": 20_200, "burst": 1, "channels": channels})
        }
        // Only the limit of 10 messages in 10 s.
        "F2" => json!({"kind": "alternate", "every_ms": 2100, "burst": 1, "channels": channels}),
        // The three of the window, at once, in every socket.
        _ => json!({"kind": "alternate", "every_ms": 60_500, "burst": 3, "channels": channels}),
    };
    let attackers = if name == "F4" { 900 } else { 1000 };
    ctx.control
        .connect("attackers", addr, attackers, 2000, alternating)
        .await;
    if name == "F4" {
        // 40 IP that reconnect at the pace of the limit of upgrades.
        ctx.control
            .connect(
                "reconnecting",
                addr,
                40,
                5000,
                json!({"kind": "reconnect", "channels": channels}),
            )
            .await;
    }
    ctx.control.ask(json!({"cmd": "bytes"})).await;
    ctx.control
        .ask(json!({"cmd": "reset", "group": "viewers"}))
        .await;
    // What the viewers measure: a small batch every 600 ms, as in a fight.
    let small = longest_events(&ctx.vector, 1);
    let started = Instant::now();
    let seconds = if name == "F3" { 70 } else { 65 };
    while started.elapsed() < Duration::from_secs(seconds) {
        ctx.publish(from, &small);
        from += 1;
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    let bytes = ctx.control.ask(json!({"cmd": "bytes"})).await;
    let viewers_end = ctx.control.report("viewers").await;
    let attackers_end = ctx.control.report("attackers").await;
    let reconnecting = if name == "F4" {
        ctx.control.report("reconnecting").await
    } else {
        json!(null)
    };
    let p99 = viewers_end["p99_ms"].as_i64().unwrap();
    print(
        name,
        &json!({
            "viewers_connected": viewers_report["connected"],
            "attackers": attackers,
            "sync_bytes": ctx.shared.hub().held(ctx.id),
            "bytes": bytes,
            "viewers": viewers_end,
            "attackers_report": attackers_end,
            "reconnecting": reconnecting,
            "p99_within_1000_ms": p99 <= 1000,
            "rss_peak_mib": ctx.rss.peak_mib(),
        }),
    );
}

/// The server, on one worker: see the header.
#[test]
#[ignore = "a measurement: see the header"]
fn fanout_server() {
    let Ok(scenario) = std::env::var("FANOUT_SCENARIO") else {
        println!("FANOUT_SCENARIO is not set: see the header of this file");
        return;
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        let limit = std::process::Command::new("sh")
            .args(["-c", "ulimit -n"])
            .output()
            .unwrap();
        let limit = String::from_utf8_lossy(&limit.stdout).trim().to_owned();
        let enough = limit == "unlimited" || limit.parse::<u32>().is_ok_and(|n| n >= 5000);
        assert!(
            enough,
            "ulimit -n is {limit}: at least 5000, see the header"
        );

        let read_buffer = std::env::var("FANOUT_READ_BUFFER")
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(WsConfig::default().read_buffer);
        let mut config = WsConfig {
            read_buffer,
            ..WsConfig::default()
        };
        if scenario == "F2" {
            // In effect, only the limit of 10 messages in 10 s.
            config.subscribe_sync_limit = (3, Duration::from_millis(1));
        }
        let db = TestDb::create().await;
        let mut ctx = setup(&db, config).await;
        let vector_events = ctx.vector.log.events.clone();
        match scenario.as_str() {
            "A" => scenario_a(&mut ctx, read_buffer).await,
            "B" => scenario_b(&mut ctx).await,
            "C" => scenario_final(&mut ctx, "C", vector_events).await,
            "D" => {
                let events = longest_events(&ctx.vector, BOUND_EVENTS);
                scenario_final(&mut ctx, "D", events).await;
            }
            "D2" => scenario_d2(&mut ctx).await,
            "E" => scenario_e(&mut ctx).await,
            name => scenario_f(&mut ctx, name).await,
        }
        // The clients end with the control connection. The rest of the tasks do not end by themselves.
        drop(db);
        tokio::time::sleep(Duration::from_millis(500)).await;
        std::process::exit(0);
    });
}
