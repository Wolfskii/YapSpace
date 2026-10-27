//! `/v1/me/blocks`: the signed-in user's blocks (docs/architecture/api.md §F.2).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    api::{dto::rfc3339_ms, extractors::AuthUser},
    app::AppState,
    domain::blocks as service,
    error::AppError,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct BlockDto {
    pub username: String,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BlockList {
    pub items: Vec<BlockDto>,
}

/// Lists the users you have blocked.
#[utoipa::path(
    get,
    path = "/v1/me/blocks",
    tag = "me",
    security(("session_token" = [])),
    responses(
        (status = 200, description = "Blocked users", body = BlockList),
        (status = 401, description = "Not signed in"),
    )
)]
pub async fn list(State(state): State<AppState>, user: AuthUser) -> Result<Json<BlockList>, AppError> {
    let rows = service::list(&state, &user).await?;
    Ok(Json(BlockList {
        items: rows
            .into_iter()
            .map(|row| BlockDto {
                username: row.username,
                created_at: row.created_at,
            })
            .collect(),
    }))
}

/// Blocks a user. Repeating the call does nothing.
#[utoipa::path(
    put,
    path = "/v1/me/blocks/{username}",
    tag = "me",
    security(("session_token" = [])),
    params(("username" = String, Path, description = "Username to block")),
    responses(
        (status = 204, description = "Blocked"),
        (status = 401, description = "Not signed in"),
        (status = 404, description = "No such user"),
        (status = 409, description = "Block limit reached"),
        (status = 422, description = "You cannot block yourself"),
    )
)]
pub async fn block(
    State(state): State<AppState>,
    user: AuthUser,
    Path(username): Path<String>,
) -> Result<StatusCode, AppError> {
    service::block(&state, &user, &username).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a block.
#[utoipa::path(
    delete,
    path = "/v1/me/blocks/{username}",
    tag = "me",
    security(("session_token" = [])),
    params(("username" = String, Path, description = "Username to unblock")),
    responses(
        (status = 204, description = "Unblocked (or was not blocked)"),
        (status = 401, description = "Not signed in"),
    )
)]
pub async fn unblock(
    State(state): State<AppState>,
    user: AuthUser,
    Path(username): Path<String>,
) -> Result<StatusCode, AppError> {
    service::unblock(&state, &user, &username).await?;
    Ok(StatusCode::NO_CONTENT)
}
