//! Application state, the router with its middleware stack, the server lifecycle, and the healthcheck client.
//! See docs/architecture/overview.md §C.1 and docs/architecture/realtime.md §G.3 (shutdown).

use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderName, StatusCode, header},
    middleware::{self, Next},
    response::Response,
};
use serde_json::json;
use sqlx::PgPool;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnFailure, TraceLayer},
};

use crate::{api, config::Config, db, error::AppError, telemetry};

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_REQUEST_ID_LEN: usize = 64;
const PROBLEM_BODY_LIMIT: usize = 64 * 1024;

/// Shared state for every handler. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Cancelled when shutdown starts. Readiness fails from that moment, and WebSockets close with 1012 (Phase 3).
    pub shutdown: CancellationToken,
    /// Background tasks. Shutdown waits for them, up to the grace period.
    pub tasks: TaskTracker,
}

impl AppState {
    /// Builds the state. The pool connects lazily, so the server can start while the database is still down.
    /// `/readyz` then reports 503 until the database answers.
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let db = db::pool(&config)?;
        Ok(Self::with_pool(config, db))
    }

    /// Builds the state around an existing pool. Tests use this with the pool that `sqlx::test` provides.
    pub fn with_pool(config: Config, db: PgPool) -> Self {
        Self {
            db,
            config: Arc::new(config),
            shutdown: CancellationToken::new(),
            tasks: TaskTracker::new(),
        }
    }

    pub fn is_draining(&self) -> bool {
        self.shutdown.is_cancelled()
    }
}

/// Builds the router. Layers are listed innermost first. The last `.layer` call is the outermost layer.
pub fn build_router(state: AppState) -> Router {
    api::router()
        .route_layer(middleware::from_fn(telemetry::http_metrics))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .fallback(not_found)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                // 5xx is expected during outages (for example readyz 503), so log it as a warning, not an error.
                .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
        )
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
        .layer(SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestUuid))
        .layer(middleware::from_fn(sanitize_request_id))
        .layer(middleware::from_fn(fill_request_id))
        .layer(CatchPanicLayer::new())
        .with_state(state)
}

async fn not_found() -> AppError {
    AppError::NotFound
}

/// Span for each request. It carries the method and request id only, never the URI, because the URI may hold
/// sensitive query strings.
fn request_span(request: &Request) -> tracing::Span {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    tracing::info_span!("request", method = %request.method(), request_id = %request_id)
}

/// Drops an incoming `x-request-id` unless it is 1–64 characters of `[A-Za-z0-9-]`. Then `SetRequestIdLayer`
/// generates a fresh UUID. This stops clients from injecting arbitrary text into logs.
async fn sanitize_request_id(mut request: Request, next: Next) -> Response {
    let valid = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            (1..=MAX_REQUEST_ID_LEN).contains(&v.len())
                && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    if !valid {
        request.headers_mut().remove(&REQUEST_ID_HEADER);
    }
    next.run(request).await
}

/// Copies the request id into `problem+json` bodies. Handlers cannot see the request id, so it is added here.
async fn fill_request_id(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes() == b"application/problem+json");
    if !is_problem {
        return response;
    }

    let request_id = response
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, PROBLEM_BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let Ok(mut problem) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    if let Some(id) = request_id {
        problem["request_id"] = json!(id);
    }
    // The body length changed, so drop the old length and let hyper set the new one.
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(
        parts,
        Body::from(serde_json::to_vec(&problem).unwrap_or_default()),
    )
}

/// Binds `config.bind_addr` and serves until shutdown is requested (SIGINT, SIGTERM, or cancellation of
/// `state.shutdown`).
pub async fn serve(state: AppState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(state.config.bind_addr).await?;
    tokio::spawn(shutdown_on_signal(state.shutdown.clone()));
    serve_on(state, listener).await
}

/// Serves HTTP on an already-bound listener until shutdown. Tests bind to port 0 and call this directly.
///
/// Shutdown order: stop accepting connections, let in-flight requests finish, wait up to the grace period for
/// background tasks, then close the database pool.
pub async fn serve_on(state: AppState, listener: TcpListener) -> anyhow::Result<()> {
    tracing::info!(addr = %listener.local_addr()?, "listening");
    state
        .tasks
        .spawn(db::pool_metrics_loop(state.db.clone(), state.shutdown.clone()));

    let token = state.shutdown.clone();
    let tasks = state.tasks.clone();
    let db = state.db.clone();
    let grace = state.config.shutdown_grace;
    let router = build_router(state);

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move { token.cancelled().await })
    .await?;

    tasks.close();
    if tokio::time::timeout(grace, tasks.wait()).await.is_err() {
        tracing::warn!("background tasks did not finish within the shutdown grace period");
    }
    db.close().await;
    tracing::info!("shutdown complete");
    Ok(())
}

async fn shutdown_on_signal(token: CancellationToken) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutdown signal received");
    token.cancel();
}

/// Checks `/healthz` on the local server. Used by container healthchecks, because the release image has no curl.
pub async fn run_healthcheck(bind: SocketAddr) -> anyhow::Result<()> {
    let target = if bind.ip().is_unspecified() {
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), bind.port())
    } else {
        bind
    };
    let exchange = async {
        let mut stream = TcpStream::connect(target).await?;
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await?;
        Ok::<_, std::io::Error>(response)
    };
    let response = tokio::time::timeout(Duration::from_secs(3), exchange).await??;
    let text = String::from_utf8_lossy(&response);
    if text.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        anyhow::bail!(
            "healthz did not return 200: {}",
            text.lines().next().unwrap_or("no response")
        )
    }
}
