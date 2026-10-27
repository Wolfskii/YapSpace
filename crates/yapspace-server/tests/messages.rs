//! Messages: posting (idempotent, replies, seq allocation under concurrency), pagination, tombstones, and locks.

mod common;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";

async fn new_discussion(app: &common::TestApp, token: &str, url: &str) -> String {
    let response = common::authed(app, Method::POST, "/v1/discussions", token)
        .json(&json!({ "url": url }))
        .send()
        .await
        .unwrap();
    response.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn post(app: &common::TestApp, token: &str, discussion: &str, body: &str) -> Value {
    let response = common::authed(
        app,
        Method::POST,
        &format!("/v1/discussions/{discussion}/messages"),
        token,
    )
    .json(&json!({ "body": body, "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "post {body}");
    response.json().await.unwrap()
}

#[sqlx::test]
async fn posting_returns_seq_one_and_a_retry_returns_the_same_message(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = new_discussion(&app, &token, "https://example.com/a").await;

    let client_msg_id = Uuid::now_v7();
    let path = format!("/v1/discussions/{discussion}/messages");
    let first = common::authed(&app, Method::POST, &path, &token)
        .json(&json!({ "body": "hello", "client_msg_id": client_msg_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first: Value = first.json().await.unwrap();
    assert_eq!(first["seq"], 1);
    assert_eq!(first["body"], "hello");

    let retry = common::authed(&app, Method::POST, &path, &token)
        .json(&json!({ "body": "hello", "client_msg_id": client_msg_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    let retry: Value = retry.json().await.unwrap();
    assert_eq!(retry["id"], first["id"]);
    assert_eq!(retry["seq"], 1);

    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn fifty_concurrent_posts_get_seqs_one_to_fifty(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = new_discussion(&app, &token, "https://example.com/busy").await;

    let path = format!("/v1/discussions/{discussion}/messages");
    let requests = (0..50).map(|i| {
        let http = app.http.clone();
        let url = app.url(&path);
        let token = token.clone();
        async move {
            http.post(url)
                .bearer_auth(token)
                .json(&json!({ "body": format!("message {i}"), "client_msg_id": Uuid::now_v7() }))
                .send()
                .await
                .unwrap()
                .status()
        }
    });
    let statuses = futures_util::future::join_all(requests).await;
    assert!(statuses.iter().all(|s| *s == StatusCode::CREATED));

    let seqs: Vec<i64> = sqlx::query_scalar("SELECT seq FROM messages WHERE discussion_id = $1 ORDER BY seq")
        .bind(discussion.parse::<Uuid>().unwrap())
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(seqs, (1..=50).collect::<Vec<i64>>(), "gapless, no duplicates");

    let last_seq: i64 = sqlx::query_scalar("SELECT last_seq FROM discussions WHERE id = $1")
        .bind(discussion.parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(last_seq, 50);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn reply_must_point_at_a_visible_message_in_the_same_discussion(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let first = new_discussion(&app, &token, "https://example.com/one").await;
    let second = new_discussion(&app, &token, "https://example.com/two").await;
    let parent = post(&app, &token, &first, "parent text").await;

    let cross = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{second}/messages"),
        &token,
    )
    .json(&json!({ "body": "reply", "parent_id": parent["id"], "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(cross.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(cross.json::<Value>().await.unwrap()["code"], "invalid_parent");

    let reply = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{first}/messages"),
        &token,
    )
    .json(&json!({ "body": "reply", "parent_id": parent["id"], "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(reply.status(), StatusCode::CREATED);
    let reply: Value = reply.json().await.unwrap();
    assert_eq!(reply["parent"]["id"], parent["id"]);
    assert_eq!(reply["parent"]["excerpt"], "parent text");
    assert_eq!(reply["parent"]["author_username"], "alice_01");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn message_body_limits_are_enforced(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = new_discussion(&app, &token, "https://example.com/limits").await;
    let path = format!("/v1/discussions/{discussion}/messages");

    for body in ["   ".to_owned(), "x".repeat(2001)] {
        let response = common::authed(&app, Method::POST, &path, &token)
            .json(&json!({ "body": body, "client_msg_id": Uuid::now_v7() }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.json::<Value>().await.unwrap()["errors"][0]["field"],
            "body"
        );
    }

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn pages_walk_backwards_and_forwards_with_cursors(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = new_discussion(&app, &token, "https://example.com/history").await;
    for i in 1..=120 {
        post(&app, &token, &discussion, &format!("message {i}")).await;
    }
    let base = format!("/v1/discussions/{discussion}/messages");
    let seqs = |page: &Value| -> Vec<i64> {
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["seq"].as_i64().unwrap())
            .collect()
    };

    let newest: Value = app.get(&base).await.json().await.unwrap();
    assert_eq!(seqs(&newest), (71..=120).collect::<Vec<_>>());
    assert_eq!(newest["has_more"], true);
    assert_eq!(newest["next_before_seq"], 71);

    let older: Value = app
        .get(&format!("{base}?before_seq=71"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&older), (21..=70).collect::<Vec<_>>());

    let oldest: Value = app
        .get(&format!("{base}?before_seq=21"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&oldest), (1..=20).collect::<Vec<_>>());
    assert_eq!(oldest["has_more"], false);
    assert!(oldest["next_before_seq"].is_null());

    let newer: Value = app
        .get(&format!("{base}?after_seq=100&limit=10"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&newer), (101..=110).collect::<Vec<_>>());
    assert_eq!(newer["has_more"], true);
    assert_eq!(newer["next_after_seq"], 110);

    let clamped_low = app
        .get(&format!("{base}?limit=0"))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(seqs(&clamped_low).len(), 1);
    let clamped_high = app
        .get(&format!("{base}?limit=1000"))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(seqs(&clamped_high).len(), 100);

    let both = app.get(&format!("{base}?before_seq=5&after_seq=1")).await;
    assert_eq!(both.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let garbage = app.get(&format!("{base}?limit=abc")).await;
    assert_eq!(garbage.status(), StatusCode::BAD_REQUEST);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn history_query_uses_the_discussion_seq_index(pool: PgPool) {
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("SET enable_seqscan = off")
        .execute(&mut *conn)
        .await
        .unwrap();
    let plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN SELECT id FROM messages WHERE discussion_id = $1 AND seq < $2 ORDER BY seq DESC LIMIT 10",
    )
    .bind(Uuid::now_v7())
    .bind(i64::MAX)
    .fetch_all(&mut *conn)
    .await
    .unwrap();
    assert!(
        plan.iter()
            .any(|line| line.contains("messages_discussion_seq_key")),
        "plan does not use the seq index: {plan:?}"
    );
}

#[sqlx::test]
async fn deleting_leaves_a_tombstone_and_blocks_new_replies(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (bob, _, _) = common::register(&app, "bob_0002", PASSWORD).await;
    let discussion = new_discussion(&app, &alice, "https://example.com/del").await;
    let message = post(&app, &alice, &discussion, "secret plans").await;
    let id = message["id"].as_str().unwrap().to_owned();

    let by_other = common::authed(&app, Method::DELETE, &format!("/v1/messages/{id}"), &bob)
        .send()
        .await
        .unwrap();
    assert_eq!(by_other.status(), StatusCode::NOT_FOUND);

    let deleted = common::authed(&app, Method::DELETE, &format!("/v1/messages/{id}"), &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let again = common::authed(&app, Method::DELETE, &format!("/v1/messages/{id}"), &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(
        again.status(),
        StatusCode::NO_CONTENT,
        "repeating a delete succeeds"
    );

    let tombstone: Value = app.get(&format!("/v1/messages/{id}")).await.json().await.unwrap();
    assert_eq!(tombstone["status"], "deleted");
    assert!(tombstone["body"].is_null());
    assert_eq!(tombstone["seq"], 1, "the seq is kept so history stays gapless");

    let reply = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{discussion}/messages"),
        &bob,
    )
    .json(&json!({ "body": "reply", "parent_id": id, "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(reply.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn locked_discussion_refuses_posts(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = new_discussion(&app, &token, "https://example.com/locked").await;

    sqlx::query("UPDATE discussions SET status = 'locked' WHERE id = $1")
        .bind(discussion.parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let response = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{discussion}/messages"),
        &token,
    )
    .json(&json!({ "body": "hello", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "discussion_locked"
    );

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn suspended_account_can_read_but_not_post(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = new_discussion(&app, &token, "https://example.com/suspended").await;

    sqlx::query("UPDATE users SET status = 'suspended', suspended_until = now() + interval '1 day' WHERE username = 'alice_01'")
        .execute(&pool)
        .await
        .unwrap();

    let read = common::authed(
        &app,
        Method::GET,
        &format!("/v1/discussions/{discussion}/messages"),
        &token,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(read.status(), StatusCode::OK);

    let post_attempt = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{discussion}/messages"),
        &token,
    )
    .json(&json!({ "body": "hello", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(post_attempt.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        post_attempt.json::<Value>().await.unwrap()["code"],
        "account_suspended"
    );

    app.stop().await.unwrap();
}
