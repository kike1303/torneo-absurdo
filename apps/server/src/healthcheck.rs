//! `server healthcheck` (RF-B11). Implemented in step 8.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the health check failed")]
pub struct HealthcheckFailed;

pub fn run_from_env() -> Result<(), HealthcheckFailed> {
    Err(HealthcheckFailed)
}
