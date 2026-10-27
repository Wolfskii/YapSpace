//! Real-time socket tests against the real server, using a real WebSocket client.

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";
const RECV_TIMEOUT: Duration = Duration::from_secs(5);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn connect(app: &common::TestApp) -> Socket {
    let url = format!("ws://{}/v1/realtime", app.addr);
    connect_async(url).await.expect("websocket connects").0
}

async fn send(socket: &mut Socket, frame: Value) {
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .expect("send frame");
}

/// Next JSON text frame, skipping pings. Panics if nothing arrives in time.
async fn next(socket: &mut Socket) -> Value {
    loop {
        let message = tokio::time::timeout(RECV_TIMEOUT, socket.next())
            .await
            .expect("frame arrives in time")
            .expect("socket open")
            .expect("no transport error");
        match message {
            Message::Text(text) => return serde_json::from_str(&text).expect("json frame"),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected message {other:?}"),
        }
    }
}

async fn next_type(socket: &mut Socket, kind: &str) -> Value {
    loop {
        let frame = next(socket).await;
        if frame["type"] == kind {
            return frame;
        }
    }
}

async fn discussion_for(app: &common::TestApp, token: &str, url: &str) -> String {
    common::authed(app, Method::POST, "/v1/discussions", token)
        .json(&json!({ "url": url }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
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
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json().await.unwrap()
}

#[sqlx::test]
async fn welcome_arrives_on_connect(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let mut socket = connect(&app).await;

    let welcome = next(&mut socket).await;
    assert_eq!(welcome["type"], "welcome");
    assert_eq!(welcome["protocol"], 1);
    assert_eq!(welcome["max_subscriptions"], 5);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn subscribe_replays_the_backlog_then_confirms(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = discussion_for(&app, &token, "https://example.com/replay").await;
    for i in 1..=3 {
        post(&app, &token, &discussion, &format!("message {i}")).await;
    }

    let mut socket = connect(&app).await;
    next_type(&mut socket, "welcome").await;
    send(
        &mut socket,
        json!({ "type": "subscribe", "discussion_id": discussion, "after_seq": 1 }),
    )
    .await;

    let second = next(&mut socket).await;
    assert_eq!(second["type"], "message.created");
    assert_eq!(second["seq"], 2);
    let third = next(&mut socket).await;
    assert_eq!(third["seq"], 3);
    let confirmed = next(&mut socket).await;
    assert_eq!(confirmed["type"], "subscribed");
    assert_eq!(confirmed["last_seq"], 3);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn live_messages_reach_other_subscribers(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (bob, _, _) = common::register(&app, "bob_0002", PASSWORD).await;
    let discussion = discussion_for(&app, &alice, "https://example.com/live").await;

    let mut watcher = connect(&app).await;
    next_type(&mut watcher, "welcome").await;
    send(
        &mut watcher,
        json!({ "type": "subscribe", "discussion_id": discussion }),
    )
    .await;
    next_type(&mut watcher, "subscribed").await;

    let posted = post(&app, &bob, &discussion, "hello from bob").await;
    let event = next_type(&mut watcher, "message.created").await;
    assert_eq!(event["seq"], 1);
    assert_eq!(event["message"]["id"], posted["id"]);
    assert_eq!(event["message"]["body"], "hello from bob");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn reconnecting_with_the_last_seq_recovers_what_was_missed(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = discussion_for(&app, &token, "https://example.com/resume").await;
    post(&app, &token, &discussion, "before the drop").await;

    let mut first = connect(&app).await;
    next_type(&mut first, "welcome").await;
    send(
        &mut first,
        json!({ "type": "subscribe", "discussion_id": discussion, "after_seq": 0 }),
    )
    .await;
    next_type(&mut first, "subscribed").await;
    drop(first);

    for i in 0..5 {
        post(&app, &token, &discussion, &format!("while offline {i}")).await;
    }

    let mut second = connect(&app).await;
    next_type(&mut second, "welcome").await;
    send(
        &mut second,
        json!({ "type": "subscribe", "discussion_id": discussion, "after_seq": 1 }),
    )
    .await;
    let mut seqs = Vec::new();
    loop {
        let frame = next(&mut second).await;
        match frame["type"].as_str().unwrap() {
            "message.created" => seqs.push(frame["seq"].as_i64().unwrap()),
            "subscribed" => break,
            other => panic!("unexpected frame {other}"),
        }
    }
    assert_eq!(
        seqs,
        vec![2, 3, 4, 5, 6],
        "exactly the five missed messages, no duplicates"
    );

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn a_gap_larger_than_500_asks_the_client_to_reload(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (token, alice, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = discussion_for(&app, &token, "https://example.com/big-gap").await;
    let discussion_id: Uuid = discussion.parse().unwrap();
    let author: Uuid = alice["id"].as_str().unwrap().parse().unwrap();

    sqlx::query(
        "INSERT INTO messages (id, discussion_id, seq, author_id, body, client_msg_id)
         SELECT gen_random_uuid(), $1, g, $2, 'bulk ' || g, gen_random_uuid() FROM generate_series(1, 600) g",
    )
    .bind(discussion_id)
    .bind(author)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE discussions SET last_seq = 600, message_count = 600 WHERE id = $1")
        .bind(discussion_id)
        .execute(&pool)
        .await
        .unwrap();

    let mut socket = connect(&app).await;
    next_type(&mut socket, "welcome").await;
    send(
        &mut socket,
        json!({ "type": "subscribe", "discussion_id": discussion, "after_seq": 0 }),
    )
    .await;
    let resync = next_type(&mut socket, "resync_required").await;
    assert_eq!(resync["reason"], "gap_too_large");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn bad_frames_get_an_error_and_the_socket_stays_open(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let mut socket = connect(&app).await;
    next_type(&mut socket, "welcome").await;

    socket.send(Message::Text("not json".into())).await.unwrap();
    let error = next_type(&mut socket, "error").await;
    assert_eq!(error["code"], "bad_frame");

    send(&mut socket, json!({ "type": "ping", "nonce": 9 })).await;
    let pong = next_type(&mut socket, "pong").await;
    assert_eq!(pong["nonce"], 9);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn oversized_frame_ends_the_connection(pool: PgPool) {
    // Known limitation: the server closes with 1009, but the client often sees a TCP reset first, because bytes of
    // the oversized frame were never read. So this test checks that the connection ends, not the exact code.
    let app = common::spawn_app(pool).await;
    let mut socket = connect(&app).await;
    next_type(&mut socket, "welcome").await;

    socket
        .send(Message::Text("x".repeat(5 * 1024).into()))
        .await
        .unwrap();
    let ended = loop {
        match tokio::time::timeout(RECV_TIMEOUT, socket.next())
            .await
            .expect("ends in time")
        {
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break true,
            Some(Ok(_)) => continue,
        }
    };
    assert!(ended);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn unknown_discussion_and_too_many_subscriptions_are_errors(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let mut socket = connect(&app).await;
    next_type(&mut socket, "welcome").await;

    send(
        &mut socket,
        json!({ "type": "subscribe", "discussion_id": Uuid::now_v7() }),
    )
    .await;
    assert_eq!(
        next_type(&mut socket, "error").await["code"],
        "unknown_discussion"
    );

    for i in 0..5 {
        let id = discussion_for(&app, &token, &format!("https://example.com/many-{i}")).await;
        send(&mut socket, json!({ "type": "subscribe", "discussion_id": id })).await;
        next_type(&mut socket, "subscribed").await;
    }
    let sixth = discussion_for(&app, &token, "https://example.com/many-5").await;
    send(
        &mut socket,
        json!({ "type": "subscribe", "discussion_id": sixth }),
    )
    .await;
    assert_eq!(
        next_type(&mut socket, "error").await["code"],
        "too_many_subscriptions"
    );

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn origins_from_web_pages_are_refused_and_extensions_allowed(pool: PgPool) {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let app = common::spawn_app(pool).await;
    let url = format!("ws://{}/v1/realtime", app.addr);

    let mut evil = url.as_str().into_client_request().unwrap();
    evil.headers_mut()
        .insert("origin", "https://evil.example".parse().unwrap());
    let refused = connect_async(evil)
        .await
        .expect_err("web page origin must be refused");
    match refused {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::FORBIDDEN)
        }
        other => panic!("expected an HTTP refusal, got {other:?}"),
    }

    let mut extension = url.as_str().into_client_request().unwrap();
    extension
        .headers_mut()
        .insert("origin", "chrome-extension://abcdef".parse().unwrap());
    let (mut socket, _) = connect_async(extension)
        .await
        .expect("extension origin is allowed");
    next_type(&mut socket, "welcome").await;

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn silent_client_is_disconnected_after_the_idle_timeout(pool: PgPool) {
    // Known limitation: tokio-tungstenite answers pings automatically, and only while the client reads. A client that
    // stops reading gets no pongs, so the server idles out and closes with 4004. The client often sees a reset, because
    // its late pong hits the closed socket. So this test checks that the connection ends, not the exact code.
    let app = common::spawn_app(pool).await;
    let mut socket = connect(&app).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let ended = loop {
        match tokio::time::timeout(RECV_TIMEOUT, socket.next())
            .await
            .expect("ends in time")
        {
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break true,
            Some(Ok(_)) => continue,
        }
    };
    assert!(ended);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn shutdown_closes_open_sockets_with_1012(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let mut socket = connect(&app).await;
    next_type(&mut socket, "welcome").await;

    app.shutdown.cancel();
    let close = loop {
        match tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("close within 2 s")
        {
            Some(Ok(Message::Close(Some(frame)))) => break u16::from(frame.code),
            Some(Ok(_)) => continue,
            other => panic!("expected a close frame, got {other:?}"),
        }
    };
    assert_eq!(close, 1012);
    let _ = app.server.await;
}

#[sqlx::test]
async fn deleting_a_message_publishes_an_update(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let discussion = discussion_for(&app, &token, "https://example.com/deletes").await;
    let posted = post(&app, &token, &discussion, "to be removed").await;

    let mut watcher = connect(&app).await;
    next_type(&mut watcher, "welcome").await;
    send(
        &mut watcher,
        json!({ "type": "subscribe", "discussion_id": discussion, "after_seq": 1 }),
    )
    .await;
    next_type(&mut watcher, "subscribed").await;

    let deleted = common::authed(
        &app,
        Method::DELETE,
        &format!("/v1/messages/{}", posted["id"].as_str().unwrap()),
        &token,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let update = next_type(&mut watcher, "message.updated").await;
    assert_eq!(update["seq"], 1);
    assert_eq!(update["message"]["status"], "deleted");
    assert!(update["message"]["body"].is_null());

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn an_account_may_hold_at_most_ten_sockets(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (token, _, _) = common::register(&app, "alice_01", PASSWORD).await;

    let mut sockets = Vec::new();
    for _ in 0..10 {
        let mut socket = connect(&app).await;
        next_type(&mut socket, "welcome").await;
        send(
            &mut socket,
            json!({ "type": "hello", "protocol": 1, "token": token }),
        )
        .await;
        assert!(!next_type(&mut socket, "hello_ok").await["user_id"].is_null());
        sockets.push(socket);
    }

    let mut eleventh = connect(&app).await;
    next_type(&mut eleventh, "welcome").await;
    send(
        &mut eleventh,
        json!({ "type": "hello", "protocol": 1, "token": token }),
    )
    .await;
    let refused = next_type(&mut eleventh, "error").await;
    assert_eq!(refused["code"], "rate_limited");

    app.stop().await.unwrap();
}
