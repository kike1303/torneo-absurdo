//! Logs (RF-B15, plan-b decision 4): one JSON object per line on stdout, with a closed
//! allowlist of fields.
//!
//! - Every line has `ts` (RFC 3339, UTC) and `level` (lowercase).
//! - Events of this crate (`target == "server"` or `server::…`) add only the allowlisted
//!   fields. Anything else, `message` included, is dropped.
//! - Events of any other crate (sqlx, hyper, tungstenite…) write only `ts`, `level`, `target`
//!   and `"event":"external"`, so their text (which may hold connection options, URIs or
//!   payloads) never reaches the log.
//! - Each field accepts one type (ADR-0012 A3): strings only through `record_str`, integers
//!   only through `record_u64`/`record_i64`, and `request_id` only as a `%` UUID. A value of
//!   any other type is dropped, so `error_kind = ?err` never writes the error.
//! - Span fields are never written, and [`layer`] is the only layer with output (ADR-0012 A3).
//!   Tracing's `instrument` attribute always goes with `skip_all` (a test walks `src/`).
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
use uuid::Uuid;

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

/// The type a field accepts (ADR-0012 A3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// Only `record_str`.
    Str,
    /// Only `record_u64` and `record_i64`.
    Int,
    /// Only `record_debug` (`%value`), and only if the value is a UUID.
    Uuid,
}

/// The type of an allowlisted field; `None` for any other name.
fn field_kind(name: &str) -> Option<FieldKind> {
    match name {
        "event" | "method" | "route" | "var" | "error_kind" | "file" => Some(FieldKind::Str),
        "status" | "latency_ms" | "port" | "close_code" | "line" => Some(FieldKind::Int),
        "request_id" => Some(FieldKind::Uuid),
        _ => None,
    }
}

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
            Value::String(level_name(*meta.level()).to_owned()),
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

/// The lowercase name of a level (ADR-0012 A1).
fn level_name(level: Level) -> &'static str {
    match level {
        Level::ERROR => "error",
        Level::WARN => "warn",
        Level::INFO => "info",
        Level::DEBUG => "debug",
        Level::TRACE => "trace",
    }
}

struct AllowlistVisitor<'a> {
    line: &'a mut Map<String, Value>,
}

impl AllowlistVisitor<'_> {
    /// Writes the field if its name is allowlisted with `kind`. `ts` and `level` are always
    /// the formatter's own, and `message` has no kind.
    fn put(&mut self, field: &Field, kind: FieldKind, value: Value) {
        let name = field.name();
        if field_kind(name) == Some(kind) {
            self.line.insert(name.to_owned(), value);
        }
    }
}

/// Every method is explicit: the defaults forward to `record_debug`, which would write the
/// `Debug` (or an error's `Display`) of the value. Those that are not one of the types of
/// `FieldKind` drop the field.
impl Visit for AllowlistVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, FieldKind::Str, Value::String(value.to_owned()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, FieldKind::Int, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, FieldKind::Int, Value::from(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field_kind(field.name()) == Some(FieldKind::Uuid) {
            let text = format!("{value:?}");
            if Uuid::parse_str(&text).is_ok() {
                self.put(field, FieldKind::Uuid, Value::String(text));
            }
        }
    }

    fn record_bool(&mut self, _field: &Field, _value: bool) {}

    fn record_f64(&mut self, _field: &Field, _value: f64) {}

    fn record_i128(&mut self, _field: &Field, _value: i128) {}

    fn record_u128(&mut self, _field: &Field, _value: u128) {}

    fn record_bytes(&mut self, _field: &Field, _value: &[u8]) {}

    fn record_error(&mut self, _field: &Field, _value: &(dyn std::error::Error + 'static)) {}
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

/// The subscriber: the registry with [`layer`] as its only layer with output (ADR-0012 A3).
fn subscriber<W>(rust_log: &str, writer: W) -> Result<impl Subscriber + Send + Sync, ParseError>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    Ok(tracing_subscriber::registry().with(layer(rust_log, writer)?))
}

/// Installs the global subscriber (stdout) and the panic hook.
pub fn init(rust_log: &str) -> Result<(), TelemetryError> {
    subscriber(rust_log, std::io::stdout)
        .map_err(|_| TelemetryError::Filter)?
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
        tracing::subscriber::with_default(subscriber(rust_log, out.clone()).unwrap(), emit);
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
        assert_eq!(line["level"], "info");
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
        assert_eq!(lines[0]["level"], "warn");
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
        assert_eq!(lines[0]["level"], "info");
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

    #[test]
    fn level_is_lowercase() {
        let lines = capture("trace", || {
            tracing::error!(target: "server", event = "x");
            tracing::warn!(target: "server", event = "x");
            tracing::info!(target: "server", event = "x");
            tracing::debug!(target: "server", event = "x");
            tracing::trace!(target: "server", event = "x");
            tracing::warn!(target: "sqlx::pool", "x");
        });
        let levels: Vec<&str> = lines.iter().map(|l| l["level"].as_str().unwrap()).collect();
        assert_eq!(levels, ["error", "warn", "info", "debug", "trace", "warn"]);
    }

    #[derive(Debug)]
    struct Canary;

    impl fmt::Display for Canary {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("canary-display")
        }
    }

    impl std::error::Error for Canary {}

    #[test]
    fn every_allowed_field_has_a_kind() {
        for name in ALLOWED_FIELDS {
            assert!(field_kind(name).is_some(), "{name}");
        }
        assert_eq!(field_kind("message"), None);
        assert_eq!(field_kind("ts"), None);
        assert_eq!(field_kind("level"), None);
    }

    #[test]
    fn fields_accept_only_their_type() {
        let error = Canary;
        let lines = capture("info", || {
            tracing::info!(
                target: "server",
                event = "x",
                error_kind = ?error,
                route = %error,
                var = &error as &(dyn std::error::Error + 'static),
                method = &b"canary-bytes"[..],
                file = true,
                status = "200",
                latency_ms = 1.5f64,
                port = 8080u128,
                close_code = -1i128,
                line = false,
            );
        });
        assert_eq!(lines.len(), 1);
        let mut found = keys(&lines[0]);
        found.sort_unstable();
        assert_eq!(found, ["event", "level", "ts"], "{:?}", lines[0]);
        assert!(!serde_json::to_string(&lines[0]).unwrap().contains("canary"));
    }

    #[test]
    fn fields_keep_their_type() {
        let lines = capture("info", || {
            tracing::info!(
                target: "server",
                event = "x",
                method = "GET",
                route = "/api/health",
                error_kind = "timeout",
                var = "PORT",
                file = "src/x.rs",
                status = 200u16,
                latency_ms = 3u64,
                port = 8080u16,
                close_code = 1008i64,
                line = 7u32,
                request_id = %uuid::Uuid::nil(),
            );
        });
        let line = &lines[0];
        assert_eq!(line.len(), ALLOWED_FIELDS.len() + 2, "{line:?}");
        assert_eq!(line["close_code"], 1008);
        assert_eq!(line["line"], 7);
        assert_eq!(line["request_id"], uuid::Uuid::nil().to_string());
    }

    #[test]
    fn request_id_must_be_a_uuid() {
        let lines = capture("info", || {
            tracing::info!(target: "server", event = "x", request_id = %Canary);
            tracing::info!(target: "server", event = "x", request_id = "canary");
        });
        for line in &lines {
            assert!(!line.contains_key("request_id"), "{line:?}");
        }
    }

    #[tracing::instrument(skip_all, target = "server", fields(route = canary, event = "span"))]
    fn instrumented(canary: &str) {
        tracing::Span::current().record("error_kind", canary);
        tracing::info!(target: "server", event = "inside");
    }

    #[test]
    fn span_fields_are_never_written() {
        let canary = "canary-span";
        let lines = capture("trace", || {
            let span = tracing::info_span!(
                target: "server",
                "work",
                route = canary,
                error_kind = tracing::field::Empty,
            );
            let _guard = span.enter();
            span.record("error_kind", canary);
            tracing::info!(target: "server", event = "inside");
            instrumented(canary);
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        for line in &lines {
            let mut found = keys(line);
            found.sort_unstable();
            assert_eq!(found, ["event", "level", "ts"], "{line:?}");
            assert_eq!(line["event"], "inside");
        }
    }

    #[test]
    fn the_subscriber_writes_each_event_once() {
        let lines = capture("info", || tracing::info!(target: "server", event = "x"));
        assert_eq!(lines.len(), 1, "{lines:?}");
    }

    /// The `instrument` attributes of `source` without `skip_all` (A3). Comment lines are
    /// ignored.
    fn instrument_without_skip_all(source: &str) -> Vec<String> {
        let code = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut found = Vec::new();
        let mut rest = code.as_str();
        while let Some(start) = rest.find("#[") {
            let attr = &rest[start + 2..];
            let mut depth = 1;
            let mut end = attr.len();
            for (i, c) in attr.char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body: String = attr[..end].chars().filter(|c| !c.is_whitespace()).collect();
            let is_instrument = body
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|word| word == "instrument");
            if is_instrument && !body.contains("skip_all") {
                found.push(body);
            }
            rest = &attr[end..];
        }
        found
    }

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    fn server_sources() -> Vec<(std::path::PathBuf, String)> {
        let mut files = Vec::new();
        rust_files(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
            &mut files,
        );
        assert!(files.len() > 5, "{files:?}");
        files
            .into_iter()
            .map(|path| {
                let text = std::fs::read_to_string(&path).unwrap();
                (path, text)
            })
            .collect()
    }

    #[test]
    fn the_instrument_check_catches_a_missing_skip_all() {
        // Built with `concat!` so this file's text has no attribute for the check to find.
        let bad = [
            concat!("#", "[instrument]\nfn f(x: u8) {}"),
            concat!(
                "#",
                "[tracing::instrument(fields(route = \"/x\"))] fn f() {}"
            ),
            concat!(
                "#",
                "[ tracing :: instrument (\n  skip(x)\n)]\nfn f(x: u8) {}"
            ),
            concat!("#", "[cfg_attr(test, tracing::instrument(err))] fn f() {}"),
        ];
        for source in bad {
            assert_eq!(instrument_without_skip_all(source).len(), 1, "{source}");
        }
        let good = [
            concat!(
                "#",
                "[tracing::instrument(skip_all, fields(route = \"/x\"))] fn f() {}"
            ),
            concat!("// #", "[instrument]\n#[derive(Debug)] struct S;"),
            concat!("#", "[doc = \"instrumented\"] fn f() {}"),
        ];
        for source in good {
            assert!(instrument_without_skip_all(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn every_instrument_in_the_server_skips_all() {
        for (path, text) in server_sources() {
            let found = instrument_without_skip_all(&text);
            assert!(found.is_empty(), "{}: {found:?}", path.display());
        }
    }

    /// The code of `text` before its test module.
    fn non_test_code(text: &str) -> &str {
        text.split("#[cfg(test)]").next().unwrap_or(text)
    }

    #[test]
    fn telemetry_layer_is_the_only_output_layer() {
        // Outside this module, nothing builds or installs a subscriber or a layer.
        let forbidden = [
            "tracing_subscriber::fmt",
            "tracing_subscriber::registry",
            "tracing_subscriber::layer",
            "tracing_subscriber::Layer",
            "tracing_subscriber::util",
            "set_global_default",
            "with_default",
            "SubscriberExt",
            "SubscriberInitExt",
        ];
        for (path, text) in server_sources() {
            if path.ends_with("src/telemetry.rs") {
                continue;
            }
            for token in forbidden {
                assert!(!text.contains(token), "{token} in {}", path.display());
            }
        }
        // Here: one output layer, one registry, and `init` installs `subscriber`.
        let own = non_test_code(include_str!("telemetry.rs"));
        assert_eq!(own.matches("fmt::layer(").count(), 1);
        assert_eq!(own.matches("registry(").count(), 1);
        assert_eq!(own.matches(".with(").count(), 1);
        assert_eq!(own.matches("try_init(").count(), 1);
        assert!(own.contains("subscriber(rust_log, std::io::stdout)"));
        // No crate with its own output layer.
        let manifest = include_str!("../Cargo.toml");
        for line in manifest.lines() {
            let name = line.split(['=', '.']).next().unwrap_or("").trim();
            if name.starts_with("tracing") {
                assert!(["tracing", "tracing-subscriber"].contains(&name), "{name}");
            }
            for banned in [
                "opentelemetry",
                "console-subscriber",
                "journald",
                "appender",
            ] {
                assert!(!name.contains(banned), "{name}");
            }
        }
    }
}
