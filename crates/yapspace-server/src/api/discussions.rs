//! `/v1/discussions/*`: resolve a page, create its discussion, read a discussion (docs/architecture/api.md §F.2).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{
        dto::{DiscussionDto, IdentityDto, discussion_dto},
        extractors::{ApiJson, AuthUser, MaybeAuthUser},
    },
    app::AppState,
    domain::discussions as service,
    error::AppError,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveRequest {
    /// The URL the browser is showing.
    #[schema(example = "https://example.com/article?utm_source=x")]
    pub url: String,
    /// The page's `<link rel="canonical">`, if it has one.
    #[serde(default)]
    pub canonical_url: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateDiscussionRequest {
    pub url: String,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResolveResponse {
    pub identity: IdentityDto,
    /// `null` when nobody has started a discussion for this page yet.
    pub discussion: Option<DiscussionDto>,
}

/// Looks up the discussion for a page. This never creates one.
#[utoipa::path(
    post,
    path = "/v1/discussions/resolve",
    tag = "discussions",
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Identity and the discussion if one exists", body = ResolveResponse),
        (status = 422, description = "URL not supported"),
    )
)]
pub async fn resolve(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ResolveRequest>,
) -> Result<Json<ResolveResponse>, AppError> {
    let resolution = service::resolve(&state, &body.url, body.canonical_url.as_deref()).await?;
    let identity = IdentityDto {
        kind: resolution.identity.kind.as_str().to_owned(),
        display_url: resolution.identity.display_url.clone(),
        site: resolution.identity.site.clone(),
        ruleset: yapspace_identity::RULESET,
    };
    let discussion = resolution
        .discussion
        .as_ref()
        .map(|row| discussion_dto(row, resolution.merged_from));
    Ok(Json(ResolveResponse { identity, discussion }))
}

/// Creates the discussion for a page, or returns the existing one (200).
#[utoipa::path(
    post,
    path = "/v1/discussions",
    tag = "discussions",
    security(("session_token" = [])),
    request_body = CreateDiscussionRequest,
    responses(
        (status = 201, description = "Created", body = DiscussionDto),
        (status = 200, description = "Already existed", body = DiscussionDto),
        (status = 401, description = "Not signed in"),
        (status = 403, description = "Account suspended"),
        (status = 422, description = "URL not supported"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(body): ApiJson<CreateDiscussionRequest>,
) -> Result<(StatusCode, Json<DiscussionDto>), AppError> {
    user.ensure_can_write()?;
    let (row, created) = service::create(
        &state,
        user.user_id,
        &body.url,
        body.canonical_url.as_deref(),
        body.title.as_deref(),
    )
    .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(discussion_dto(&row, None))))
}

/// Reads a discussion. A merged discussion returns its target, and `merged_from` names the id that was asked for.
#[utoipa::path(
    get,
    path = "/v1/discussions/{id}",
    tag = "discussions",
    security(("session_token" = [])),
    params(("id" = Uuid, Path, description = "Discussion id")),
    responses(
        (status = 200, description = "The discussion", body = DiscussionDto),
        (status = 404, description = "No such discussion"),
    )
)]
pub async fn get_discussion(
    State(state): State<AppState>,
    _viewer: MaybeAuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DiscussionDto>, AppError> {
    let (row, merged_from) = service::get(&state, id).await?;
    Ok(Json(discussion_dto(&row, merged_from)))
}
