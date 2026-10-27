//! `/v1/users/{username}`: public profiles (docs/architecture/api.md §F.2).

use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    api::dto::{PublicUserDto, public_user_dto},
    app::AppState,
    db::users,
    domain::validation::normalize_username,
    error::AppError,
};

/// A public profile: id, username and creation time.
#[utoipa::path(
    get,
    path = "/v1/users/{username}",
    tag = "users",
    params(("username" = String, Path, description = "Username, case-insensitive")),
    responses(
        (status = 200, description = "Public profile", body = PublicUserDto),
        (status = 404, description = "No such user"),
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<PublicUserDto>, AppError> {
    let Ok(username) = normalize_username(&username) else {
        return Err(AppError::NotFound);
    };
    let row = users::find_by_username(&state.db, &username)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(public_user_dto(&row)))
}
