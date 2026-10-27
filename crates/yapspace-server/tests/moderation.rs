//! Reports, the moderator queue and actions, blocks, account deletion and export, and the retention jobs.

mod common;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use yapspace_server::{
    app::AppState,
    jobs::{Job, run_once},
};

const PASSWORD: &str = "correct horse battery";

async fn make_role(pool: &PgPool, username: &str, role: &str) {
    sqlx::query("UPDATE users SET role = $2 WHERE username = $1")
        .bind(username)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

async fn discussion(app: &common::TestApp, token: &str, url: &str) -> String {
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

async fn post(app: &common::TestApp, token: &str, discussion: &str, body: &str) -> String {
    common::authed(
        app,
        Method::POST,
        &format!("/v1/discussions/{discussion}/messages"),
        token,
    )
    .json(&json!({ "body": body, "client_msg_id": Uuid::now_v7() }))
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

async fn report(
    app: &common::TestApp,
    token: &str,
    target_type: &str,
    target_id: &str,
    reason: &str,
) -> reqwest::Response {
    common::authed(app, Method::POST, "/v1/reports", token)
        .json(&json!({ "target_type": target_type, "target_id": target_id, "reason": reason }))
        .send()
        .await
        .unwrap()
}

#[sqlx::test]
async fn reporting_is_validated_and_idempotent(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (bob, bob_user, _) = common::register(&app, "bob_0002", PASSWORD).await;
    let disc = discussion(&app, &alice, "https://example.com/report").await;
    let message = post(&app, &alice, &disc, "suspicious text").await;

    let first = report(&app, &bob, "message", &message, "spam").await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first: Value = first.json().await.unwrap();
    assert_eq!(first["status"], "open");

    let again = report(&app, &bob, "message", &message, "harassment").await;
    assert_eq!(
        again.status(),
        StatusCode::OK,
        "same target again returns the existing report"
    );
    assert_eq!(again.json::<Value>().await.unwrap()["id"], first["id"]);

    assert_eq!(
        report(&app, &bob, "message", &message, "not-a-reason")
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        report(&app, &bob, "message", &Uuid::now_v7().to_string(), "spam")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let own = bob_user["id"].as_str().unwrap();
    assert_eq!(
        report(&app, &bob, "user", own, "spam").await.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "you cannot report yourself"
    );

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn moderators_work_the_queue_and_removals_are_visible_to_everyone(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (bob, _, _) = common::register(&app, "bob_0002", PASSWORD).await;
    let (mod_token, _, _) = common::register(&app, "mod_0003", PASSWORD).await;
    make_role(&pool, "mod_0003", "moderator").await;
    let disc = discussion(&app, &alice, "https://example.com/queue").await;
    let message = post(&app, &alice, &disc, "bad words").await;
    let filed: Value = report(&app, &bob, "message", &message, "harassment")
        .await
        .json()
        .await
        .unwrap();

    let not_mod = common::authed(&app, Method::GET, "/v1/mod/reports", &bob)
        .send()
        .await
        .unwrap();
    assert_eq!(not_mod.status(), StatusCode::FORBIDDEN);

    let queue: Value = common::authed(&app, Method::GET, "/v1/mod/reports", &mod_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(queue["items"][0]["id"], filed["id"]);

    let removed = common::authed(
        &app,
        Method::POST,
        &format!("/v1/mod/messages/{message}/remove"),
        &mod_token,
    )
    .json(&json!({ "reason": "breaks the rules", "report_id": filed["id"] }))
    .send()
    .await
    .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let public: Value = app
        .get(&format!("/v1/messages/{message}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(public["status"], "removed");
    assert!(public["body"].is_null(), "removed text must not leave the server");

    let queue_after: Value = common::authed(&app, Method::GET, "/v1/mod/reports", &mod_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        queue_after["items"].as_array().unwrap().is_empty(),
        "the report was resolved by the removal"
    );

    let restored = common::authed(
        &app,
        Method::POST,
        &format!("/v1/mod/messages/{message}/restore"),
        &mod_token,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(restored.status(), StatusCode::NO_CONTENT);
    let back: Value = app
        .get(&format!("/v1/messages/{message}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(back["body"], "bad words");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn dismissing_a_report_is_recorded_and_cannot_happen_twice(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (bob, _, _) = common::register(&app, "bob_0002", PASSWORD).await;
    let (mod_token, _, _) = common::register(&app, "mod_0003", PASSWORD).await;
    make_role(&pool, "mod_0003", "moderator").await;
    let disc = discussion(&app, &alice, "https://example.com/dismiss").await;
    let message = post(&app, &alice, &disc, "fine text").await;
    let filed: Value = report(&app, &bob, "message", &message, "spam")
        .await
        .json()
        .await
        .unwrap();
    let path = format!("/v1/mod/reports/{}/resolve", filed["id"].as_str().unwrap());

    let dismissed = common::authed(&app, Method::POST, &path, &mod_token)
        .json(&json!({ "outcome": "dismissed", "note": "not spam" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dismissed.status(), StatusCode::NO_CONTENT);
    let again = common::authed(&app, Method::POST, &path, &mod_token)
        .json(&json!({ "outcome": "dismissed" }))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::CONFLICT);

    let recorded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM moderation_actions WHERE action = 'dismiss_report'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recorded, 1);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn moderators_cannot_act_on_other_staff_but_admins_can(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (mod_a, _, _) = common::register(&app, "mod_a_001", PASSWORD).await;
    let (_, mod_b, _) = common::register(&app, "mod_b_002", PASSWORD).await;
    let (admin, _, _) = common::register(&app, "admin_0003", PASSWORD).await;
    make_role(&pool, "mod_a_001", "moderator").await;
    make_role(&pool, "mod_b_002", "moderator").await;
    make_role(&pool, "admin_0003", "admin").await;
    let mod_b_id = mod_b["id"].as_str().unwrap();
    let path = format!("/v1/mod/users/{mod_b_id}/suspend");

    let refused = common::authed(&app, Method::POST, &path, &mod_a)
        .json(&json!({ "reason": "peer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    let allowed = common::authed(&app, Method::POST, &path, &admin)
        .json(&json!({ "reason": "peer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::NO_CONTENT);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn suspension_blocks_posting_and_lifting_restores_it(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (alice, alice_user, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (mod_token, _, _) = common::register(&app, "mod_0003", PASSWORD).await;
    make_role(&pool, "mod_0003", "moderator").await;
    let disc = discussion(&app, &alice, "https://example.com/suspend").await;
    let alice_id = alice_user["id"].as_str().unwrap();

    let suspend = common::authed(
        &app,
        Method::POST,
        &format!("/v1/mod/users/{alice_id}/suspend"),
        &mod_token,
    )
    .json(&json!({ "reason": "spam", "until": "2999-01-01T00:00:00Z" }))
    .send()
    .await
    .unwrap();
    assert_eq!(suspend.status(), StatusCode::NO_CONTENT);
    let blocked = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{disc}/messages"),
        &alice,
    )
    .json(&json!({ "body": "hello", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        blocked.json::<Value>().await.unwrap()["code"],
        "account_suspended"
    );

    let lift = common::authed(
        &app,
        Method::POST,
        &format!("/v1/mod/users/{alice_id}/unsuspend"),
        &mod_token,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(lift.status(), StatusCode::NO_CONTENT);
    let allowed = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{disc}/messages"),
        &alice,
    )
    .json(&json!({ "body": "back again", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(allowed.status(), StatusCode::CREATED);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn locking_a_discussion_stops_posts_and_tells_subscribers(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let (mod_token, _, _) = common::register(&app, "mod_0003", PASSWORD).await;
    make_role(&pool, "mod_0003", "moderator").await;
    let disc = discussion(&app, &alice, "https://example.com/lock").await;

    let lock = common::authed(
        &app,
        Method::POST,
        &format!("/v1/mod/discussions/{disc}/lock"),
        &mod_token,
    )
    .json(&json!({ "reason": "heated" }))
    .send()
    .await
    .unwrap();
    assert_eq!(lock.status(), StatusCode::NO_CONTENT);
    let post_attempt = common::authed(
        &app,
        Method::POST,
        &format!("/v1/discussions/{disc}/messages"),
        &alice,
    )
    .json(&json!({ "body": "still here", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap();
    assert_eq!(post_attempt.status(), StatusCode::FORBIDDEN);

    let hidden = common::authed(
        &app,
        Method::POST,
        &format!("/v1/mod/discussions/{disc}/hide"),
        &mod_token,
    )
    .json(&json!({}))
    .send()
    .await
    .unwrap();
    assert_eq!(hidden.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        app.get(&format!("/v1/discussions/{disc}")).await.status(),
        StatusCode::NOT_FOUND
    );

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn blocks_are_personal_and_validated(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    common::register(&app, "bob_0002", PASSWORD).await;

    let put = common::authed(&app, Method::PUT, "/v1/me/blocks/bob_0002", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::NO_CONTENT);
    let put_again = common::authed(&app, Method::PUT, "/v1/me/blocks/bob_0002", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(put_again.status(), StatusCode::NO_CONTENT);

    let list: Value = common::authed(&app, Method::GET, "/v1/me/blocks", &alice)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["items"][0]["username"], "bob_0002");

    let myself = common::authed(&app, Method::PUT, "/v1/me/blocks/alice_01", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(myself.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let nobody = common::authed(&app, Method::PUT, "/v1/me/blocks/nobody_here", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(nobody.status(), StatusCode::NOT_FOUND);

    let removed = common::authed(&app, Method::DELETE, "/v1/me/blocks/bob_0002", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let empty: Value = common::authed(&app, Method::GET, "/v1/me/blocks", &alice)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(empty["items"].as_array().unwrap().is_empty());

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn deleting_an_account_frees_the_name_and_erases_content_in_the_background(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let (alice, alice_user, _) = common::register(&app, "alice_01", PASSWORD).await;
    let disc = discussion(&app, &alice, "https://example.com/delete-me").await;
    let message = post(&app, &alice, &disc, "my private words").await;

    let wrong = common::authed(&app, Method::DELETE, "/v1/me", &alice)
        .json(&json!({ "password": "not the password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let deleted = common::authed(&app, Method::DELETE, "/v1/me", &alice)
        .json(&json!({ "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::ACCEPTED);

    let login = app
        .http
        .post(app.url("/v1/auth/login"))
        .json(&json!({ "username": "alice_01", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::UNAUTHORIZED);
    let reused = common::authed(&app, Method::GET, "/v1/me", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(reused.status(), StatusCode::UNAUTHORIZED);

    let public: Value = app
        .get(&format!("/v1/messages/{message}"))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        public["author"].is_null(),
        "the author is hidden once the account is deleted"
    );
    let tombstone_name: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(alice_user["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(tombstone_name.starts_with("deleted_"));

    let state = AppState::with_pool(common::test_config(), pool.clone());
    let changed = run_once(&state, Job::EraseDeletedAccountContent).await.unwrap();
    assert!(changed >= 1);
    let body: String = sqlx::query_scalar("SELECT body FROM messages WHERE id = $1")
        .bind(message.parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(body, "", "the background job erases the text");

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn export_contains_the_profile_sessions_and_every_message(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (alice, _, _) = common::register(&app, "alice_01", PASSWORD).await;
    let disc = discussion(&app, &alice, "https://example.com/export").await;
    for i in 0..3 {
        post(&app, &alice, &disc, &format!("export me {i}")).await;
    }

    let response = common::authed(&app, Method::GET, "/v1/me/export", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    let export: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(export["profile"]["username"], "alice_01");
    assert_eq!(export["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(export["messages"].as_array().unwrap().len(), 3);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn retention_jobs_run_on_empty_data(pool: PgPool) {
    let state = AppState::with_pool(common::test_config(), pool);
    for job in [
        Job::PurgeSessions,
        Job::EraseDeletedAccountContent,
        Job::EraseRemovedBodies,
        Job::PurgeResolvedReports,
        Job::PurgeAudit,
    ] {
        assert_eq!(run_once(&state, job).await.unwrap(), 0, "{job:?}");
    }
}
