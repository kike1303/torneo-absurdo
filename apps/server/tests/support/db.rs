//! One database per test (plan-b decision 18).
//!
//! Guards: the admin `DATABASE_URL` must point to a loopback host (a real database in the
//! shell never receives a `CREATE`/`DROP DATABASE`), and only the database this object
//! created, validated against `^torneo_test_[0-9a-f]{32}$`, is ever dropped.

use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;

use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection};
use url::Url;

const PREFIX: &str = "torneo_test_";

pub struct TestDb {
    name: String,
    admin_url: Url,
    url: Url,
}

pub fn is_test_name(name: &str) -> bool {
    name.strip_prefix(PREFIX).is_some_and(|hex| {
        hex.len() == 32
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn admin_url() -> Url {
    let raw = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (the just recipes set it to the dev Postgres)");
    let url = Url::parse(&raw).expect("DATABASE_URL must be a URL");
    let host = url.host_str().unwrap_or_default();
    assert!(
        matches!(host, "127.0.0.1" | "[::1]" | "::1" | "localhost"),
        "refusing to create test databases on a non-loopback DATABASE_URL host"
    );
    url
}

impl TestDb {
    pub async fn create() -> Self {
        let admin_url = admin_url();
        let name = format!("{PREFIX}{}", uuid::Uuid::new_v4().simple());
        assert!(is_test_name(&name));
        let mut conn = PgConnection::connect(admin_url.as_str()).await.unwrap();
        // `name` matched ^torneo_test_[0-9a-f]{32}$ above.
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut conn)
            .await
            .unwrap();
        conn.close().await.ok();
        let mut url = admin_url.clone();
        url.set_path(&format!("/{name}"));
        Self {
            name,
            admin_url,
            url,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// `postgres://…/torneo_test_<uuid>`.
    pub fn url(&self) -> String {
        self.url.to_string()
    }

    /// The same URL through another address (the TCP proxy).
    pub fn url_via(&self, addr: SocketAddr) -> String {
        let mut url = self.url.clone();
        url.set_host(Some(&addr.ip().to_string())).unwrap();
        url.set_port(Some(addr.port())).unwrap();
        url.to_string()
    }

    /// A pool on this database with the server migrations applied.
    pub async fn migrated_pool(&self) -> sqlx::PgPool {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect_with(self.options())
            .await
            .unwrap();
        server::db::migrate(&pool).await.unwrap();
        pool
    }

    pub fn options(&self) -> PgConnectOptions {
        PgConnectOptions::from_str(self.url.as_str()).unwrap()
    }

    pub fn options_via(&self, addr: SocketAddr) -> PgConnectOptions {
        PgConnectOptions::from_str(&self.url_via(addr)).unwrap()
    }

    /// The Postgres server address (target of the proxy).
    pub fn server_addr(&self) -> SocketAddr {
        let host = self.url.host_str().unwrap().trim_matches(['[', ']']);
        let port = self.url.port().unwrap_or(5432);
        (host, port)
            .to_socket_addrs()
            .unwrap()
            .find(SocketAddr::is_ipv4)
            .or_else(|| (host, port).to_socket_addrs().unwrap().next())
            .unwrap()
    }
}

// The drop blocks the thread it runs on. A test that starts the app on this database must use a
// multi-thread runtime (`#[tokio::test(flavor = "multi_thread")]`): on a single thread, a
// connection the app is still opening (its revocation listener, its retention job) cannot move
// while the drop waits, Postgres keeps that half-authenticated backend until
// `authentication_timeout` (1 min), and `DROP DATABASE … WITH (FORCE)` waits for it.
impl Drop for TestDb {
    fn drop(&mut self) {
        if !is_test_name(&self.name) {
            return;
        }
        let name = self.name.clone();
        let admin = self.admin_url.to_string();
        // Drop runs outside any runtime guarantee: use a thread with its own runtime.
        let _ = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async move {
                if let Ok(mut conn) = PgConnection::connect(&admin).await {
                    // `name` matched ^torneo_test_[0-9a-f]{32}$ (checked at the top of drop).
                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                        "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
                    )))
                    .execute(&mut conn)
                    .await;
                    let _ = conn.close().await;
                }
            });
        })
        .join();
    }
}

/// Runs `query` the way the admin CLI writes `role` and `banned_at` (008 plan-c decision 10): in
/// its own transaction with `torneo.actor = 'cli'` set **locally**, so `users_privileged_guard`
/// lets it through and the actor dies with the commit. The only place of `tests/` that sets it;
/// never a session-wide `SET` (a pooled connection would keep the guard off for the next
/// requests of the test).
pub async fn privileged(
    pool: &sqlx::PgPool,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> sqlx::postgres::PgQueryResult {
    let mut tx = begin_privileged(pool).await;
    let done = query.execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    done
}

/// An open transaction with the CLI's actor set locally, for a test that has to hold a
/// privileged write uncommitted (the ban that commits while a request waits on its lock).
pub async fn begin_privileged(pool: &sqlx::PgPool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('torneo.actor', 'cli', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}
