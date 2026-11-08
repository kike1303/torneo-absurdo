//! The read API (RF34, RF35, RF36, 011 RF7). Nothing here mutates, and there is no way to cancel,
//! void or read a seed before `Settled` (RF16).

use axum::Json;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{FightDetail, FightErrorCode, FightState, FightSummary, FightsPage};
use serde::Deserialize;

use super::state::FightRow;
use super::store::{PAGE_SIZE, PageRow};
use super::{hex, store};
use crate::app::AppState;
use crate::db::{ACQUIRE_TIMEOUT, error_kind};
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

/// The summaries of the rows that have one. A row that does not decode, or without a summary, is
/// left out without taking the page down (011 RF8): only the kind of error, `ALLOWED_FIELDS` has no
/// `fight_id`.
fn summaries_of(rows: impl IntoIterator<Item = PageRow>) -> Vec<FightSummary> {
    rows.into_iter()
        .filter_map(|page_row| match page_row.row {
            Ok(row) => {
                let summary = row.summary();
                if summary.is_none() {
                    tracing::warn!(event = "fights.list_row_invalid", error_kind = "bad_row");
                }
                summary
            }
            Err(error) => {
                let kind = error_kind(&error);
                tracing::warn!(event = "fights.list_row_invalid", error_kind = kind);
                None
            }
        })
        .collect()
}

/// A page from what was read: `next_before` comes from the rows read, not from those shown, so a row
/// left out at the end of a page does not lose the rest of the list (011 RF8).
fn page_of(mut rows: Vec<PageRow>) -> FightsPage {
    let next_before = (rows.len() > PAGE_SIZE)
        .then(|| rows.get(PAGE_SIZE - 1))
        .flatten()
        .and_then(|row| u32::try_from(row.id).ok());
    rows.truncate(PAGE_SIZE);
    FightsPage {
        fights: summaries_of(rows),
        next_before,
    }
}

/// The query of `GET /api/fights`. The size of a page is fixed: there is no `limit` (011 RF7).
#[derive(Deserialize)]
pub struct ListQuery {
    before: Option<String>,
}

/// The `before` of a query, `None` if it is not one: not an id, or repeated.
fn before_of(query: Result<Query<ListQuery>, QueryRejection>) -> Option<Option<i32>> {
    let Query(ListQuery { before }) = query.ok()?;
    match before {
        Some(text) => parse_id(&text).map(Some),
        None => Some(None),
    }
}

/// `GET /api/fights`. A `before` that is not an id is the same 404 as a path that is not one
/// (011 RF7).
pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    let Some(before) = before_of(query) else {
        return no_store(error_response(
            StatusCode::NOT_FOUND,
            FightErrorCode::NotFound,
        ));
    };
    // The same bounds as the detail.
    let lock_timeout_ms = u64::try_from(ACQUIRE_TIMEOUT.as_millis()).unwrap_or(u64::MAX);
    let read = tokio::time::timeout(
        ACQUIRE_TIMEOUT.saturating_mul(3),
        store::settled_page(&state.db, before, lock_timeout_ms),
    );
    let answer = match read.await.unwrap_or(Err(sqlx::Error::PoolTimedOut)) {
        Ok(rows) => Json(page_of(rows)).into_response(),
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
    use protocol::FightState;

    use super::store::{BadFightRow, PAGE_SIZE, PageRow};
    use super::{page_of, parse_id, summaries_of};
    use crate::fights::state::FightRow;
    use crate::fights::state::tests::row;

    /// A settled row with a final, and so with a summary.
    fn cancelled(id: i32) -> FightRow {
        FightRow {
            id,
            cancel_reason: Some("missed_window".to_owned()),
            ..row(FightState::Settled)
        }
    }

    fn good(id: i32) -> PageRow {
        PageRow {
            id,
            row: Ok(cancelled(id)),
        }
    }

    fn undecodable(id: i32) -> PageRow {
        PageRow {
            id,
            row: Err(sqlx::Error::Decode(Box::new(BadFightRow))),
        }
    }

    /// Settled, and a final the `CHECK`s of the table forbid: it has no summary.
    fn without_summary(id: i32) -> PageRow {
        PageRow {
            id,
            row: Ok(FightRow {
                cancel_reason: Some("other".to_owned()),
                ..cancelled(id)
            }),
        }
    }

    /// The rows the query brings: from `first` downwards.
    fn read(first: i32, count: i32) -> Vec<PageRow> {
        (0..count).map(|i| good(first - i)).collect()
    }

    fn ids(rows: Vec<PageRow>) -> Vec<u32> {
        summaries_of(rows).iter().map(|fight| fight.id).collect()
    }

    #[test]
    fn a_row_that_does_not_decode_or_has_no_summary_is_left_out() {
        let rows = vec![good(9), undecodable(8), without_summary(7), good(6)];
        assert_eq!(ids(rows), [9, 6]);
        assert_eq!(ids(vec![undecodable(1)]), [] as [u32; 0]);
        assert_eq!(ids(vec![]), [] as [u32; 0]);
    }

    #[test]
    fn a_full_read_says_where_the_next_page_starts_and_keeps_the_page_size() {
        let page = page_of(read(100, 21));
        assert_eq!(page.fights.len(), PAGE_SIZE);
        assert_eq!(page.fights.first().map(|fight| fight.id), Some(100));
        assert_eq!(page.fights.last().map(|fight| fight.id), Some(81));
        assert_eq!(page.next_before, Some(81));
    }

    #[test]
    fn exactly_a_page_or_less_is_the_end_of_the_list() {
        assert_eq!(page_of(read(100, 20)).next_before, None);
        assert_eq!(page_of(read(100, 20)).fights.len(), 20);
        assert_eq!(page_of(read(100, 1)).next_before, None);
        assert_eq!(page_of(vec![]).next_before, None);
        assert!(page_of(vec![]).fights.is_empty());
    }

    /// RF8: the cursor comes from the rows read, also when the last one of the page is left out.
    #[test]
    fn a_row_left_out_does_not_lose_the_rest_of_the_list() {
        for bad in [undecodable(81), without_summary(81)] {
            let mut rows = read(100, 21);
            rows[19] = bad;
            let page = page_of(rows);
            assert_eq!(page.fights.len(), 19);
            assert_eq!(page.fights.last().map(|fight| fight.id), Some(82));
            assert_eq!(page.next_before, Some(81));
        }
        // A page in which nothing can be shown still leads to the next one.
        let rows: Vec<PageRow> = (0..21).map(|i| undecodable(100 - i)).collect();
        let page = page_of(rows);
        assert!(page.fights.is_empty());
        assert_eq!(page.next_before, Some(81));
    }

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
