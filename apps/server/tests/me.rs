//! 008 RF-B9 and RF-B12 with the production cookie policy (the binary without `dev-auth`).
#![cfg(not(feature = "dev-auth"))]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::app::Settings;
use server::sessions::{self, UaFamily};
use support::db::TestDb;
use support::http;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_production_cookie_is_read() {
    let db = TestDb::create().await;
    let (app, pool) = support::app::start_db(&db, &[], &Settings::default()).await;
    let user = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, display_name, display_name_key) VALUES ($1, 'Ana Bel', 'ana bel')",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    server::ledger::create_user_account(&mut tx, user)
        .await
        .unwrap();
    let issued = sessions::issue(
        &mut tx,
        user,
        protocol::Role::User,
        sessions::Authentication::None,
        &[0; 32],
        UaFamily::Other,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let token = issued.token.as_str();

    let ok = http::send(
        app.addr,
        "GET",
        "/api/me",
        &[("cookie", &format!("__Host-session={token}"))],
    )
    .await;
    assert_eq!(ok.status, 200, "{}", ok.text());
    assert_eq!(
        ok.text(),
        r#"{"display_name":"Ana Bel","role":"user","balance":0,"providers":[],"locale":null,"theme":null,"display_name_is_generated":true}"#
    );
    for cookie in [
        format!("ta_dev_session={token}"),
        format!("__Host-session={token}; __Host-session={token}"),
        format!("__host-session={token}"),
    ] {
        let response = http::send(app.addr, "GET", "/api/me", &[("cookie", &cookie)]).await;
        assert_eq!(response.status, 401, "{cookie}");
        assert_eq!(
            response.cookie("__Host-session"),
            Some(""),
            "the production cookie is expired"
        );
        assert!(response.set_cookies().iter().all(|l| l.contains("Secure")));
    }
    // No dev route in the production binary.
    let dev = http::get(app.addr, "/api/auth/dev/oauth/userinfo").await;
    assert_eq!(dev.status, 404);
    let providers = http::get(app.addr, "/api/auth/providers").await;
    assert_eq!(providers.text(), "[]");
}
