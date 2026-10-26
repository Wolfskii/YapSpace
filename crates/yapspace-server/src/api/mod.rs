//! HTTP layer: extract, call domain, map to DTO.

use axum::{Router, routing::get};

use crate::app::AppState;

pub mod extractors;
pub mod health;

/// Routes that are not under `/v1`. Real API routes are added in Phase 2.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
}
