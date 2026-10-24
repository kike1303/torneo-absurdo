//! TCP proxy between the server and Postgres that a test can cut and restore
//! (plan-b decision 19).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub struct PgProxy {
    addr: SocketAddr,
    cut: watch::Sender<bool>,
    accepted: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl PgProxy {
    pub async fn start(target: SocketAddr) -> Self {
        Self::start_with(target, false).await
    }

    /// Starts already cut (new connections are closed right away).
    pub async fn start_cut(target: SocketAddr) -> Self {
        Self::start_with(target, true).await
    }

    async fn start_with(target: SocketAddr, cut: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = watch::channel(cut);
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = accepted.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut client, _)) = listener.accept().await else {
                    return;
                };
                if *rx.borrow() {
                    drop(client);
                    continue;
                }
                counter.fetch_add(1, Ordering::SeqCst);
                let mut rx = rx.clone();
                tokio::spawn(async move {
                    let Ok(mut upstream) = TcpStream::connect(target).await else {
                        return;
                    };
                    tokio::select! {
                        _ = tokio::io::copy_bidirectional(&mut client, &mut upstream) => {}
                        _ = rx.wait_for(|cut| *cut) => {}
                    }
                });
            }
        });
        Self {
            addr,
            cut: tx,
            accepted,
            task,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Closes every live connection and refuses new ones.
    pub fn cut(&self) {
        self.cut.send_replace(true);
    }

    pub fn restore(&self) {
        self.cut.send_replace(false);
    }

    /// Connections forwarded so far.
    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}

impl Drop for PgProxy {
    fn drop(&mut self) {
        self.cut.send_replace(true);
        self.task.abort();
    }
}
