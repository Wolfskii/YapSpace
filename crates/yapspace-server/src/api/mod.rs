//! HTTP layer: extract, call domain, map to DTO.

use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::AppState;

pub mod extractors;
pub mod health;
pub mod openapi;

/// Routes that are not under `/v1`. Real API routes are added in Phase 2. Each handler registers its path in the
/// OpenAPI document through `routes!`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(openapi::ApiDoc::openapi())
        .routes(routes!(health::healthz))
        .routes(routes!(health::readyz))
}
