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
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test port");
    let addr = listener.local_addr().expect("local addr");
    let state = AppState::with_pool(test_config(), pool);
    let shutdown = state.shutdown.clone();
    let server = tokio::spawn(yapspace_server::app::serve_on(state, listener));
    TestApp {
        addr,
        http: reqwest::Client::new(),
        shutdown,
        server,
    }
}
