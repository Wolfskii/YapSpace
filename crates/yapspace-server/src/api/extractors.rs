//! Request extractors: JSON bodies, and the authenticated user (docs/architecture/api.md §F.1).

use axum::{
    Json,
    extract::{FromRef, FromRequest, FromRequestParts, Request, rejection::JsonRejection},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::{
    app::AppState,
    db::sessions,
    domain::{
        auth::{Role, is_suspended},
        tokens,
    },
    error::AppError,
};

/// JSON body extractor. Every failure is reported in problem+json form (see docs/architecture/api.md §F.1).
#[derive(Debug)]
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(map_rejection(rejection)),
        }
    }
}

fn map_rejection(rejection: JsonRejection) -> AppError {
    match rejection.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => {
            AppError::BadRequest("content-type must be application/json".into())
        }
        _ => AppError::BadRequest(rejection.body_text()),
    }
}

/// An authenticated request. Extracting it checks the bearer token, and extends the session when it is due.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub username: String,
    pub role: Role,
    pub suspended: bool,
}

impl AuthUser {
    /// Writes are refused for suspended accounts. Reads still work, so a user can see why they were suspended.
    pub fn ensure_can_write(&self) -> Result<(), AppError> {
        if self.suspended {
            Err(AppError::AccountSuspended)
        } else {
            Ok(())
        }
    }
}

/// Reads the bearer token from the request. Returns `None` if there is no Authorization header.
fn bearer_token(parts: &Parts) -> Result<Option<&str>, AppError> {
    let Some(value) = parts.headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let text = value.to_str().map_err(|_| AppError::Unauthorized)?;
    let token = text.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
    if !token.starts_with(tokens::TOKEN_PREFIX) || token.len() > 64 {
        return Err(AppError::Unauthorized);
    }
    Ok(Some(token))
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let token = bearer_token(parts)?.ok_or(AppError::Unauthorized)?;
        let hash = tokens::hash_token(token);
        let row = sessions::find_active_by_token_hash(&state.db, &hash)
            .await?
            .ok_or(AppError::Unauthorized)?;

        // Touching the session is best effort, and it runs in the background so the request does not wait for it.
        let pool = state.db.clone();
        let session_id = row.session_id;
        tokio::spawn(async move {
            if let Err(err) = sessions::touch(&pool, session_id).await {
                tracing::warn!(error = %err, "session touch failed");
            }
        });

        Ok(AuthUser {
            user_id: row.user_id,
            session_id: row.session_id,
            username: row.username,
            role: Role::parse(&row.role),
            suspended: is_suspended(&row.status, row.suspended_until),
        })
    }
}

/// Optional authentication. No header means anonymous. A header with a bad token is still an error, so clients notice
/// an expired session instead of quietly reading as anonymous.
#[derive(Debug, Clone)]
pub struct MaybeAuthUser(pub Option<AuthUser>);

impl<S> FromRequestParts<S> for MaybeAuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if parts.headers.get(AUTHORIZATION).is_none() {
            return Ok(Self(None));
        }
        Ok(Self(Some(AuthUser::from_request_parts(parts, state).await?)))
    }
}

/// Requires a moderator or admin. Suspended accounts cannot moderate.
#[derive(Debug, Clone)]
pub struct ModUser(pub AuthUser);

impl<S> FromRequestParts<S> for ModUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.role.is_moderator() {
            return Err(AppError::Forbidden);
        }
        user.ensure_can_write()?;
        Ok(Self(user))
    }
}

/// Requires an admin.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl<S> FromRequestParts<S> for AdminUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.role.is_admin() {
            return Err(AppError::Forbidden);
        }
        user.ensure_can_write()?;
        Ok(Self(user))
    }
}

/// Query string extractor with problem+json errors (api.md §F.1), like `ApiJson`.
#[derive(Debug)]
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(|err| AppError::BadRequest(err.to_string()))
    }
}
