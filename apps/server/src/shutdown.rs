//! Shutdown signals (RF-B14, plan-b decision 11).
//!
//! `app::serve` turns the signal into a `CancellationToken`: the listener stops accepting,
//! every WebSocket closes with 1012 (each socket holds a `TaskTrackerToken` taken in the
//! handshake), and the drain is bounded by `app::SHUTDOWN_DEADLINE`.

/// Resolves on `SIGTERM` or `SIGINT`.
pub async fn signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        // Without signal handlers the process can only be killed; never resolve.
        std::future::pending::<()>().await;
        return;
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}
