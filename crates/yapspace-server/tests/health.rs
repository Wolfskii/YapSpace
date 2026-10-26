//! Health endpoints, request-id handling, error bodies, and shutdown, tested against the real HTTP server.

mod common;

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};

#[sqlx::test]
async fn healthz_is_ok_and_returns_a_request_id(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app.get("/healthz").await;
    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers()["x-request-id"].to_str().unwrap().to_owned();
    assert_eq!(request_id.len(), 36, "generated ids are UUIDs");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn readyz_is_ok_when_the_database_answers(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app.get("/readyz").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["database"], "ok");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn unknown_route_returns_problem_json_with_the_same_request_id(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app.get("/does-not-exist").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()["content-type"], "application/problem+json");
    let header_id = response.headers()["x-request-id"].to_str().unwrap().to_owned();
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["status"], 404);
    assert_eq!(body["request_id"], header_id.as_str());

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn incoming_request_id_with_unsafe_characters_is_replaced(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app
        .http
        .get(app.url("/healthz"))
        .header("x-request-id", "bad id\"!!")
        .send()
        .await
        .unwrap();
    let id = response.headers()["x-request-id"].to_str().unwrap();
    assert_ne!(id, "bad id\"!!");
    assert_eq!(id.len(), 36);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn valid_incoming_request_id_is_kept(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app
        .http
        .get(app.url("/healthz"))
        .header("x-request-id", "abc-123")
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["x-request-id"], "abc-123");

    app.stop().await.unwrap();
}

#[tokio::test]
async fn readyz_is_503_when_the_database_is_unreachable() {
    // Port 1 refuses connections, so the check fails at once. The server must still start and report not ready.
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(1))
        .connect_lazy("postgres://yapspace:yapspace@127.0.0.1:1/yapspace")
        .unwrap();
    let app = common::spawn_app(pool).await;

    let response = app.get("/readyz").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["database"], "unavailable");
    // Liveness is unaffected by database trouble.
    assert_eq!(app.get("/healthz").await.status(), StatusCode::OK);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn shutdown_stops_the_server_within_the_grace_period(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    assert_eq!(app.get("/healthz").await.status(), StatusCode::OK);

    let finished = tokio::time::timeout(Duration::from_secs(5), app.stop()).await;
    assert!(finished.is_ok(), "server did not stop within 5 s");
    finished
        .unwrap()
        .expect("server returned an error during shutdown");
}
