//! Logs (RF-B15, plan-b decision 4): one JSON object per line on stdout, with a closed
//! allowlist of fields.
//!
//! - Every line has `ts` (RFC 3339, UTC) and `level`.
//! - Events of this crate (`target == "server"` or `server::…`) add only the allowlisted
//!   fields. Anything else, `message` included, is dropped.
//! - Events of any other crate (sqlx, hyper, tungstenite…) write only `ts`, `level`, `target`
//!   and `"event":"external"`, so their text (which may hold connection options, URIs or
//!   payloads) never reaches the log.
//! - Span fields are never written; the server does not open spans with fields.
//! - Values are written with `serde_json`, so no value can break the line or inject fields.
//! - Third-party crates are capped at `warn` whatever `RUST_LOG` says.
//! - The panic hook logs only `event`, `file` and `line` (from `std::panic::Location`), never
//!   the panic message.

use std::fmt;

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{EnvFilter, FilterExt, LevelFilter, ParseError, Targets};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, MakeWriter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

/// Fields an event of this crate may write. Closed list: adding one means updating the
/// privacy notice (spec, "Diseño → Logs y plazo").
pub const ALLOWED_FIELDS: &[&str] = &[
    "event",
    "method",
    "route",
    "status",
    "latency_ms",
    "request_id",
    "var",
    "port",
    "error_kind",
    "close_code",
    "file",
    "line",
];

/// Third-party crates that never log below `warn`, even if `RUST_LOG` names them.
pub const CAPPED_TARGETS: &[&str] = &[
    "sqlx",
    "hyper",
    "hyper_util",
    "tungstenite",
    "tokio_tungstenite",
    "h2",
];

/// Whether an event belongs to this crate (and may write allowlisted fields).
pub fn is_own_target(target: &str) -> bool {
    target == "server" || target.starts_with("server::")
}

/// The JSON-per-line formatter.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonLine;

impl<S, N> FormatEvent<S, N> for JsonLine
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut ts = String::new();
        SystemTime.format_time(&mut Writer::new(&mut ts))?;

        let mut line = Map::new();
        line.insert("ts".to_owned(), Value::String(ts));
        line.insert(
            "level".to_owned(),
            Value::String(meta.level().as_str().to_owned()),
        );
        if is_own_target(meta.target()) {
            event.record(&mut AllowlistVisitor { line: &mut line });
        } else {
            line.insert("target".to_owned(), Value::String(meta.target().to_owned()));
            line.insert("event".to_owned(), Value::String("external".to_owned()));
        }
        let text = serde_json::to_string(&line).map_err(|_| fmt::Error)?;
        writeln!(writer, "{text}")
    }
}

struct AllowlistVisitor<'a> {
    line: &'a mut Map<String, Value>,
}

impl AllowlistVisitor<'_> {
    fn put(&mut self, field: &Field, value: Value) {
        let name = field.name();
        // `ts` and `level` are always the formatter's own; `message` is never allowlisted.
        if ALLOWED_FIELDS.contains(&name) {
            self.line.insert(name.to_owned(), value);
        }
    }
}

impl Visit for AllowlistVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::String(value.to_owned()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::Bool(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if ALLOWED_FIELDS.contains(&field.name()) {
            self.put(field, Value::String(format!("{value:?}")));
        }
    }
}

/// The caps for third-party crates (always applied on top of `RUST_LOG`).
fn caps() -> Targets {
    CAPPED_TARGETS.iter().fold(
        Targets::new().with_default(LevelFilter::TRACE),
        |t, name| t.with_target(*name, Level::WARN),
    )
}

/// The formatting layer with the `RUST_LOG` filter and the caps, writing to `writer`.
///
/// The caps are a separate filter combined with `and`, not extra `EnvFilter` directives: a
/// more specific directive in `RUST_LOG` (for example `sqlx::query=trace`) would otherwise win
/// over `sqlx=warn`.
pub fn layer<S, W>(rust_log: &str, writer: W) -> Result<impl Layer<S>, ParseError>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let env = EnvFilter::try_new(rust_log)?;
    Ok(tracing_subscriber::fmt::layer()
        .event_format(JsonLine)
        .with_writer(writer)
        .with_filter(env.and(caps())))
}

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("invalid RUST_LOG")]
    Filter,
    #[error("a global subscriber is already set")]
    AlreadySet,
}

/// Installs the global subscriber (stdout) and the panic hook.
pub fn init(rust_log: &str) -> Result<(), TelemetryError> {
    let layer = layer(rust_log, std::io::stdout).map_err(|_| TelemetryError::Filter)?;
    tracing_subscriber::registry()
        .with(layer)
        .try_init()
        .map_err(|_| TelemetryError::AlreadySet)?;
    install_panic_hook();
    Ok(())
}

/// Replaces the default panic hook (which prints the message to stderr) with one that logs
/// only where the panic happened.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let (file, line) = info.location().map_or(("unknown", 0), |location| {
            (location.file(), location.line())
        });
        tracing::error!(event = "panic", file, line);
    }));
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn lines(&self) -> Vec<serde_json::Map<String, Value>> {
            let bytes = self.0.lock().unwrap().clone();
            String::from_utf8(bytes)
                .unwrap()
                .lines()
                .map(|l| match serde_json::from_str::<Value>(l).unwrap() {
                    Value::Object(map) => map,
                    other => panic!("not an object: {other}"),
                })
                .collect()
        }
    }

    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'w> MakeWriter<'w> for Capture {
        type Writer = Capture;
        fn make_writer(&'w self) -> Self::Writer {
            self.clone()
        }
    }

    fn capture(rust_log: &str, emit: impl FnOnce()) -> Vec<serde_json::Map<String, Value>> {
        let out = Capture::default();
        let subscriber = tracing_subscriber::registry().with(layer(rust_log, out.clone()).unwrap());
        tracing::subscriber::with_default(subscriber, emit);
        out.lines()
    }

    fn keys(line: &serde_json::Map<String, Value>) -> Vec<&str> {
        line.keys().map(String::as_str).collect()
    }

    #[test]
    fn own_events_keep_only_allowlisted_fields() {
        let lines = capture("info", || {
            tracing::info!(
                target: "server::http",
                event = "http.request",
                route = "/api/health",
                status = 200u16,
                cookie = "canary-cookie",
                uri = "/x?canary=1",
                "message canary"
            );
        });
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        let mut found = keys(line);
        found.sort_unstable();
        assert_eq!(found, ["event", "level", "route", "status", "ts"]);
        assert_eq!(line["event"], "http.request");
        assert_eq!(line["level"], "INFO");
        assert_eq!(line["status"], 200);
        let text = serde_json::to_string(line).unwrap();
        assert!(!text.contains("canary"));
    }

    #[test]
    fn external_events_write_only_ts_level_target_and_event() {
        let lines = capture("info", || {
            tracing::warn!(
                target: "sqlx::query",
                event = "anything",
                summary = "SELECT canary",
                "password canary"
            );
        });
        assert_eq!(lines.len(), 1);
        let mut found = keys(&lines[0]);
        found.sort_unstable();
        assert_eq!(found, ["event", "level", "target", "ts"]);
        assert_eq!(lines[0]["event"], "external");
        assert_eq!(lines[0]["target"], "sqlx::query");
    }

    #[test]
    fn similarly_named_crates_are_external() {
        assert!(is_own_target("server"));
        assert!(is_own_target("server::ws::session"));
        assert!(!is_own_target("server_x"));
        assert!(!is_own_target("serverless::x"));
        let lines = capture("info", || {
            tracing::info!(target: "server_x", event = "canary", route = "canary");
        });
        assert_eq!(lines[0]["event"], "external");
        assert!(!lines[0].contains_key("route"));
    }

    #[test]
    fn caps_win_over_rust_log() {
        for rust_log in [
            "sqlx=trace,hyper=trace",
            "trace",
            "debug,sqlx::query=trace,tungstenite=trace",
        ] {
            let lines = capture(rust_log, || {
                tracing::debug!(target: "sqlx::query", "debug canary");
                tracing::info!(target: "hyper::proto", "info canary");
                tracing::trace!(target: "tungstenite::protocol", "trace canary");
                tracing::info!(target: "h2::codec", "info canary");
                tracing::debug!(target: "server::ws", event = "ws.closed", close_code = 1008u16);
            });
            // Without a default directive (the first case) the own debug event is filtered
            // out too; in every case no third-party line gets through below warn.
            assert!(
                lines.iter().all(|l| l["event"] == "ws.closed"),
                "{rust_log}: {lines:?}"
            );
        }
        let lines = capture("debug,sqlx::query=trace", || {
            tracing::debug!(target: "sqlx::query", "debug canary");
            tracing::warn!(target: "sqlx::pool", "warn canary");
            tracing::debug!(target: "server::ws", event = "ws.closed", close_code = 1008u16);
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0]["event"], "external");
        assert_eq!(lines[0]["level"], "WARN");
        assert_eq!(lines[1]["event"], "ws.closed");
        assert_eq!(lines[1]["close_code"], 1008);
    }

    #[test]
    fn rust_log_still_filters_own_events() {
        let lines = capture("info", || {
            tracing::debug!(target: "server::ws", event = "ws.closed");
        });
        assert!(lines.is_empty());
    }

    #[test]
    fn values_are_escaped_and_cannot_inject_fields() {
        let lines = capture("info", || {
            tracing::info!(
                target: "server",
                event = "x\"},\"injected\":\"1\n{\"level\":\"ERROR"
            );
        });
        assert_eq!(lines.len(), 1);
        let mut found = keys(&lines[0]);
        found.sort_unstable();
        assert_eq!(found, ["event", "level", "ts"]);
        assert_eq!(lines[0]["level"], "INFO");
    }

    #[test]
    fn ts_is_rfc3339_utc() {
        let lines = capture("info", || tracing::info!(target: "server", event = "x"));
        let ts = lines[0]["ts"].as_str().unwrap();
        assert!(ts.ends_with('Z'), "{ts}");
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
    }

    #[test]
    fn invalid_rust_log_is_an_error() {
        assert!(layer::<tracing_subscriber::Registry, _>("sqlx=loud", Capture::default()).is_err());
    }
}
