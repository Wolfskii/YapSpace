//! Shared integration-test harness. Starts the real server on 127.0.0.1 with a random port.
//!
//! Used by test files with `mod common;`. Some helpers are not used by every test binary, so dead-code warnings
//! are allowed here.
#![allow(dead_code)]

use std::net::SocketAddr;

use sqlx::PgPool;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use yapspace_server::{app::AppState, config::Config};

pub struct TestApp {
    pub addr: SocketAddr,
    pub http: reqwest::Client,
    pub shutdown: CancellationToken,
    pub server: JoinHandle<anyhow::Result<()>>,
}

impl TestApp {
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.http
            .get(self.url(path))
            .send()
            .await
            .expect("request to test server")
    }

    /// Requests shutdown and waits for the server task to finish. Returns its result.
    pub async fn stop(self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        self.server.await.expect("server task panicked")
    }
}

/// Test configuration. The database URL is a placeholder, because tests pass in their own pool.
pub fn test_config() -> Config {
    Config::from_lookup(|key| match key {
        "YAPSPACE_DATABASE_URL" => Some("postgres://unused@127.0.0.1:1/unused".into()),
        "YAPSPACE_SHUTDOWN_GRACE_SECS" => Some("2".into()),
        _ => None,
    })
    .expect("test config is valid")
}

/// Starts the server with `pool` as its database. Returns once the listener is bound.
pub async fn spawn_app(pool: PgPool) -> TestApp {
    spawn_app_with(pool, test_config()).await
}

/// Starts the server with a custom configuration.
pub async fn spawn_app_with(pool: PgPool, config: Config) -> TestApp {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test port");
    let addr = listener.local_addr().expect("local addr");
    let state = AppState::with_pool(config, pool);
    let shutdown = state.shutdown.clone();
    let server = tokio::spawn(yapspace_server::app::serve_on(state, listener));
    TestApp {
        addr,
        http: reqwest::Client::new(),
        shutdown,
        server,
    }
}

/// Registers a user through the API. Returns the session token, the user JSON and the recovery code.
pub async fn register(app: &TestApp, username: &str, password: &str) -> (String, serde_json::Value, String) {
    let response = app
        .http
        .post(app.url("/v1/auth/register"))
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("register request");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::CREATED,
        "register {username}"
    );
    let body: serde_json::Value = response.json().await.expect("register json");
    (
        body["session_token"].as_str().expect("token").to_owned(),
        body["user"].clone(),
        body["recovery_code"].as_str().expect("recovery code").to_owned(),
    )
}

/// Sends an authenticated request.
pub fn authed(app: &TestApp, method: reqwest::Method, path: &str, token: &str) -> reqwest::RequestBuilder {
    app.http.request(method, app.url(path)).bearer_auth(token)
}
