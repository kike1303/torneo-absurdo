//! HTTP router.

pub mod health;

use axum::Router;
use axum::routing::get;

use crate::app::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health::handler))
        .with_state(state)
}
