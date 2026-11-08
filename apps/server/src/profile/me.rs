//! `GET /api/me` (008 RF-B12): the user's own profile. No `user_id`, no session id and nothing
//! from the provider.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use protocol::{CoinAmount, Locale, Me, Provider, Role, Theme};
use sqlx::Row;

use crate::app::AppState;
use crate::http::session::{self, PRIVATE_NO_STORE};
use crate::ledger;

pub async fn handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let info = match session::current(&state, &headers).await {
        Ok(Some(info)) => info,
        Ok(None) => return session::unauthenticated(&state),
        Err(_) => return session::try_again(),
    };
    match me(&state, info.user_id).await {
        Ok(Some(me)) => ([(header::CACHE_CONTROL, PRIVATE_NO_STORE)], Json(me)).into_response(),
        Ok(None) => session::unauthenticated(&state),
        Err(()) => session::try_again(),
    }
}

pub(crate) async fn me(state: &AppState, user_id: uuid::Uuid) -> Result<Option<Me>, ()> {
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
        // Never truncated, and never above what a JavaScript number holds exactly: the cases are
        // `CoinAmount::new`'s tests in `protocol`.
        balance: CoinAmount::new(balance.0).ok_or(())?,
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
