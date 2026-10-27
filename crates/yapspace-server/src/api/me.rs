//! `/v1/me/*`: the signed-in user's profile, password, recovery code and sessions (docs/architecture/api.md §F.2).

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{
        dto::{UserDto, rfc3339_ms, user_dto},
        extractors::{ApiJson, AuthUser},
    },
    app::AppState,
    db::account::{self, ExportMessage},
    db::{sessions, users},
    domain::{account as account_service, auth as service},
    error::AppError,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CurrentPasswordRequest {
    pub current_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecoveryCodeResponse {
    pub recovery_code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionDto {
    pub id: Uuid,
    pub client_label: Option<String>,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub last_used_at: DateTime<Utc>,
    /// True for the session that made this request.
    pub current: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionListResponse {
    pub items: Vec<SessionDto>,
}

/// The signed-in user's profile.
#[utoipa::path(
    get,
    path = "/v1/me",
    tag = "me",
    security(("session_token" = [])),
    responses(
        (status = 200, description = "Own profile", body = UserDto),
        (status = 401, description = "Not signed in"),
    )
)]
pub async fn get_me(State(state): State<AppState>, user: AuthUser) -> Result<Json<UserDto>, AppError> {
    let row = users::find_by_id(&state.db, user.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(user_dto(&row)))
}

/// Changes the password. Every other session is revoked, and the current one stays signed in.
#[utoipa::path(
    post,
    path = "/v1/me/password",
    tag = "me",
    security(("session_token" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password changed"),
        (status = 401, description = "Current password is wrong"),
        (status = 422, description = "New password is invalid"),
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(body): ApiJson<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    user.ensure_can_write()?;
    service::change_password(
        &state,
        user.user_id,
        &user.username,
        user.session_id,
        &body.current_password,
        &body.new_password,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Replaces the recovery code. The password must be confirmed first.
#[utoipa::path(
    post,
    path = "/v1/me/recovery-code",
    tag = "me",
    security(("session_token" = [])),
    request_body = CurrentPasswordRequest,
    responses(
        (status = 200, description = "New recovery code", body = RecoveryCodeResponse),
        (status = 401, description = "Current password is wrong"),
    )
)]
pub async fn rotate_recovery_code(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(body): ApiJson<CurrentPasswordRequest>,
) -> Result<Json<RecoveryCodeResponse>, AppError> {
    user.ensure_can_write()?;
    let code = service::rotate_recovery_code(&state, user.user_id, &body.current_password).await?;
    Ok(Json(RecoveryCodeResponse { recovery_code: code }))
}

/// Lists the active sessions, most recently used first.
#[utoipa::path(
    get,
    path = "/v1/me/sessions",
    tag = "me",
    security(("session_token" = [])),
    responses(
        (status = 200, description = "Active sessions", body = SessionListResponse),
        (status = 401, description = "Not signed in"),
    )
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<SessionListResponse>, AppError> {
    let rows = sessions::list_for_user(&state.db, user.user_id).await?;
    let items = rows
        .into_iter()
        .map(|row| SessionDto {
            current: row.id == user.session_id,
            id: row.id,
            client_label: row.client_label,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        })
        .collect();
    Ok(Json(SessionListResponse { items }))
}

/// Revokes one of your own sessions. Someone else's session id returns 404, so ids cannot be probed.
#[utoipa::path(
    delete,
    path = "/v1/me/sessions/{id}",
    tag = "me",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Session id")),
    responses(
        (status = 204, description = "Session revoked"),
        (status = 404, description = "No such session"),
    )
)]
pub async fn revoke_session(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if sessions::revoke_for_user(&state.db, user.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteAccountRequest {
    /// Confirms the deletion. A deleted account cannot be recovered.
    pub password: String,
}

/// Deletes the account. Sessions end now, and message bodies are erased within a few minutes by a background job.
#[utoipa::path(
    delete,
    path = "/v1/me",
    tag = "me",
    security(("session_token" = [])),
    request_body = DeleteAccountRequest,
    responses(
        (status = 202, description = "Account deletion accepted"),
        (status = 401, description = "Password is wrong"),
    )
)]
pub async fn delete_account(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(body): ApiJson<DeleteAccountRequest>,
) -> Result<StatusCode, AppError> {
    account_service::delete(&state, &user, &body.password).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Messages per database read during export.
const EXPORT_BATCH: i64 = 500;

fn export_message(message: &ExportMessage) -> serde_json::Value {
    serde_json::json!({
        "id": message.id,
        "created_at": message.created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "discussion": message.discussion_display_url,
        "seq": message.seq,
        "body": message.body,
        "status": message.status,
        "edited_at": message.edited_at.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    })
}

/// Streams a JSON export of the account: profile, sessions and every message, in batches.
#[utoipa::path(
    get,
    path = "/v1/me/export",
    tag = "me",
    security(("session_token" = [])),
    responses(
        (status = 200, description = "JSON export", content_type = "application/json"),
        (status = 401, description = "Not signed in"),
    )
)]
pub async fn export(State(state): State<AppState>, user: AuthUser) -> Result<Response, AppError> {
    let profile = users::find_by_id(&state.db, user.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let sessions_rows = sessions::list_for_user(&state.db, user.user_id).await?;
    let sessions_json: Vec<serde_json::Value> = sessions_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "client_label": row.client_label,
                "created_at": row.created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "last_used_at": row.last_used_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            })
        })
        .collect();
    let header = serde_json::json!({
        "profile": user_dto(&profile),
        "sessions": sessions_json,
    })
    .to_string();
    // Reopen the object so the messages array can be appended.
    let opening = format!("{},\"messages\":[", &header[..header.len() - 1]);

    // State: the next cursor, whether the opening was sent, whether a message was written, and whether we are done.
    let pages = stream::unfold(
        (state.clone(), user.user_id, Some(opening), None, false, false),
        |(state, user_id, opening, cursor, wrote_any, done)| async move {
            if let Some(opening) = opening {
                return Some((
                    Ok::<Bytes, std::io::Error>(Bytes::from(opening)),
                    (state, user_id, None, cursor, wrote_any, done),
                ));
            }
            if done {
                return None;
            }
            match account::export_page(&state.db, user_id, cursor, EXPORT_BATCH).await {
                Err(err) => {
                    tracing::error!(error = %err, "export failed part way");
                    Some((
                        Err(std::io::Error::other("export failed")),
                        (state, user_id, None, None, wrote_any, true),
                    ))
                }
                Ok(rows) if rows.is_empty() => Some((
                    Ok(Bytes::from_static(b"]}")),
                    (state, user_id, None, None, wrote_any, true),
                )),
                Ok(rows) => {
                    let last = rows.last().map(|m| (m.created_at, m.id));
                    let mut chunk = String::new();
                    for (index, message) in rows.iter().enumerate() {
                        if wrote_any || index > 0 {
                            chunk.push(',');
                        }
                        chunk.push_str(&export_message(message).to_string());
                    }
                    Some((Ok(Bytes::from(chunk)), (state, user_id, None, last, true, false)))
                }
            }
        },
    );

    let mut response = Body::from_stream(pages).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/json".parse().expect("static header"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"yapspace-export.json\""
            .parse()
            .expect("static header"),
    );
    Ok(response)
}
