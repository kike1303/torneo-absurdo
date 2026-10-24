//! `GET /api/health` (RF-B10, plan-b decision 8).
//!
//! The database result is cached for `cache_ttl` and there is at most one query in flight:
//! the query runs with the mutex held, so concurrent requests wait for it and then read the
//! fresh result. A burst of requests can never fill the pool from here.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::HealthResponse;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::db::DbProbe;
use crate::version::APP_VERSION;

#[derive(Debug, Clone, Copy)]
pub struct HealthConfig {
    /// How long a database result is reused.
    pub cache_ttl: Duration,
    /// Timeout of the `SELECT 1`.
    pub db_timeout: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            cache_ttl: Duration::from_secs(1),
            db_timeout: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbStatus {
    Up,
    Down,
}

pub struct HealthState {
    probe: Arc<dyn DbProbe>,
    config: HealthConfig,
    cache: Mutex<Option<(Instant, DbStatus)>>,
}

impl HealthState {
    pub fn new(probe: Arc<dyn DbProbe>, config: HealthConfig) -> Self {
        Self {
            probe,
            config,
            cache: Mutex::new(None),
        }
    }

    /// The database status, from the cache if it is fresh.
    pub async fn status(&self) -> DbStatus {
        let mut cache = self.cache.lock().await;
        if let Some((at, status)) = *cache
            && at.elapsed() < self.config.cache_ttl
        {
            return status;
        }
        let status = match tokio::time::timeout(self.config.db_timeout, self.probe.ping()).await {
            Ok(true) => DbStatus::Up,
            Ok(false) | Err(_) => DbStatus::Down,
        };
        *cache = Some((Instant::now(), status));
        status
    }
}

pub async fn handler(State(state): State<Arc<HealthState>>) -> Response {
    let (code, body) = match state.status().await {
        DbStatus::Up => (
            StatusCode::OK,
            HealthResponse::Ok {
                version: APP_VERSION.to_owned(),
            },
        ),
        DbStatus::Down => (
            StatusCode::SERVICE_UNAVAILABLE,
            HealthResponse::DbUnavailable,
        ),
    };
    let mut response = (code, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
