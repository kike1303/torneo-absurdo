//! `GET /api/leaderboard?season=` and `GET /api/seasons` (020): public, read-only, with the limit
//! and the cache of the list of fights (`FIGHT_READ_IP`, `no-store`).

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use protocol::{Leaderboard, SeasonErrorCode};
use serde::Deserialize;

use super::{SEASON_MIN_FIGHTS, board};
use crate::app::AppState;
use crate::db::{ACQUIRE_TIMEOUT, error_kind};
use crate::http::errors::error_response;

/// A season id as a query value: exactly `YYYYMM`, a year from 2000 and a month from 01 to 12.
/// Anything else is not a season, without asking the database (020, security review 12).
#[must_use]
pub fn parse_season(text: &str) -> Option<i32> {
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let id: i32 = text.parse().ok()?;
    let (year, month) = (id / 100, id % 100);
    ((2000..=9999).contains(&year) && (1..=12).contains(&month)).then_some(id)
}

#[derive(Deserialize)]
pub struct BoardQuery {
    season: Option<String>,
}

fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, SeasonErrorCode::NotFound)
}

fn unavailable(error: &sqlx::Error, event: &'static str) -> Response {
    tracing::warn!(event, error_kind = error_kind(error));
    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        SeasonErrorCode::Unavailable,
    )
}

/// The same bound as the reads of the fights.
async fn bounded<T>(read: impl Future<Output = Result<T, sqlx::Error>>) -> Result<T, sqlx::Error> {
    tokio::time::timeout(ACQUIRE_TIMEOUT.saturating_mul(3), read)
        .await
        .unwrap_or(Err(sqlx::Error::PoolTimedOut))
}

/// `GET /api/leaderboard`. Without `season`, the season of this moment, even before its first
/// settled fight; a `season` that is not `YYYYMM`, repeated, or without a row is a 404.
pub async fn leaderboard(
    State(state): State<AppState>,
    query: Result<Query<BoardQuery>, QueryRejection>,
) -> Response {
    let wanted = match query {
        Ok(Query(BoardQuery { season: None })) => None,
        Ok(Query(BoardQuery { season: Some(text) })) => match parse_season(&text) {
            Some(id) => Some(id),
            None => return not_found(),
        },
        Err(_) => return not_found(),
    };
    let read = bounded(async {
        let mut conn = state.db.acquire().await?;
        let season = match wanted {
            Some(id) => match board::season(&mut conn, id).await? {
                Some(season) => season,
                None => return Ok(None),
            },
            None => board::current(&mut conn).await?,
        };
        let id = i32::try_from(season.id).map_err(|_| sqlx::Error::Protocol("season".into()))?;
        let entries = board::entries(&mut conn, id).await?;
        Ok(Some(Leaderboard {
            season,
            entries,
            min_fights: u16::try_from(SEASON_MIN_FIGHTS).unwrap_or(u16::MAX),
        }))
    })
    .await;
    match read {
        Ok(Some(board)) => Json(board).into_response(),
        Ok(None) => not_found(),
        Err(error) => unavailable(&error, "seasons.leaderboard_failed"),
    }
}

/// `GET /api/seasons`: the seasons with a row, the newest first.
pub async fn list(State(state): State<AppState>) -> Response {
    let read = bounded(async {
        let mut conn = state.db.acquire().await?;
        board::seasons(&mut conn).await
    })
    .await;
    match read {
        Ok(seasons) => Json(seasons).into_response(),
        Err(error) => unavailable(&error, "seasons.list_failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_season;

    #[test]
    fn a_season_has_one_form() {
        assert_eq!(parse_season("202611"), Some(202_611));
        assert_eq!(parse_season("200001"), Some(200_001));
        assert_eq!(parse_season("999912"), Some(999_912));
        for bad in [
            "",
            "2026",
            "20261",
            "2026111",
            "202600",
            "202613",
            "199912",
            "+20261",
            "2026-1",
            " 202611",
            "２０２６１１",
        ] {
            assert_eq!(parse_season(bad), None, "{bad}");
        }
    }
}
