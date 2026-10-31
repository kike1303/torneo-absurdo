//! The read API (RF34, RF35, RF36). Nothing here mutates, and there is no way to cancel, void or
//! read a seed before `Settled` (RF16).

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{FightDetail, FightErrorCode, FightState};

use super::state::FightRow;
use super::{hex, store};
use crate::app::AppState;
use crate::db::ACQUIRE_TIMEOUT;
use crate::http::errors::error_response;

const NO_STORE: HeaderValue = HeaderValue::from_static("no-store");

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, NO_STORE);
    response
}

/// An id is a canonical decimal from 1 to `i32::MAX`: no sign, no leading zeros, no spaces.
fn parse_id(segment: &str) -> Option<i32> {
    let canonical = segment
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_digit() && b != b'0')
        && segment.bytes().all(|b| b.is_ascii_digit());
    if !canonical {
        return None;
    }
    segment.parse().ok()
}

/// The only place that builds a [`FightDetail`]: `seed` and `log` only with a settled row, and a
/// field whose bytes are not UTF-8 goes `null`, without hiding the rest, never through a lossy
/// conversion (RF34, amendment DP-10).
pub fn detail_of(row: FightRow) -> Option<FightDetail> {
    let fight = row.summary()?;
    let settled = row.state == FightState::Settled;
    Some(FightDetail {
        fight,
        spec_a: String::from_utf8(row.spec_a).ok(),
        spec_b: String::from_utf8(row.spec_b).ok(),
        seed: row.revealed_seed.as_deref().filter(|_| settled).map(hex),
        log: row
            .log
            .filter(|_| settled)
            .and_then(|log| String::from_utf8(log).ok()),
    })
}

/// `GET /api/fights/{id}`.
pub async fn detail(
    State(state): State<AppState>,
    segment: Result<Path<String>, PathRejection>,
) -> Response {
    let id = segment.ok().and_then(|Path(segment)| parse_id(&segment));
    let Some(id) = id else {
        return no_store(error_response(
            StatusCode::NOT_FOUND,
            FightErrorCode::NotFound,
        ));
    };
    // The lock of the query, and everything else, including a hung network: never unbounded.
    let lock_timeout_ms = u64::try_from(ACQUIRE_TIMEOUT.as_millis()).unwrap_or(u64::MAX);
    let read = tokio::time::timeout(
        ACQUIRE_TIMEOUT.saturating_mul(3),
        store::by_id_bounded(&state.db, id, lock_timeout_ms),
    );
    let answer = match read.await.unwrap_or(Err(sqlx::Error::PoolTimedOut)) {
        Ok(Some(row)) => match detail_of(row) {
            Some(detail) => Json(detail).into_response(),
            None => error_response(StatusCode::SERVICE_UNAVAILABLE, FightErrorCode::Unavailable),
        },
        Ok(None) => error_response(StatusCode::NOT_FOUND, FightErrorCode::NotFound),
        Err(_) => error_response(StatusCode::SERVICE_UNAVAILABLE, FightErrorCode::Unavailable),
    };
    no_store(answer)
}

/// `GET /api/fights/current`: from memory, without touching the database and with no limit of its
/// own. Before the runner has loaded, everything is `null`, with a 200.
pub async fn current(State(state): State<AppState>) -> Response {
    no_store(Json(state.fights.current()).into_response())
}

#[cfg(test)]
mod tests {
    use super::parse_id;

    #[test]
    fn an_id_is_a_canonical_decimal_in_range() {
        assert_eq!(parse_id("1"), Some(1));
        assert_eq!(parse_id("12"), Some(12));
        assert_eq!(parse_id("2147483647"), Some(i32::MAX));
        for text in [
            "",
            "0",
            "01",
            "+1",
            "-1",
            " 1",
            "1 ",
            "1.0",
            "0x1",
            "2147483648",
            "99999999999999",
            "uno",
            "١",
        ] {
            assert_eq!(parse_id(text), None, "{text:?}");
        }
    }
}
