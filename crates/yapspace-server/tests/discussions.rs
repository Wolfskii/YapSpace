//! Discussion resolution, creation (including the race), reads, and merges.

mod common;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

const PASSWORD: &str = "correct horse battery";

async fn create_discussion(app: &common::TestApp, token: &str, url: &str) -> Value {
    let response = common::authed(app, Method::POST, "/v1/discussions", token)
        .json(&json!({ "url": url }))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "create {url}: {}",
        response.status()
    );
    response.json().await.unwrap()
}

#[sqlx::test]
async fn resolve_unknown_page_returns_null_discussion_and_never_creates(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;

    let response = app
        .http
        .post(app.url("/v1/discussions/resolve"))
        .json(&json!({ "url": "https://example.com/news/a?utm_source=x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert!(body["discussion"].is_null());
    assert_eq!(body["identity"]["kind"], "url");
    assert_eq!(body["identity"]["display_url"], "https://example.com/news/a");
    assert_eq!(body["identity"]["site"], "example.com");

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM discussions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "resolve must not create a discussion");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn create_then_resolve_with_other_tracking_parameters_finds_it(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let created = common::authed(&app, Method::POST, "/v1/discussions", &token)
        .json(&json!({ "url": "https://www.example.com/news/a?utm_source=newsletter", "title": "  Big   news\u{0}  " }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let discussion: Value = created.json().await.unwrap();
    assert_eq!(discussion["display_url"], "https://example.com/news/a");
    assert_eq!(discussion["title"], "Big news");
    assert_eq!(discussion["message_count"], 0);

    let resolved: Value = app
        .http
        .post(app.url("/v1/discussions/resolve"))
        .json(&json!({ "url": "https://example.com/news/a?fbclid=zzz" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resolved["discussion"]["id"], discussion["id"]);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn creating_the_same_page_again_returns_200_with_the_same_id(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let first = create_discussion(&app, &token, "https://example.com/a").await;
    let again = common::authed(&app, Method::POST, "/v1/discussions", &token)
        .json(&json!({ "url": "https://example.com/a/" }))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::OK);
    let second: Value = again.json().await.unwrap();
    assert_eq!(first["id"], second["id"]);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn twenty_concurrent_creates_make_exactly_one_discussion(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let requests = (0..20).map(|_| {
        let http = app.http.clone();
        let url = app.url("/v1/discussions");
        let token = token.clone();
        async move {
            http.post(url)
                .bearer_auth(token)
                .json(&json!({ "url": "https://example.com/race" }))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        }
    });
    let results = futures_util::future::join_all(requests).await;
    let ids: std::collections::HashSet<String> = results
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids.len(), 1, "all creates must return the same discussion");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM discussions WHERE site = 'example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn sensitive_and_blocked_urls_are_refused_with_a_reason(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    for (url, reason) in [
        ("https://example.com/reset?token=abc", "sensitive_parameters"),
        ("https://docs.google.com/document/d/1x/edit", "blocked_host"),
        ("http://192.168.0.1/admin", "ip_host"),
    ] {
        let response = common::authed(&app, Method::POST, "/v1/discussions", &token)
            .json(&json!({ "url": url }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{url}");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["code"], "url_unsupported");
        assert_eq!(body["reason"], reason, "{url}");
    }

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn canonical_link_on_the_same_site_joins_the_discussion(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let created = common::authed(&app, Method::POST, "/v1/discussions", &token)
        .json(&json!({ "url": "https://blog.example.com/p?page=2", "canonical_url": "https://news.example.com/p" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let discussion: Value = created.json().await.unwrap();
    assert_eq!(discussion["display_url"], "https://news.example.com/p");

    // Without the canonical link, the same page on the other host is a different discussion.
    let resolved: Value = app
        .http
        .post(app.url("/v1/discussions/resolve"))
        .json(&json!({ "url": "https://news.example.com/p" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resolved["identity"]["kind"], "url");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn cross_site_canonical_is_ignored(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let created: Value = common::authed(&app, Method::POST, "/v1/discussions", &token)
        .json(
            &json!({ "url": "https://news.example.com/a", "canonical_url": "https://youtu.be/dQw4w9WgXcQ" }),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["display_url"], "https://news.example.com/a");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn create_needs_a_session(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let response = app
        .http
        .post(app.url("/v1/discussions"))
        .json(&json!({ "url": "https://example.com/a" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn unknown_discussion_is_404_and_bad_id_is_400(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let missing = app
        .get(&format!("/v1/discussions/{}", uuid::Uuid::now_v7()))
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let bad = app.get("/v1/discussions/not-a-uuid").await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn merged_discussion_resolves_to_its_target(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let source = create_discussion(&app, &token, "https://example.com/old").await;
    let target = create_discussion(&app, &token, "https://example.com/new").await;

    sqlx::query("UPDATE discussions SET merged_into_id = $2 WHERE id = $1")
        .bind(source["id"].as_str().unwrap().parse::<uuid::Uuid>().unwrap())
        .bind(target["id"].as_str().unwrap().parse::<uuid::Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let read: Value = app
        .get(&format!("/v1/discussions/{}", source["id"].as_str().unwrap()))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(read["id"], target["id"]);
    assert_eq!(read["merged_from"], source["id"]);

    app.stop().await.unwrap();
}
