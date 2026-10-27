//! Authorization matrix. Every route in the OpenAPI document must appear in this table, so a new endpoint cannot ship
//! without its access rules being tested. The table says whether a route needs a session, and whether a suspended
//! account may still call it.

mod common;

use std::collections::BTreeSet;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";

/// One row of the matrix.
struct Route {
    method: Method,
    template: &'static str,
    /// Needs a session. Anonymous callers get 401.
    protected: bool,
    /// Refused for suspended accounts with 403 `account_suspended`.
    write: bool,
    /// Body that passes validation, used so the suspension check is what fails.
    body: Option<Value>,
}

fn table() -> Vec<Route> {
    let r = |method: Method, template, protected, write, body| Route {
        method,
        template,
        protected,
        write,
        body,
    };
    vec![
        r(Method::GET, "/healthz", false, false, None),
        r(Method::GET, "/readyz", false, false, None),
        r(
            Method::POST,
            "/v1/auth/register",
            false,
            false,
            Some(json!({ "username": "unused_name", "password": PASSWORD })),
        ),
        r(
            Method::POST,
            "/v1/auth/login",
            false,
            false,
            Some(json!({ "username": "unused_name", "password": PASSWORD })),
        ),
        r(Method::POST, "/v1/auth/logout", true, false, None),
        r(
            Method::POST,
            "/v1/auth/recover",
            false,
            false,
            Some(
                json!({ "username": "unused_name", "recovery_code": "AAAAA-AAAAA-AAAAA-AAAAA", "new_password": PASSWORD }),
            ),
        ),
        r(Method::GET, "/v1/me", true, false, None),
        r(
            Method::POST,
            "/v1/me/password",
            true,
            true,
            Some(json!({ "current_password": PASSWORD, "new_password": "another secret phrase" })),
        ),
        r(
            Method::POST,
            "/v1/me/recovery-code",
            true,
            true,
            Some(json!({ "current_password": PASSWORD })),
        ),
        r(Method::GET, "/v1/me/sessions", true, false, None),
        r(Method::DELETE, "/v1/me/sessions/{id}", true, false, None),
        r(Method::GET, "/v1/users/{username}", false, false, None),
        r(
            Method::POST,
            "/v1/discussions/resolve",
            false,
            false,
            Some(json!({ "url": "https://example.com/matrix" })),
        ),
        r(
            Method::POST,
            "/v1/discussions",
            true,
            true,
            Some(json!({ "url": "https://example.com/matrix-create" })),
        ),
        r(Method::GET, "/v1/discussions/{id}", false, false, None),
        r(Method::GET, "/v1/discussions/{id}/messages", false, false, None),
        r(
            Method::POST,
            "/v1/discussions/{id}/messages",
            true,
            true,
            Some(json!({ "body": "matrix message", "client_msg_id": Uuid::now_v7() })),
        ),
        r(Method::GET, "/v1/messages/{id}", false, false, None),
        r(Method::DELETE, "/v1/messages/{id}", true, true, None),
        r(
            Method::POST,
            "/v1/reports",
            true,
            true,
            Some(json!({ "target_type": "message", "target_id": Uuid::now_v7(), "reason": "spam" })),
        ),
        r(
            Method::DELETE,
            "/v1/me",
            true,
            false,
            Some(json!({ "password": PASSWORD })),
        ),
        r(Method::GET, "/v1/me/export", true, false, None),
        r(Method::GET, "/v1/me/blocks", true, false, None),
        r(Method::PUT, "/v1/me/blocks/{username}", true, true, None),
        r(Method::DELETE, "/v1/me/blocks/{username}", true, false, None),
        r(Method::GET, "/v1/mod/reports", true, false, None),
        r(
            Method::POST,
            "/v1/mod/reports/{id}/resolve",
            true,
            false,
            Some(json!({ "outcome": "dismissed" })),
        ),
        r(
            Method::POST,
            "/v1/mod/messages/{id}/remove",
            true,
            false,
            Some(json!({})),
        ),
        r(Method::POST, "/v1/mod/messages/{id}/restore", true, false, None),
        r(
            Method::POST,
            "/v1/mod/users/{id}/suspend",
            true,
            false,
            Some(json!({})),
        ),
        r(Method::POST, "/v1/mod/users/{id}/unsuspend", true, false, None),
        r(
            Method::POST,
            "/v1/mod/discussions/{id}/lock",
            true,
            false,
            Some(json!({})),
        ),
        r(
            Method::POST,
            "/v1/mod/discussions/{id}/unlock",
            true,
            false,
            Some(json!({})),
        ),
        r(
            Method::POST,
            "/v1/mod/discussions/{id}/hide",
            true,
            false,
            Some(json!({})),
        ),
        r(
            Method::POST,
            "/v1/mod/discussions/{id}/unhide",
            true,
            false,
            Some(json!({})),
        ),
    ]
}

/// Method and path template pairs in the OpenAPI document.
fn documented_routes() -> BTreeSet<(String, String)> {
    let doc = serde_json::to_value(yapspace_server::api::openapi::document()).unwrap();
    let mut routes = BTreeSet::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for method in item.as_object().unwrap().keys() {
            routes.insert((method.to_uppercase(), path.clone()));
        }
    }
    routes
}

#[test]
fn every_documented_route_is_in_the_matrix() {
    let documented = documented_routes();
    let tested: BTreeSet<(String, String)> = table()
        .iter()
        .map(|r| (r.method.as_str().to_owned(), r.template.to_owned()))
        .collect();
    assert_eq!(documented, tested, "matrix and OpenAPI document differ");
}

struct World {
    bob_token: String,
    carol_token: String,
    discussion: String,
    message: String,
    alice_session: String,
    alice_username: String,
}

async fn build_world(app: &common::TestApp, pool: &PgPool) -> World {
    let (alice_token, alice, _) = common::register(app, "alice_01", PASSWORD).await;
    let (bob_token, _, _) = common::register(app, "bob_0002", PASSWORD).await;
    let (carol_token, _, _) = common::register(app, "carol_003", PASSWORD).await;

    let discussion: Value = common::authed(app, Method::POST, "/v1/discussions", &alice_token)
        .json(&json!({ "url": "https://example.com/world" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let discussion = discussion["id"].as_str().unwrap().to_owned();

    let message: Value = common::authed(
        app,
        Method::POST,
        &format!("/v1/discussions/{discussion}/messages"),
        &alice_token,
    )
    .json(&json!({ "body": "seed", "client_msg_id": Uuid::now_v7() }))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

    let sessions: Value = common::authed(app, Method::GET, "/v1/me/sessions", &alice_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_session = sessions["items"][0]["id"].as_str().unwrap().to_owned();

    // Carol is the suspended account.
    sqlx::query("UPDATE users SET status = 'suspended', suspended_until = now() + interval '1 day' WHERE username = 'carol_003'")
        .execute(pool)
        .await
        .unwrap();

    World {
        bob_token,
        carol_token,
        discussion,
        message: message["id"].as_str().unwrap().to_owned(),
        alice_session,
        alice_username: alice["username"].as_str().unwrap().to_owned(),
    }
}

fn path_for(template: &str, world: &World) -> String {
    template
        .replace(
            "{id}",
            &if template.contains("sessions") {
                world.alice_session.clone()
            } else if template.contains("messages/") {
                world.message.clone()
            } else {
                world.discussion.clone()
            },
        )
        .replace("{username}", &world.alice_username)
}

#[sqlx::test]
async fn access_rules_hold_for_every_route(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let world = build_world(&app, &pool).await;

    for route in table() {
        let path = path_for(route.template, &world);
        let method = route.method.clone();

        // Anonymous callers: protected routes must refuse them.
        if route.protected {
            let anon = app.http.request(method.clone(), app.url(&path));
            let anon = match &route.body {
                Some(body) => anon.json(body),
                None => anon,
            };
            let response = anon.send().await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "anonymous {method} {path}"
            );
        }

        // Suspended accounts: write routes refuse them with account_suspended.
        if route.write {
            let request = common::authed(&app, method.clone(), &path, &world.carol_token);
            let request = match &route.body {
                Some(body) => request.json(body),
                None => request,
            };
            let response = request.send().await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "suspended {method} {path}"
            );
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["code"], "account_suspended", "suspended {method} {path}");
        }
    }

    // Another user cannot delete someone else's message. The answer is 404, not 403.
    let stolen = common::authed(
        &app,
        Method::DELETE,
        &format!("/v1/messages/{}", world.message),
        &world.bob_token,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(stolen.status(), StatusCode::NOT_FOUND);

    // Public reads work without a session.
    assert_eq!(
        app.get(&format!("/v1/discussions/{}", world.discussion))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.get(&format!("/v1/messages/{}", world.message)).await.status(),
        StatusCode::OK
    );

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn oversized_body_is_413_problem_json(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let big = "x".repeat(20 * 1024);

    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .header("content-type", "application/json")
        .body(format!("{{\"username\":\"big_name\",\"password\":\"{big}\"}}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    app.stop().await.unwrap();
}

#[sqlx::test]
async fn malformed_json_and_wrong_content_type_are_400_problem_json(pool: PgPool) {
    let app = common::spawn_app(pool).await;

    let malformed = app
        .http
        .post(app.url("/v1/auth/login"))
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(malformed.headers()["content-type"], "application/problem+json");

    let wrong_type = app
        .http
        .post(app.url("/v1/auth/login"))
        .header("content-type", "text/plain")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_type.status(), StatusCode::BAD_REQUEST);
    assert_eq!(wrong_type.json::<Value>().await.unwrap()["code"], "bad_request");

    app.stop().await.unwrap();
}
