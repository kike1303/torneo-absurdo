//! Runs the real `server` binary with a clean environment (only `PATH` plus the variables
//! of each test) and captures stdout and stderr line by line.

use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

#[derive(Default)]
struct Output {
    stdout: Mutex<Vec<String>>,
    stderr: Mutex<Vec<String>>,
    changed: Notify,
}

pub struct ServerProcess {
    child: Child,
    output: Arc<Output>,
    readers: Vec<JoinHandle<()>>,
}

pub struct Finished {
    pub status: ExitStatus,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
}

fn read_lines(
    stream: impl AsyncRead + Unpin + Send + 'static,
    output: Arc<Output>,
    stdout: bool,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let target = if stdout {
                &output.stdout
            } else {
                &output.stderr
            };
            target.lock().unwrap().push(line);
            output.changed.notify_waiters();
        }
        output.changed.notify_waiters();
    })
}

/// Runs the binary once and waits for it. On macOS the first exec of a freshly linked binary
/// is scanned by the system (about half a second), which would count against timing tests.
pub async fn warm_up() {
    static WARM: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    WARM.get_or_init(|| async {
        let _ = Command::new(env!("CARGO_BIN_EXE_server"))
            .arg("--warm-up")
            .env_clear()
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    })
    .await;
}

/// Variables every server run needs since phase B (008 RF-B18) and, outside loopback, phase C
/// (the OAuth credentials, RF-C1), with test-only values. The variables of each test go after
/// them, so a test can override or check any of them. With `dev-auth`, the gate needs a
/// loopback `BIND_ADDR`.
pub const DEFAULT_ENV: &[(&str, &str)] = &[
    ("INTERNAL_API_SECRET", super::app::SECRET),
    ("IP_HASH_KEY", super::app::IP_HASH_KEY),
    ("DAILY_GRANT", "100"),
    super::config::PROVIDER_VARS[0],
    super::config::PROVIDER_VARS[1],
    super::config::PROVIDER_VARS[2],
    super::config::PROVIDER_VARS[3],
    #[cfg(feature = "dev-auth")]
    ("BIND_ADDR", "127.0.0.1"),
];

pub fn spawn(args: &[&str], envs: &[(&str, &str)]) -> ServerProcess {
    let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
    command
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .envs(DEFAULT_ENV.iter().copied())
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    let output = Arc::new(Output::default());
    let readers = vec![
        read_lines(child.stdout.take().unwrap(), output.clone(), true),
        read_lines(child.stderr.take().unwrap(), output.clone(), false),
    ];
    ServerProcess {
        child,
        output,
        readers,
    }
}

impl ServerProcess {
    pub fn pid(&self) -> u32 {
        self.child.id().unwrap()
    }

    pub fn stdout(&self) -> Vec<String> {
        self.output.stdout.lock().unwrap().clone()
    }

    pub fn stderr(&self) -> Vec<String> {
        self.output.stderr.lock().unwrap().clone()
    }

    /// Waits until a stdout line contains `needle`, up to `timeout`.
    pub async fn wait_for_stdout(&self, needle: &str, timeout: Duration) -> Option<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.output.changed.notified();
            if let Some(line) = self.stdout().into_iter().find(|l| l.contains(needle)) {
                return Some(line);
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return None;
            }
        }
    }

    /// Waits for `server.listening` and returns the port it logged.
    pub async fn wait_listening(&self, timeout: Duration) -> u16 {
        let line = self
            .wait_for_stdout("\"server.listening\"", timeout)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "no server.listening; stdout: {:?}; stderr: {:?}",
                    self.stdout(),
                    self.stderr()
                )
            });
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        u16::try_from(value["port"].as_u64().unwrap()).unwrap()
    }

    /// Sends a signal (`TERM`, `INT`, …) with `kill`.
    pub fn signal(&self, name: &str) {
        let status = std::process::Command::new("kill")
            .args(["-s", name, &self.pid().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Waits for the process to exit (killing it after `timeout`) and collects the output.
    pub async fn finish(mut self, timeout: Duration) -> Finished {
        let status = match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(status) => status.unwrap(),
            Err(_) => {
                self.child.kill().await.ok();
                self.child.wait().await.unwrap()
            }
        };
        for reader in self.readers.drain(..) {
            let _ = reader.await;
        }
        Finished {
            status,
            stdout: self.stdout(),
            stderr: self.stderr(),
        }
    }

    /// Stops the process right away and collects the output.
    pub async fn kill(mut self) -> Finished {
        self.child.start_kill().ok();
        self.finish(Duration::from_secs(5)).await
    }
}
