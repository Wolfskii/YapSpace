//! HTTP layer: extract, call domain, map to DTO.

use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::AppState;

pub mod auth;
pub mod dto;
pub mod extractors;
pub mod health;
pub mod me;
pub mod openapi;
pub mod users;

/// Every route. Each handler registers its path in the OpenAPI document through `routes!`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(openapi::ApiDoc::openapi())
        .routes(routes!(health::healthz))
        .routes(routes!(health::readyz))
        .routes(routes!(auth::register))
        .routes(routes!(auth::login))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::recover))
        .routes(routes!(me::get_me))
        .routes(routes!(me::change_password))
        .routes(routes!(me::rotate_recovery_code))
        .routes(routes!(me::list_sessions))
        .routes(routes!(me::revoke_session))
        .routes(routes!(users::get_user))
}
