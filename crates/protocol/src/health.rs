use serde::{Deserialize, Serialize};

/// Body of `GET /api/health` (RF-B10): 200 with `ok`, 503 with `db_unavailable`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HealthResponse {
    /// The server and its database answer. `version` is the build's `APP_VERSION`.
    Ok { version: String },
    /// The database did not answer `SELECT 1` in time.
    DbUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_has_the_exact_shape() {
        let value = HealthResponse::Ok {
            version: "dev".to_owned(),
        };
        let json = r#"{"status":"ok","version":"dev"}"#;
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        assert_eq!(serde_json::from_str::<HealthResponse>(json).unwrap(), value);
    }

    #[test]
    fn db_unavailable_has_the_exact_shape() {
        let json = r#"{"status":"db_unavailable"}"#;
        assert_eq!(
            serde_json::to_string(&HealthResponse::DbUnavailable).unwrap(),
            json
        );
        assert_eq!(
            serde_json::from_str::<HealthResponse>(json).unwrap(),
            HealthResponse::DbUnavailable
        );
    }

    #[test]
    fn ok_without_version_does_not_deserialize() {
        assert!(serde_json::from_str::<HealthResponse>(r#"{"status":"ok"}"#).is_err());
    }
}
