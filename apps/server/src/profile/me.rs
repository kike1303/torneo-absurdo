//! `GET /api/me` (008 RF-B12): the user's own profile. No `user_id`, no session id and nothing
//! from the provider.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use protocol::{Locale, MAX_SAFE_INTEGER, Me, Provider, Role, Theme};
use sqlx::Row;

use crate::app::AppState;
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::ledger;

pub async fn handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(info) = session::current(&state, &headers).await else {
        return session::unauthenticated(&state);
    };
    match me(&state, info.user_id).await {
        Ok(Some(me)) => ([(header::CACHE_CONTROL, PRIVATE_NO_STORE)], Json(me)).into_response(),
        Ok(None) => session::unauthenticated(&state),
        Err(()) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CACHE_CONTROL, PRIVATE_NO_STORE)],
        )
            .into_response(),
    }
}

/// The balance as the protocol's `number` (008 plan-b question 2): never truncated, and never
/// above what a JavaScript number holds exactly.
pub fn wire_balance(coins: ledger::Coins) -> Option<i64> {
    // `unsigned_abs`, not `abs`: `i64::MIN` has no positive counterpart.
    (coins.0.unsigned_abs() <= MAX_SAFE_INTEGER.unsigned_abs()).then_some(coins.0)
}

async fn me(state: &AppState, user_id: uuid::Uuid) -> Result<Option<Me>, ()> {
    let row = sqlx::query(
        "SELECT display_name, role, locale, theme, display_name_is_generated \
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let providers =
        sqlx::query("SELECT provider FROM oauth_accounts WHERE user_id = $1 ORDER BY provider")
            .bind(user_id)
            .fetch_all(&state.db)
            .await
            .map_err(|_| ())?;
    let balance = ledger::balance(&state.db, user_id).await.map_err(|_| ())?;
    let text = |i: usize| row.try_get::<Option<String>, _>(i).map_err(|_| ());
    Ok(Some(Me {
        display_name: row.try_get(0).map_err(|_| ())?,
        role: if row.try_get::<&str, _>(1).map_err(|_| ())? == "admin" {
            Role::Admin
        } else {
            Role::User
        },
        balance: wire_balance(balance).ok_or(())?,
        providers: providers
            .iter()
            .filter_map(|r| r.try_get::<&str, _>(0).ok().and_then(Provider::parse))
            .collect(),
        locale: text(2)?.as_deref().and_then(|l| match l {
            "en" => Some(Locale::En),
            "es" => Some(Locale::Es),
            _ => None,
        }),
        theme: text(3)?.as_deref().and_then(|t| match t {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "system" => Some(Theme::System),
            _ => None,
        }),
        display_name_is_generated: row.try_get(4).map_err(|_| ())?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_balance_guard_never_truncates() {
        assert_eq!(wire_balance(ledger::Coins(100)), Some(100));
        assert_eq!(
            wire_balance(ledger::Coins(MAX_SAFE_INTEGER)),
            Some(MAX_SAFE_INTEGER)
        );
        assert_eq!(
            wire_balance(ledger::Coins(-MAX_SAFE_INTEGER)),
            Some(-MAX_SAFE_INTEGER)
        );
        assert_eq!(wire_balance(ledger::Coins(MAX_SAFE_INTEGER + 1)), None);
        assert_eq!(wire_balance(ledger::Coins(i64::MIN)), None);
    }
}
