//! `/v1/mod/*`: moderator queue and actions (docs/architecture/api.md §F.2). Every route requires a moderator or admin.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::{
        dto::rfc3339_ms,
        extractors::{ApiJson, ApiQuery, ModUser},
    },
    app::AppState,
    db::reports::ReportRow,
    domain::moderation as service,
    error::AppError,
};

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueueQuery {
    /// Opaque cursor from the previous page.
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QueueItem {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QueuePage {
    pub items: Vec<QueueItem>,
    /// Pass this as `cursor` to get the next page. `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveRequest {
    /// `actioned` or `dismissed`.
    pub outcome: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasonRequest {
    #[serde(default)]
    pub reason: Option<String>,
    /// The report this action resolves, if any.
    #[serde(default)]
    pub report_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SuspendRequest {
    #[serde(default)]
    pub reason: Option<String>,
    /// When the suspension ends (RFC 3339). Omit for an indefinite suspension.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub report_id: Option<Uuid>,
}

fn queue_item(row: &ReportRow) -> QueueItem {
    QueueItem {
        id: row.id,
        reporter_id: row.reporter_id,
        target_type: row.target_type.clone(),
        target_id: row.target_id,
        reason: row.reason.clone(),
        details: row.details.clone(),
        status: row.status.clone(),
        created_at: row.created_at,
    }
}

/// Open reports, oldest first.
#[utoipa::path(
    get,
    path = "/v1/mod/reports",
    tag = "moderation",
    security(("session_token" = [])),
    params(QueueQuery),
    responses(
        (status = 200, description = "A page of open reports", body = QueuePage),
        (status = 400, description = "Bad cursor"),
        (status = 403, description = "Not a moderator"),
    )
)]
pub async fn queue(
    State(state): State<AppState>,
    _moderator: ModUser,
    ApiQuery(query): ApiQuery<QueueQuery>,
) -> Result<Json<QueuePage>, AppError> {
    let page = service::queue(&state, query.cursor.as_deref()).await?;
    Ok(Json(QueuePage {
        items: page.items.iter().map(queue_item).collect(),
        next_cursor: page.next_cursor,
    }))
}

/// Marks a report as actioned or dismissed.
#[utoipa::path(
    post,
    path = "/v1/mod/reports/{id}/resolve",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Report id")),
    request_body = ResolveRequest,
    responses(
        (status = 204, description = "Resolved"),
        (status = 403, description = "Not a moderator"),
        (status = 404, description = "No such report"),
        (status = 409, description = "Already resolved"),
    )
)]
pub async fn resolve_report(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<ResolveRequest>,
) -> Result<StatusCode, AppError> {
    service::resolve(&state, &moderator, id, &body.outcome, body.note.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a message from public view. The body is kept for appeals, and a job erases it after 90 days.
#[utoipa::path(
    post,
    path = "/v1/mod/messages/{id}/remove",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Message id")),
    request_body = ReasonRequest,
    responses(
        (status = 204, description = "Removed"),
        (status = 403, description = "Not a moderator"),
        (status = 404, description = "No such message"),
    )
)]
pub async fn remove_message(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<ReasonRequest>,
) -> Result<StatusCode, AppError> {
    service::remove_message(&state, &moderator, id, body.reason.as_deref(), body.report_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Restores a removed message.
#[utoipa::path(
    post,
    path = "/v1/mod/messages/{id}/restore",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Message id")),
    responses(
        (status = 204, description = "Restored"),
        (status = 403, description = "Not a moderator"),
        (status = 404, description = "No such message"),
    )
)]
pub async fn restore_message(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    service::restore_message(&state, &moderator, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Suspends an account. Moderators cannot suspend other moderators or admins. Suspended accounts can still read.
#[utoipa::path(
    post,
    path = "/v1/mod/users/{id}/suspend",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "User id")),
    request_body = SuspendRequest,
    responses(
        (status = 204, description = "Suspended"),
        (status = 403, description = "Not allowed for this target"),
        (status = 404, description = "No such user"),
    )
)]
pub async fn suspend_user(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<SuspendRequest>,
) -> Result<StatusCode, AppError> {
    service::suspend_user(
        &state,
        &moderator,
        id,
        body.reason.as_deref(),
        body.until,
        body.report_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Lifts a suspension.
#[utoipa::path(
    post,
    path = "/v1/mod/users/{id}/unsuspend",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "User id")),
    responses(
        (status = 204, description = "Unsuspended"),
        (status = 403, description = "Not allowed for this target"),
        (status = 404, description = "No such user"),
    )
)]
pub async fn unsuspend_user(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    service::unsuspend_user(&state, &moderator, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn discussion_status(
    state: &AppState,
    moderator: &crate::api::extractors::AuthUser,
    id: Uuid,
    status: &str,
    action: &str,
    body: &ReasonRequest,
) -> Result<StatusCode, AppError> {
    service::set_discussion_status(state, moderator, id, status, action, body.reason.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Locks a discussion: nobody can post until it is unlocked.
#[utoipa::path(
    post,
    path = "/v1/mod/discussions/{id}/lock",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Discussion id")),
    request_body = ReasonRequest,
    responses((status = 204, description = "Locked"), (status = 403, description = "Not a moderator"), (status = 404, description = "No such discussion"))
)]
pub async fn lock_discussion(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<ReasonRequest>,
) -> Result<StatusCode, AppError> {
    discussion_status(&state, &moderator, id, "locked", "lock_discussion", &body).await
}

/// Unlocks a discussion.
#[utoipa::path(
    post,
    path = "/v1/mod/discussions/{id}/unlock",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Discussion id")),
    request_body = ReasonRequest,
    responses((status = 204, description = "Unlocked"), (status = 403, description = "Not a moderator"), (status = 404, description = "No such discussion"))
)]
pub async fn unlock_discussion(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<ReasonRequest>,
) -> Result<StatusCode, AppError> {
    discussion_status(&state, &moderator, id, "open", "unlock_discussion", &body).await
}

/// Hides a discussion from everyone except moderators.
#[utoipa::path(
    post,
    path = "/v1/mod/discussions/{id}/hide",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Discussion id")),
    request_body = ReasonRequest,
    responses((status = 204, description = "Hidden"), (status = 403, description = "Not a moderator"), (status = 404, description = "No such discussion"))
)]
pub async fn hide_discussion(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<ReasonRequest>,
) -> Result<StatusCode, AppError> {
    discussion_status(&state, &moderator, id, "hidden", "hide_discussion", &body).await
}

/// Shows a hidden discussion again.
#[utoipa::path(
    post,
    path = "/v1/mod/discussions/{id}/unhide",
    tag = "moderation",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Discussion id")),
    request_body = ReasonRequest,
    responses((status = 204, description = "Shown again"), (status = 403, description = "Not a moderator"), (status = 404, description = "No such discussion"))
)]
pub async fn unhide_discussion(
    State(state): State<AppState>,
    ModUser(moderator): ModUser,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<ReasonRequest>,
) -> Result<StatusCode, AppError> {
    discussion_status(&state, &moderator, id, "open", "unhide_discussion", &body).await
}
