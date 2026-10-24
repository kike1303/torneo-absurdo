//! The server in process, on `127.0.0.1:0`.

use std::net::SocketAddr;

use server::app::{AppState, HttpConfig, serve_listener};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub struct RunningApp {
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

pub async fn start(state: AppState) -> RunningApp {
    start_with(state, HttpConfig::default()).await
}

pub async fn start_with(state: AppState, http: HttpConfig) -> RunningApp {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        serve_listener(listener, state, http, async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    RunningApp {
        addr,
        stop: Some(stop),
        task,
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
    }
}
