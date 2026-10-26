//! Health endpoints. `/healthz` checks only that the process answers. `/readyz` also checks the database.

use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::app::AppState;

const DB_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// Liveness: 200 while the process is serving, 503 once shutdown has started.
#[utoipa::path(
    get,
    path = "/healthz",
    tag = "health",
    responses(
        (status = 200, description = "The process is serving"),
        (status = 503, description = "Shutdown has started"),
    )
)]
pub async fn healthz(State(state): State<AppState>) -> Response {
    if state.is_draining() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "draining"})),
        )
            .into_response();
    }
    Json(json!({"status": "ok"})).into_response()
}

/// Readiness: 200 when the database answers within 2 s and the process is not draining, else 503.
#[utoipa::path(
    get,
    path = "/readyz",
    tag = "health",
    responses(
        (status = 200, description = "The database answers and the process is not draining"),
        (status = 503, description = "Not ready: the database is unreachable or shutdown has started"),
    )
)]
pub async fn readyz(State(state): State<AppState>) -> Response {
    if state.is_draining() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "draining"})),
        )
            .into_response();
    }
    let check = tokio::time::timeout(
        DB_CHECK_TIMEOUT,
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db),
    )
    .await;
    match check {
        Ok(Ok(_)) => Json(json!({"status": "ok", "database": "ok"})).into_response(),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "unavailable", "database": "unavailable"})),
        )
            .into_response(),
    }
}
