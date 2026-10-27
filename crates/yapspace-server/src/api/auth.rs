//! `/v1/auth/*`: register, log in, log out, recover (docs/architecture/api.md §F.2).

use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    api::{
        dto::{AuthResponse, user_dto},
        extractors::{ApiJson, AuthUser},
    },
    app::AppState,
    domain::auth as service,
    error::AppError,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    /// 3-24 characters: a-z, 0-9 or _. Case-insensitive.
    #[schema(example = "alice_01")]
    pub username: String,
    /// 10-128 characters. Must not equal the username.
    #[schema(example = "correct horse battery")]
    pub password: String,
    /// Shown in the session list, for example "Chrome extension".
    #[serde(default)]
    pub client_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub client_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecoverRequest {
    pub username: String,
    /// The recovery code, in any case, with or without dashes.
    pub recovery_code: String,
    pub new_password: String,
    #[serde(default)]
    pub client_label: Option<String>,
}

fn auth_response(issued: service::Issued) -> AuthResponse {
    AuthResponse {
        user: user_dto(&issued.user),
        session_token: issued.session_token,
        recovery_code: issued.recovery_code,
    }
}

/// Creates an account and returns a session and the recovery code (shown once).
#[utoipa::path(
    post,
    path = "/v1/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created", body = AuthResponse),
        (status = 403, description = "Registration is closed"),
        (status = 409, description = "Username taken"),
        (status = 422, description = "Validation failed"),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    let issued = service::register(
        &state,
        &body.username,
        &body.password,
        body.client_label.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(auth_response(issued))))
}

/// Signs in with a username and password.
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in", body = AuthResponse),
        (status = 401, description = "Invalid credentials"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let issued = service::login(
        &state,
        &body.username,
        &body.password,
        body.client_label.as_deref(),
    )
    .await?;
    Ok(Json(auth_response(issued)))
}

/// Revokes the session that made the request.
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    tag = "auth",
    security(("session_token" = [])),
    responses(
        (status = 204, description = "Signed out"),
        (status = 401, description = "Not signed in"),
    )
)]
pub async fn logout(State(state): State<AppState>, user: AuthUser) -> Result<StatusCode, AppError> {
    service::logout(&state, user.session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resets the password with a recovery code. Every session is revoked, and a new recovery code is issued.
#[utoipa::path(
    post,
    path = "/v1/auth/recover",
    tag = "auth",
    request_body = RecoverRequest,
    responses(
        (status = 200, description = "Password reset", body = AuthResponse),
        (status = 401, description = "Invalid credentials or recovery code"),
        (status = 422, description = "Validation failed"),
    )
)]
pub async fn recover(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RecoverRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let issued = service::recover(
        &state,
        &body.username,
        &body.recovery_code,
        &body.new_password,
        body.client_label.as_deref(),
    )
    .await?;
    Ok(Json(auth_response(issued)))
}
