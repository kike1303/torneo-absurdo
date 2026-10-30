//! The server's real log layer (`telemetry::layer`, the JSON lines of stdout) in process, for the
//! tests that need the fake providers (their URLs are only injectable by `Settings`, never in
//! the binary). A multi-thread runtime with the dispatcher installed on every one of its threads,
//! so the events of the app's tasks are captured too, whatever thread runs them.

use std::future::Future;
use std::io;
use std::sync::{Arc, Mutex};

use tracing::Dispatch;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'w> MakeWriter<'w> for Captured {
    type Writer = Captured;
    fn make_writer(&'w self) -> Self::Writer {
        self.clone()
    }
}

impl Captured {
    /// Everything written so far, as text.
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }

    /// Every line, parsed.
    pub fn lines(&self) -> Vec<serde_json::Value> {
        self.text()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The lines with `event == name`.
    pub fn events(&self, name: &str) -> Vec<serde_json::Value> {
        self.lines()
            .into_iter()
            .filter(|l| l["event"] == name)
            .collect()
    }
}

/// Runs `test` on a fresh multi-thread runtime whose threads all log to the returned capture,
/// with the filter `rust_log`.
pub fn run<F, Fut>(rust_log: &str, test: F) -> Captured
where
    F: FnOnce(Captured) -> Fut,
    Fut: Future<Output = ()>,
{
    let captured = Captured::default();
    let layer = server::telemetry::layer(rust_log, captured.clone()).unwrap();
    let dispatch = Dispatch::new(tracing_subscriber::registry().with(layer));
    let on_thread = dispatch.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .on_thread_start(move || {
            // For the life of the thread: the runtime owns it and drops it at the end.
            std::mem::forget(tracing::dispatcher::set_default(&on_thread));
        })
        .build()
        .unwrap();
    {
        let _main = tracing::dispatcher::set_default(&dispatch);
        runtime.block_on(test(captured.clone()));
    }
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    captured
}
