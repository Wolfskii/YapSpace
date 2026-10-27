//! `/v1/discussions/{id}/messages` and `/v1/messages/{id}` (docs/architecture/api.md §F.2).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::client_ip::ClientIp;
use crate::{
    api::{
        dto::{MessageDto, message_dto},
        extractors::{ApiJson, ApiQuery, AuthUser, MaybeAuthUser},
    },
    app::AppState,
    domain::messages as service,
    error::AppError,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PostMessageRequest {
    /// 1-2000 characters. Line breaks are kept.
    pub body: String,
    /// The message this one replies to. It must be visible and in the same discussion.
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    /// Client-generated id, reused on retries so a message is never stored twice.
    pub client_msg_id: Uuid,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListMessagesQuery {
    /// Return messages with a seq below this value (older messages).
    pub before_seq: Option<i64>,
    /// Return messages with a seq above this value (newer messages).
    pub after_seq: Option<i64>,
    /// 1-100, default 50.
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessagePage {
    /// Ascending by seq.
    pub items: Vec<MessageDto>,
    pub has_more: bool,
    pub next_before_seq: Option<i64>,
    pub next_after_seq: Option<i64>,
}

/// Reads a page of messages.
#[utoipa::path(
    get,
    path = "/v1/discussions/{id}/messages",
    tag = "messages",
    params(("id" = Uuid, Path, description = "Discussion id"), ListMessagesQuery),
    responses(
        (status = 200, description = "A page of messages", body = MessagePage),
        (status = 400, description = "Bad cursor or limit"),
        (status = 404, description = "No such discussion"),
    )
)]
pub async fn list_messages(
    State(state): State<AppState>,
    _viewer: MaybeAuthUser,
    Path(id): Path<Uuid>,
    ApiQuery(query): ApiQuery<ListMessagesQuery>,
) -> Result<Json<MessagePage>, AppError> {
    let page = service::list(&state, id, query.before_seq, query.after_seq, query.limit).await?;
    Ok(Json(MessagePage {
        items: page.items.iter().map(message_dto).collect(),
        has_more: page.has_more,
        next_before_seq: page.next_before_seq,
        next_after_seq: page.next_after_seq,
    }))
}

/// Posts a message. Send the same `client_msg_id` again to retry. A retry returns the stored message with 200.
#[utoipa::path(
    post,
    path = "/v1/discussions/{id}/messages",
    tag = "messages",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Discussion id")),
    request_body = PostMessageRequest,
    responses(
        (status = 201, description = "Posted", body = MessageDto),
        (status = 200, description = "Already posted with this client_msg_id", body = MessageDto),
        (status = 401, description = "Not signed in"),
        (status = 403, description = "Account suspended or discussion locked"),
        (status = 404, description = "No such discussion"),
        (status = 422, description = "Validation failed or invalid parent"),
    )
)]
pub async fn post_message(
    State(state): State<AppState>,
    user: AuthUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<PostMessageRequest>,
) -> Result<(StatusCode, Json<MessageDto>), AppError> {
    let posted = service::post(
        &state,
        &user,
        ip,
        id,
        &body.body,
        body.parent_id,
        body.client_msg_id,
    )
    .await?;
    let status = if posted.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(message_dto(&posted.message))))
}

/// Reads one message.
#[utoipa::path(
    get,
    path = "/v1/messages/{id}",
    tag = "messages",
    params(("id" = Uuid, Path, description = "Message id")),
    responses(
        (status = 200, description = "The message", body = MessageDto),
        (status = 404, description = "No such message"),
    )
)]
pub async fn get_message(
    State(state): State<AppState>,
    _viewer: MaybeAuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<MessageDto>, AppError> {
    let row = service::get(&state, id).await?;
    Ok(Json(message_dto(&row)))
}

/// Deletes your own message. The body is erased and the message becomes a tombstone, so replies stay valid.
#[utoipa::path(
    delete,
    path = "/v1/messages/{id}",
    tag = "messages",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Message id")),
    responses(
        (status = 204, description = "Deleted (or already deleted)"),
        (status = 401, description = "Not signed in"),
        (status = 404, description = "No such message, or it is not yours"),
    )
)]
pub async fn delete_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state, &user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
