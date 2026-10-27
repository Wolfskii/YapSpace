//! Rate limits and spam checks. Limits are off in most tests (see common::test_config), so these turn them on
//! explicitly where they are the subject of the test.

mod common;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";

fn enabled_config() -> yapspace_server::config::Config {
    let mut config = common::test_config();
    config.rate_limit_enabled = true;
    config
}

#[sqlx::test]
async fn sixth_registration_from_one_address_is_refused_with_retry_after(pool: PgPool) {
    let app = common::spawn_app_with(pool, enabled_config()).await;

    for i in 0..5 {
        common::register(&app, &format!("user_{i:04}"), PASSWORD).await;
    }
    let refused = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&json!({ "username": "user_9999", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = refused.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(retry_after >= 1);
    let body: Value = refused.json().await.unwrap();
    assert_eq!(body["code"], "rate_limited");
    assert_eq!(body["retry_after_s"], retry_after);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn forwarded_for_from_an_untrusted_peer_cannot_dodge_the_limit(pool: PgPool) {
    let app = common::spawn_app_with(pool, enabled_config()).await;

    for i in 0..5 {
        let response = app
            .http
            .post(app.url("/v1/auth/register"))
            .header("x-forwarded-for", format!("198.51.100.{i}"))
            .json(&json!({ "username": format!("spoof_{i:03}"), "password": PASSWORD }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let sixth = app
        .http
        .post(app.url("/v1/auth/register"))
        .header("x-forwarded-for", "198.51.100.99")
        .json(&json!({ "username": "spoof_999", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(sixth.status(), StatusCode::TOO_MANY_REQUESTS);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn posting_the_same_body_twice_is_refused_as_a_duplicate(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = common::authed(&app, Method::POST, "/v1/discussions", &token)
        .json(&json!({ "url": "https://example.com/dupes" }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("/v1/discussions/{discussion}/messages");

    let first = common::authed(&app, Method::POST, &path, &token)
        .json(&json!({ "body": "Buy   NOW", "client_msg_id": Uuid::now_v7() }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = common::authed(&app, Method::POST, &path, &token)
        .json(&json!({ "body": "buy now", "client_msg_id": Uuid::now_v7() }))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = second.json().await.unwrap();
    assert_eq!(body["code"], "validation_failed");
    assert_eq!(body["errors"][0]["code"], "duplicate_message");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn new_account_posting_into_more_than_ten_discussions_is_slowed(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "newbie_01", PASSWORD).await;

    let mut discussions = Vec::new();
    for i in 0..11 {
        let created: Value = common::authed(&app, Method::POST, "/v1/discussions", &token)
            .json(&json!({ "url": format!("https://example.com/topic-{i}") }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        discussions.push(created["id"].as_str().unwrap().to_owned());
    }

    for (i, discussion) in discussions.iter().enumerate().take(10) {
        let response = common::authed(
            &app,
            Method::POST,
            &format!("/v1/discussions/{discussion}/messages"),
            &token,
        )
        .json(&json!({ "body": format!("hello number {i}"), "client_msg_id": Uuid::now_v7() }))
        .send()
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "post {i}");
    }
    let eleventh = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{}/messages", discussions[10]),
        &token,
    )
    .json(&json!({ "body": "hello number 10", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(eleventh.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(eleventh.headers().get("retry-after").is_some());

    app.stop().await.unwrap();
}
