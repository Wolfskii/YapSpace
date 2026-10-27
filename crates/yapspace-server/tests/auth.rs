//! Account and session flows: register, log in, log out, recover, change password, sessions.

mod common;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

const PASSWORD: &str = "correct horse battery";

#[sqlx::test]
async fn register_returns_user_session_and_recovery_code(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let (token, user, code) = common::register(&app, "alice_01", PASSWORD).await;
    assert!(token.starts_with("ys_"));
    assert_eq!(user["username"], "alice_01");
    assert_eq!(user["role"], "user");
    assert_eq!(code.len(), 23, "XXXXX-XXXXX-XXXXX-XXXXX");
    assert!(user["created_at"].as_str().unwrap().ends_with('Z'));

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn usernames_are_case_insensitive(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    common::register(&app, "Alice_01", PASSWORD).await;

    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&json!({ "username": "ALICE_01", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "username_taken");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn invalid_username_is_a_field_error(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&json!({ "username": "Bad-Name", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "validation_failed");
    assert_eq!(body["errors"][0]["field"], "username");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn password_equal_to_username_is_rejected(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&json!({ "username": "alice_long_name", "password": "Alice_Long_Name" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["errors"][0]["code"], "matches_username");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn unknown_request_field_is_rejected(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&json!({ "username": "alice_01", "password": PASSWORD, "email": "a@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn registration_can_be_closed_by_config(pool: PgPool) {
    let mut config = common::test_config();
    config.registration_open = false;
    let app = common::spawn_app_with(pool, config).await;

    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&json!({ "username": "alice_01", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn login_succeeds_and_wrong_password_and_unknown_user_look_identical(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    common::register(&app, "alice_01", PASSWORD).await;

    let ok = app
        .http
        .post(app.url("/v1/auth/login"))
        .json(&json!({ "username": "alice_01", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let body: Value = ok.json().await.unwrap();
    assert!(
        body["recovery_code"].is_null(),
        "login does not reveal a recovery code"
    );

    let wrong = app
        .http
        .post(app.url("/v1/auth/login"))
        .json(&json!({ "username": "alice_01", "password": "not the password" }))
        .send()
        .await
        .unwrap();
    let unknown = app
        .http
        .post(app.url("/v1/auth/login"))
        .json(&json!({ "username": "nobody_here", "password": "not the password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    let mut wrong_body: Value = wrong.json().await.unwrap();
    let mut unknown_body: Value = unknown.json().await.unwrap();
    // Only the request id may differ.
    wrong_body.as_object_mut().unwrap().remove("request_id");
    unknown_body.as_object_mut().unwrap().remove("request_id");
    assert_eq!(wrong_body, unknown_body);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn logout_revokes_the_session(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let logout = common::authed(&app, Method::POST, "/v1/auth/logout", &token)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let me = common::authed(&app, Method::GET, "/v1/me", &token)
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::UNAUTHORIZED);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn malformed_bearer_header_is_unauthorized_not_anonymous(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    for header in ["Basic abc", "Bearer nope", "Bearer ys_definitely-not-a-session"] {
        let response = app
            .http
            .get(app.url("/v1/me"))
            .header("authorization", header)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{header}");
    }

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn changing_password_keeps_this_session_and_revokes_others(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (current, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let other = app
        .http
        .post(app.url("/v1/auth/login"))
        .json(&json!({ "username": "alice_01", "password": PASSWORD }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["session_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let change = common::authed(&app, Method::POST, "/v1/me/password", &current)
        .json(&json!({ "current_password": PASSWORD, "new_password": "a brand new secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(change.status(), StatusCode::NO_CONTENT);

    let still_here = common::authed(&app, Method::GET, "/v1/me", &current)
        .send()
        .await
        .unwrap();
    assert_eq!(still_here.status(), StatusCode::OK);
    let gone = common::authed(&app, Method::GET, "/v1/me", &other)
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), StatusCode::UNAUTHORIZED);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn changing_password_needs_the_current_one(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let response = common::authed(&app, Method::POST, "/v1/me/password", &token)
        .json(&json!({ "current_password": "wrong wrong wrong", "new_password": "a brand new secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn recovery_with_the_code_resets_the_password_and_rotates_the_code(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (old_token, _, code) = common::register(&app, "alice_01", PASSWORD).await;

    let wrong = app
        .http
        .post(app.url("/v1/auth/recover"))
        .json(&json!({ "username": "alice_01", "recovery_code": "AAAAA-AAAAA-AAAAA-AAAAA", "new_password": "a brand new secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    // Typed in lowercase and without dashes, the code must still work.
    let typed = code.to_lowercase().replace('-', "");
    let recovered = app
        .http
        .post(app.url("/v1/auth/recover"))
        .json(
            &json!({ "username": "alice_01", "recovery_code": typed, "new_password": "a brand new secret" }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(recovered.status(), StatusCode::OK);
    let body: Value = recovered.json().await.unwrap();
    let new_code = body["recovery_code"].as_str().unwrap().to_owned();
    assert_ne!(new_code, code);

    let old_session = common::authed(&app, Method::GET, "/v1/me", &old_token)
        .send()
        .await
        .unwrap();
    assert_eq!(old_session.status(), StatusCode::UNAUTHORIZED);

    // The old code is used up.
    let reuse = app
        .http
        .post(app.url("/v1/auth/recover"))
        .json(&json!({ "username": "alice_01", "recovery_code": code, "new_password": "yet another secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reuse.status(), StatusCode::UNAUTHORIZED);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn rotating_the_recovery_code_needs_the_password(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, code) = common::register(&app, "alice_01", PASSWORD).await;

    let denied = common::authed(&app, Method::POST, "/v1/me/recovery-code", &token)
        .json(&json!({ "current_password": "nope nope nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let rotated = common::authed(&app, Method::POST, "/v1/me/recovery-code", &token)
        .json(&json!({ "current_password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::OK);
    let new_code = rotated.json::<Value>().await.unwrap()["recovery_code"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(new_code, code);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn sessions_can_be_listed_and_only_own_ones_revoked(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (bob, _, _) = common::register(&app, "bob_0002", PASSWORD).await;

    let list: Value = common::authed(&app, Method::GET, "/v1/me/sessions", &alice)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["current"], true);
    let alice_session = items[0]["id"].as_str().unwrap().to_owned();

    // Bob cannot revoke Alice's session. The response is 404, not 403, so ids cannot be probed.
    let probe = common::authed(
        &app,
        Method::DELETE,
        &format!("/v1/me/sessions/{alice_session}"),
        &bob,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(probe.status(), StatusCode::NOT_FOUND);
    let still = common::authed(&app, Method::GET, "/v1/me", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(still.status(), StatusCode::OK);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn public_profile_hides_role_and_unknown_user_is_404(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    common::register(&app, "alice_01", PASSWORD).await;

    let profile: Value = app.get("/v1/users/Alice_01").await.json().await.unwrap();
    assert_eq!(profile["username"], "alice_01");
    assert!(profile.get("role").is_none());

    assert_eq!(
        app.get("/v1/users/nobody_here").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.get("/v1/users/Bad-Name").await.status(),
        StatusCode::NOT_FOUND
    );

    app.stop().await.unwrap();
}
